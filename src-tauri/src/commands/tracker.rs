use crate::db::{
    add_app_time, add_screen_time, cleanup_old_data, get_daily_total, load_settings, today_str,
    AppState,
};
use crate::platform::{get_foreground_app, should_count_screen_time, should_count_time};
use chrono::{DateTime, Duration as ChronoDuration, Local, Timelike};
use serde_json::json;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

use super::reports::format_duration;
use super::todos::{check_pending_recurrences, mark_due_reminder_sent};

pub fn start_tracker(app: AppHandle, state: AppState) {
    crate::logging::spawn_named("tempo-tracker", move || {
        let mut tick_count: u64 = 0;
        let mut last_tick = Instant::now();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let elapsed_seconds = last_tick.elapsed().as_secs().clamp(1, 5) as i64;
            last_tick = Instant::now();
            tick_count += elapsed_seconds as u64;

            let now = Local::now();
            let date = now.format("%Y-%m-%d").to_string();

            {
                let mut tracker = state.tracker.lock();
                if tracker.last_date != date {
                    tracker.continuous_seconds = 0;
                    tracker.night_reminded_today = false;
                    tracker.last_date = date.clone();
                }
            }

            if !should_count_time() {
                state.tracker.lock().continuous_seconds = 0;
                continue;
            }

            let foreground = get_foreground_app();

            if !should_count_screen_time(&foreground) {
                state.tracker.lock().continuous_seconds = 0;
                continue;
            }

            let foreground_icon = foreground.as_ref().and_then(|app_info| {
                crate::app_icons::resolve_app_icon_storage_key(
                    &app,
                    &app_info.name,
                    &app_info.process_name,
                )
            });

            {
                let conn = state.db.lock();
                for (bucket_date, bucket_hour, seconds) in second_buckets(now, elapsed_seconds) {
                    let tracked_seconds =
                        add_screen_time(&conn, &bucket_date, bucket_hour, seconds);
                    if tracked_seconds <= 0 {
                        continue;
                    }

                    if let Some(ref app_info) = foreground {
                        add_app_time(
                            &conn,
                            &bucket_date,
                            &app_info.name,
                            &app_info.process_name,
                            tracked_seconds,
                            foreground_icon.as_deref(),
                        );
                    }
                }
            }

            {
                let mut tracker = state.tracker.lock();
                tracker.continuous_seconds += elapsed_seconds;
            }

            if tick_count % 5 == 0 {
                update_tray_tooltip(&app, &state);
            }

            if tick_count % 60 == 0 {
                let conn = state.db.lock();
                cleanup_old_data(&conn);
            }

            check_reminders(&app, &state);
            check_todo_due_reminders(&app, &state);
            if tick_count % 60 == 0 {
                check_pending_recurrences(&app, &state);
            }
            crate::pomodoro::tick_pomodoro(&app, &state, elapsed_seconds);
        }
    });
}

pub(crate) fn second_buckets(now: DateTime<Local>, seconds: i64) -> Vec<(String, u32, i64)> {
    let mut buckets: Vec<(String, u32, i64)> = Vec::new();

    for offset in 1..=seconds {
        let point = now - ChronoDuration::seconds(offset);
        let date = point.format("%Y-%m-%d").to_string();
        let hour = point.hour();

        if let Some((_, _, total)) = buckets
            .iter_mut()
            .find(|(bucket_date, bucket_hour, _)| bucket_date == &date && *bucket_hour == hour)
        {
            *total += 1;
        } else {
            buckets.push((date, hour, 1));
        }
    }

    buckets
}

fn update_tray_tooltip(app: &AppHandle, state: &AppState) {
    let today = {
        let conn = state.db.lock();
        get_daily_total(&conn, &today_str())
    };
    let tooltip = format!("今日屏幕时长: {}", format_duration(today));
    let app_handle = app.clone();

    if let Err(error) = app.run_on_main_thread(move || {
        if let Some(tray) = app_handle.tray_by_id("main") {
            crate::logging::debug_if_err(tray.set_tooltip(Some(&tooltip)), "update tray tooltip");
        }
    }) {
        tracing::warn!(error = %error, "failed to dispatch tray tooltip update");
    }
}

pub(crate) fn emit_on_main(app: &AppHandle, event: &str, payload: serde_json::Value) {
    let app_handle = app.clone();
    let event = event.to_string();
    let event_for_log = event.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        crate::logging::debug_if_err(app_handle.emit(&event, payload), "emit app event");
    }) {
        tracing::warn!(event = %event_for_log, error = %error, "failed to dispatch app event");
    }
}

fn check_reminders(app: &AppHandle, state: &AppState) {
    let settings = {
        let conn = state.db.lock();
        load_settings(&conn)
    };

    let continuous = state.tracker.lock().continuous_seconds;

    if settings.eye_care_enabled {
        let interval = (settings.eye_care_interval_minutes as i64) * 60;
        if interval > 0 && continuous >= interval {
            emit_on_main(app, "reminder", json!({ "type": "eye_care" }));
            state.tracker.lock().continuous_seconds = 0;
        }
    }

    if settings.night_reminder_enabled {
        let now = Local::now().time();
        let in_range = is_in_night_range(
            &now.format("%H:%M").to_string(),
            &settings.night_reminder_start,
            &settings.night_reminder_end,
        );

        let should_notify = {
            let mut tracker = state.tracker.lock();
            if in_range && !tracker.night_reminded_today {
                tracker.night_reminded_today = true;
                true
            } else {
                false
            }
        };

        if should_notify {
            emit_on_main(app, "reminder", json!({ "type": "night" }));
        }
    }
}

fn check_todo_due_reminders(app: &AppHandle, state: &AppState) {
    let now = Local::now();
    let reminders = {
        let conn = state.db.lock();
        let mut stmt = match conn.prepare(
            "SELECT id, title, due_at, remind_1d, remind_1h, remind_custom_hours,
                    due_reminded_1d, due_reminded_1h, due_reminded_custom, due_reminded_at
             FROM todos
             WHERE completed = 0 AND due_at IS NOT NULL",
        ) {
            Ok(stmt) => stmt,
            Err(error) => {
                tracing::warn!(error = %error, "failed to prepare todo due reminder query");
                return;
            }
        };

        let rows = match stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
                row.get::<_, i64>(4)? != 0,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, i64>(6)? != 0,
                row.get::<_, i64>(7)? != 0,
                row.get::<_, i64>(8)? != 0,
                row.get::<_, i64>(9)? != 0,
            ))
        }) {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(error = %error, "failed to query todo due reminders");
                return;
            }
        };

        let mut pending: Vec<(i64, String, String, Option<i64>, &'static str)> = Vec::new();
        for row in rows {
            let row = match row {
                Ok(row) => row,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to read todo due reminder row");
                    continue;
                }
            };
            let (
                id,
                title,
                due_at,
                remind_1d,
                remind_1h,
                remind_custom_hours,
                reminded_1d,
                reminded_1h,
                reminded_custom,
                reminded_at,
            ) = row;
            let Ok(due) = DateTime::parse_from_rfc3339(&due_at) else {
                tracing::warn!(todo_id = id, "todo due_at has invalid rfc3339 value");
                continue;
            };
            let due = due.with_timezone(&Local);
            let seconds_until = (due - now).num_seconds();

            let mut matched = false;
            if !matched
                && remind_1d
                && !reminded_1d
                && seconds_until <= 86_400
                && seconds_until > 86_340
            {
                pending.push((id, title.clone(), "1d".into(), None, "due_reminded_1d"));
                matched = true;
            }
            if !matched {
                if let Some(hours) = remind_custom_hours {
                    let threshold = hours * 3_600;
                    if !reminded_custom
                        && seconds_until <= threshold
                        && seconds_until > threshold - 60
                    {
                        pending.push((
                            id,
                            title.clone(),
                            "custom".into(),
                            Some(hours),
                            "due_reminded_custom",
                        ));
                        matched = true;
                    }
                }
            }
            if !matched
                && remind_1h
                && !reminded_1h
                && seconds_until <= 3_600
                && seconds_until > 3_540
            {
                pending.push((id, title.clone(), "1h".into(), None, "due_reminded_1h"));
                matched = true;
            }
            if !matched && !reminded_at && seconds_until <= 0 {
                pending.push((id, title, "due".into(), None, "due_reminded_at"));
            }
        }

        for (id, title, lead, hours, flag) in &pending {
            if let Err(error) = mark_due_reminder_sent(&conn, *id, flag) {
                tracing::warn!(
                    todo_id = *id,
                    flag = %flag,
                    error = %error,
                    "failed to mark todo due reminder as sent"
                );
            }
            let _ = (title, lead, hours);
        }

        pending
    };

    for (id, title, lead, hours, _) in reminders {
        emit_on_main(
            app,
            "reminder",
            json!({
                "type": "todo_due",
                "todo_id": id,
                "title": title,
                "lead": lead,
                "hours": hours,
            }),
        );
    }
}

pub(crate) fn is_in_night_range(now: &str, start: &str, end: &str) -> bool {
    if start <= end {
        now >= start && now <= end
    } else {
        now >= start || now <= end
    }
}
