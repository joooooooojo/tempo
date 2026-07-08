use crate::db::{
    add_app_time, add_screen_time, cleanup_old_data, get_daily_total, is_blocked, load_settings,
    today_str, AppLimit, AppState, AppUsage, DailyReport, DashboardData, HourlyData, PomodoroState,
    Settings, TodoImage, TodoItem, TodoNote, TodoNoteImage, WeeklyDay, WeeklyReport,
};
use crate::platform::{get_foreground_app, should_count_screen_time, should_count_time};
use base64::Engine as _;
use chrono::{DateTime, Duration as ChronoDuration, Local, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::json;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

const DAILY_RECOMMENDED_LIMIT_SECONDS: i64 = 8 * 60 * 60;
const MAX_TODO_IMAGES: usize = 4;
const MAX_TODO_NOTE_IMAGES: usize = 4;
const MAX_TODO_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_TODO_NOTE_CHARS: usize = 1_000;

#[derive(Debug, Clone, Deserialize)]
pub struct TodoImageInput {
    data_url: String,
    mime_type: String,
}

pub fn start_tracker(app: AppHandle, state: AppState) {
    std::thread::spawn(move || {
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

            {
                let conn = state.db.lock();
                for (bucket_date, bucket_hour, seconds) in second_buckets(now, elapsed_seconds) {
                    add_screen_time(&conn, &bucket_date, bucket_hour, seconds);

                    if let Some(ref app_info) = foreground {
                        if !is_blocked(&conn, &app_info.name) {
                            add_app_time(
                                &conn,
                                &bucket_date,
                                &app_info.name,
                                &app_info.process_name,
                                seconds,
                                app_info.icon_data_url.as_deref(),
                            );
                        }
                    }
                }
            }

            if let Some(ref app_info) = foreground {
                check_app_limits(&app, &state, &date, &app_info.name);
            }

            {
                let mut tracker = state.tracker.lock();
                tracker.continuous_seconds += elapsed_seconds;
            }

            push_dashboard_event(&app, &state);

            if tick_count % 5 == 0 {
                update_tray_tooltip(&app, &state);
            }

            if tick_count % 60 == 0 {
                let conn = state.db.lock();
                cleanup_old_data(&conn);
            }

            check_reminders(&app, &state);
            crate::pomodoro::tick_pomodoro(&app, &state, elapsed_seconds);
        }
    });
}

fn second_buckets(now: DateTime<Local>, seconds: i64) -> Vec<(String, u32, i64)> {
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

fn push_dashboard_event(app: &AppHandle, state: &AppState) {
    let Some(dashboard) = build_dashboard(state) else {
        return;
    };
    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = app_handle.emit("dashboard-update", dashboard);
    });
}

fn build_dashboard(state: &AppState) -> Option<DashboardData> {
    let continuous = state.tracker.lock().continuous_seconds;
    let today = today_str();
    let (today_secs, week_secs, month_secs, mut top_apps) = {
        let conn = state.db.lock();
        (
            get_daily_total(&conn, &today),
            crate::db::sum_range(&conn, 7),
            crate::db::sum_range(&conn, 30),
            crate::db::top_apps(&conn, &today, 10),
        )
    };
    hydrate_app_icons(state, &today, &mut top_apps);

    let status = if today_secs == 0 {
        "今日尚未开始统计".into()
    } else if today_secs > 8 * 3600 {
        "今日使用时长较长，注意休息".into()
    } else if today_secs > 4 * 3600 {
        "使用时长适中".into()
    } else {
        "今日使用正常".into()
    };

    Some(DashboardData {
        today_screen_seconds: today_secs,
        week_screen_seconds: week_secs,
        month_screen_seconds: month_secs,
        top_apps,
        continuous_screen_seconds: continuous,
        status_message: status,
    })
}

fn hydrate_app_icons(state: &AppState, date: &str, apps: &mut [AppUsage]) {
    for app in apps.iter_mut() {
        if app
            .icon_data_url
            .as_deref()
            .is_some_and(|icon| !icon_needs_refresh(icon))
        {
            continue;
        }

        let Some(icon) =
            crate::platform::resolve_app_icon_data_url(&app.app_name, &app.process_name)
        else {
            continue;
        };

        {
            let conn = state.db.lock();
            conn.execute(
                "UPDATE app_usage SET icon_data_url = ?1 WHERE date = ?2 AND app_name = ?3",
                params![icon, date, app.app_name],
            )
            .ok();
        }

        app.icon_data_url = Some(icon);
    }
}

fn icon_needs_refresh(icon_data_url: &str) -> bool {
    if icon_data_url.trim().is_empty() {
        return true;
    }

    let Some(payload) = icon_data_url.strip_prefix("data:image/png;base64,") else {
        return false;
    };

    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(payload) else {
        return true;
    };

    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return false;
    }

    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    width < 64
}

fn update_tray_tooltip(app: &AppHandle, state: &AppState) {
    let today = {
        let conn = state.db.lock();
        get_daily_total(&conn, &today_str())
    };
    let tooltip = format!("今日屏幕时长: {}", format_duration(today));
    let app_handle = app.clone();

    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = app_handle.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(&tooltip));
        }
    });
}

fn emit_on_main(app: &AppHandle, event: &str, payload: serde_json::Value) {
    let app_handle = app.clone();
    let event = event.to_string();
    let _ = app.run_on_main_thread(move || {
        let _ = app_handle.emit(&event, payload);
    });
}

fn check_app_limits(app: &AppHandle, state: &AppState, date: &str, app_name: &str) {
    let row: Option<(i64, i64, i64, i64)> = {
        let conn = state.db.lock();
        conn.query_row(
            "SELECT l.limit_seconds, COALESCE(u.seconds, 0), l.warn_sent, l.limit_sent
             FROM app_limits l
             LEFT JOIN app_usage u ON u.app_name = l.app_name AND u.date = ?1
             WHERE l.app_name = ?2",
            params![date, app_name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok()
    };

    let Some((limit, used, warn_sent, limit_sent)) = row else {
        return;
    };

    if limit <= 0 {
        return;
    }

    let pct = (used * 100) / limit;

    if pct >= 80 && warn_sent == 0 {
        {
            let conn = state.db.lock();
            conn.execute(
                "UPDATE app_limits SET warn_sent = 1 WHERE app_name = ?1",
                [app_name],
            )
            .ok();
        }
        emit_on_main(
            app,
            "reminder",
            json!({ "type": "app_limit_warn", "app_name": app_name, "percent": pct }),
        );
        emit_on_main(
            app,
            "toast",
            json!({ "message": format!("{} 使用已达 {}%", app_name, pct) }),
        );
    }

    if pct >= 100 && limit_sent == 0 {
        {
            let conn = state.db.lock();
            conn.execute(
                "UPDATE app_limits SET limit_sent = 1 WHERE app_name = ?1",
                [app_name],
            )
            .ok();
        }
        emit_on_main(
            app,
            "reminder",
            json!({ "type": "app_limit_reached", "app_name": app_name }),
        );
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

fn is_in_night_range(now: &str, start: &str, end: &str) -> bool {
    if start <= end {
        now >= start && now <= end
    } else {
        now >= start || now <= end
    }
}

#[tauri::command]
pub fn get_dashboard(state: tauri::State<AppState>) -> DashboardData {
    build_dashboard(&state).unwrap_or(DashboardData {
        today_screen_seconds: 0,
        week_screen_seconds: 0,
        month_screen_seconds: 0,
        top_apps: vec![],
        continuous_screen_seconds: 0,
        status_message: "加载中".into(),
    })
}

#[tauri::command]
pub fn get_daily_report(state: tauri::State<AppState>, date: Option<String>) -> DailyReport {
    let date = date.unwrap_or_else(today_str);
    let (total, hourly, mut top_apps) = {
        let conn = state.db.lock();
        let total = get_daily_total(&conn, &date);

        let mut hourly = Vec::new();
        for h in 0..24 {
            let secs: i64 = conn
                .query_row(
                    "SELECT COALESCE(seconds, 0) FROM screen_time_hourly WHERE date = ?1 AND hour = ?2",
                    params![date, h],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            hourly.push(HourlyData {
                hour: h,
                seconds: secs,
            });
        }

        (total, hourly, crate::db::top_apps(&conn, &date, 10))
    };
    hydrate_app_icons(&state, &date, &mut top_apps);

    let peak = hourly
        .iter()
        .max_by_key(|h| h.seconds)
        .cloned()
        .unwrap_or(HourlyData {
            hour: 0,
            seconds: 0,
        });

    let active_hours = hourly.iter().filter(|h| h.seconds > 0).count().max(1) as i64;
    let average = total / active_hours;

    DailyReport {
        date: date.clone(),
        total_seconds: total,
        average_seconds: average,
        peak_hour: peak.hour,
        peak_seconds: peak.seconds,
        hourly,
        top_apps,
    }
}

#[tauri::command]
pub fn get_weekly_report(state: tauri::State<AppState>) -> WeeklyReport {
    let conn = state.db.lock();
    let today = Local::now().date_naive();
    let mut days = Vec::new();
    let mut total = 0i64;

    for i in (0..7).rev() {
        let d = today - chrono::Duration::days(i);
        let ds = d.format("%Y-%m-%d").to_string();
        let secs = get_daily_total(&conn, &ds);
        total += secs;
        days.push(WeeklyDay {
            date: ds,
            seconds: secs,
            is_over_limit: false,
        });
    }

    let average = total / 7;

    for day in &mut days {
        day.is_over_limit = day.seconds > DAILY_RECOMMENDED_LIMIT_SECONDS;
    }

    WeeklyReport {
        days,
        average_seconds: average,
        daily_limit_seconds: DAILY_RECOMMENDED_LIMIT_SECONDS,
    }
}

#[tauri::command]
pub fn get_settings(state: tauri::State<AppState>) -> Settings {
    let conn = state.db.lock();
    load_settings(&conn)
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: tauri::State<AppState>,
    settings: serde_json::Value,
) -> Result<(), String> {
    let mut current = {
        let conn = state.db.lock();
        load_settings(&conn)
    };

    if let Some(v) = settings.get("autostart").and_then(|v| v.as_bool()) {
        current.autostart = v;
        use tauri_plugin_autostart::ManagerExt;
        let autostart = app.autolaunch();
        if v {
            autostart.enable().map_err(|e| e.to_string())?;
        } else {
            autostart.disable().map_err(|e| e.to_string())?;
        }
    }
    if let Some(v) = settings.get("sound_enabled").and_then(|v| v.as_bool()) {
        current.sound_enabled = v;
    }
    if let Some(v) = settings.get("theme").and_then(|v| v.as_str()) {
        current.theme = v.into();
    }
    if let Some(v) = settings.get("eye_care_enabled").and_then(|v| v.as_bool()) {
        current.eye_care_enabled = v;
    }
    if let Some(v) = settings
        .get("eye_care_interval_minutes")
        .and_then(|v| v.as_u64())
    {
        current.eye_care_interval_minutes = v as u32;
    }
    if let Some(v) = settings
        .get("night_reminder_enabled")
        .and_then(|v| v.as_bool())
    {
        current.night_reminder_enabled = v;
    }
    if let Some(v) = settings
        .get("night_reminder_start")
        .and_then(|v| v.as_str())
    {
        current.night_reminder_start = v.into();
    }
    if let Some(v) = settings.get("night_reminder_end").and_then(|v| v.as_str()) {
        current.night_reminder_end = v.into();
    }
    if let Some(v) = settings
        .get("pomodoro_work_minutes")
        .and_then(|v| v.as_u64())
    {
        current.pomodoro_work_minutes = v as u32;
    }
    if let Some(v) = settings
        .get("pomodoro_short_break_minutes")
        .and_then(|v| v.as_u64())
    {
        current.pomodoro_short_break_minutes = v as u32;
    }
    if let Some(v) = settings
        .get("pomodoro_long_break_minutes")
        .and_then(|v| v.as_u64())
    {
        current.pomodoro_long_break_minutes = v as u32;
    }
    if let Some(v) = settings
        .get("pomodoro_sessions_per_cycle")
        .and_then(|v| v.as_u64())
    {
        current.pomodoro_sessions_per_cycle = v as u32;
    }

    let conn = state.db.lock();
    crate::db::save_settings(&conn, &current);
    Ok(())
}

#[tauri::command]
pub fn reset_today(state: tauri::State<AppState>) {
    do_reset_today(&state);
}

pub fn do_reset_today(state: &AppState) {
    let conn = state.db.lock();
    let today = today_str();
    conn.execute("DELETE FROM screen_time_daily WHERE date = ?1", [&today])
        .ok();
    conn.execute("DELETE FROM screen_time_hourly WHERE date = ?1", [&today])
        .ok();
    conn.execute("DELETE FROM app_usage WHERE date = ?1", [&today])
        .ok();
    conn.execute("UPDATE app_limits SET warn_sent = 0, limit_sent = 0", [])
        .ok();
}

#[tauri::command]
pub fn reset_all(state: tauri::State<AppState>) {
    let conn = state.db.lock();
    conn.execute("DELETE FROM screen_time_daily", []).ok();
    conn.execute("DELETE FROM screen_time_hourly", []).ok();
    conn.execute("DELETE FROM app_usage", []).ok();
    conn.execute("UPDATE app_limits SET warn_sent = 0, limit_sent = 0", [])
        .ok();
}

#[tauri::command]
pub fn get_blocked_apps(state: tauri::State<AppState>) -> Vec<String> {
    let conn = state.db.lock();
    let mut stmt = conn
        .prepare("SELECT app_name FROM blocked_apps ORDER BY app_name")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .filter_map(|x| x.ok())
        .collect()
}

#[tauri::command]
pub fn block_app(state: tauri::State<AppState>, app_name: String) {
    let conn = state.db.lock();
    conn.execute(
        "INSERT OR IGNORE INTO blocked_apps (app_name) VALUES (?1)",
        [&app_name],
    )
    .ok();
}

#[tauri::command]
pub fn unblock_app(state: tauri::State<AppState>, app_name: String) {
    let conn = state.db.lock();
    conn.execute("DELETE FROM blocked_apps WHERE app_name = ?1", [&app_name])
        .ok();
}

#[tauri::command]
pub fn get_app_limits(state: tauri::State<AppState>) -> Vec<AppLimit> {
    let conn = state.db.lock();
    let today = today_str();
    let mut stmt = conn
        .prepare(
            "SELECT l.app_name, l.limit_seconds, COALESCE(u.seconds, 0), l.warn_sent, l.limit_sent
             FROM app_limits l
             LEFT JOIN app_usage u ON u.app_name = l.app_name AND u.date = ?1",
        )
        .unwrap();
    stmt.query_map([&today], |r| {
        Ok(AppLimit {
            app_name: r.get(0)?,
            limit_seconds: r.get(1)?,
            used_seconds: r.get(2)?,
            warn_sent: r.get::<_, i64>(3)? != 0,
            limit_sent: r.get::<_, i64>(4)? != 0,
        })
    })
    .unwrap()
    .filter_map(|x| x.ok())
    .collect()
}

#[tauri::command]
pub fn set_app_limit(state: tauri::State<AppState>, app_name: String, limit_seconds: i64) {
    let conn = state.db.lock();
    conn.execute(
        "INSERT INTO app_limits (app_name, limit_seconds, warn_sent, limit_sent)
         VALUES (?1, ?2, 0, 0)
         ON CONFLICT(app_name) DO UPDATE SET limit_seconds = excluded.limit_seconds,
           warn_sent = 0, limit_sent = 0",
        params![app_name, limit_seconds],
    )
    .ok();
}

#[tauri::command]
pub fn remove_app_limit(state: tauri::State<AppState>, app_name: String) {
    let conn = state.db.lock();
    conn.execute("DELETE FROM app_limits WHERE app_name = ?1", [&app_name])
        .ok();
}

#[tauri::command]
pub fn get_todos(state: tauri::State<AppState>) -> Result<Vec<TodoItem>, String> {
    let conn = state.db.lock();
    let mut todos = {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, completed, due_at, created_at, completed_at
                 FROM todos
                 ORDER BY completed ASC,
                   CASE WHEN completed = 0 AND due_at IS NOT NULL THEN 0 ELSE 1 END ASC,
                   CASE WHEN completed = 0 THEN datetime(due_at) END ASC,
                   datetime(COALESCE(completed_at, created_at)) DESC,
                   id DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], todo_from_row)
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    hydrate_todo_images(&conn, &mut todos)?;
    hydrate_todo_notes(&conn, &mut todos)?;
    Ok(todos)
}

#[tauri::command]
pub fn add_todo(
    state: tauri::State<AppState>,
    title: String,
    due_at: Option<String>,
    images: Option<Vec<TodoImageInput>>,
) -> Result<TodoItem, String> {
    let images = normalize_todo_images(images)?;
    let title = normalize_todo_title(title, !images.is_empty())?;
    let due_at = normalize_due_at(due_at)?;
    let created_at = Local::now().to_rfc3339();
    let conn = state.db.lock();
    conn.execute(
        "INSERT INTO todos (title, completed, due_at, created_at) VALUES (?1, 0, ?2, ?3)",
        params![title, due_at, created_at],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    insert_todo_images(&conn, id, &images)?;
    fetch_todo(&conn, id)
}

#[tauri::command]
pub fn update_todo_title(
    state: tauri::State<AppState>,
    id: i64,
    title: String,
) -> Result<TodoItem, String> {
    let title = normalize_todo_title(title, false)?;
    let conn = state.db.lock();
    let _existing = fetch_todo(&conn, id)?;

    conn.execute(
        "UPDATE todos SET title = ?1 WHERE id = ?2",
        params![title, id],
    )
    .map_err(|e| e.to_string())?;

    fetch_todo(&conn, id)
}

#[tauri::command]
pub fn update_todo_details(
    state: tauri::State<AppState>,
    id: i64,
    title: String,
    due_at: Option<String>,
) -> Result<TodoItem, String> {
    let title = normalize_todo_title(title, false)?;
    let due_at = normalize_due_at(due_at)?;
    let conn = state.db.lock();
    let _existing = fetch_todo(&conn, id)?;

    conn.execute(
        "UPDATE todos SET title = ?1, due_at = ?2 WHERE id = ?3",
        params![title, due_at, id],
    )
    .map_err(|e| e.to_string())?;

    fetch_todo(&conn, id)
}

#[tauri::command]
pub fn set_todo_completed(
    state: tauri::State<AppState>,
    id: i64,
    completed: bool,
) -> Result<TodoItem, String> {
    let completed_at = completed.then(|| Local::now().to_rfc3339());
    let conn = state.db.lock();
    conn.execute(
        "UPDATE todos SET completed = ?1, completed_at = ?2 WHERE id = ?3",
        params![if completed { 1 } else { 0 }, completed_at, id],
    )
    .map_err(|e| e.to_string())?;

    if conn.changes() == 0 {
        return Err("待办不存在".into());
    }

    fetch_todo(&conn, id)
}

#[tauri::command]
pub fn add_todo_image(
    state: tauri::State<AppState>,
    todo_id: i64,
    image: TodoImageInput,
) -> Result<TodoItem, String> {
    let images = normalize_todo_images(Some(vec![image]))?;
    let conn = state.db.lock();
    let _existing = fetch_todo(&conn, todo_id)?;
    let image_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM todo_images WHERE todo_id = ?1",
            [todo_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    if image_count as usize + images.len() > MAX_TODO_IMAGES {
        return Err(format!("每个待办最多添加 {} 张图片", MAX_TODO_IMAGES));
    }

    insert_todo_images(&conn, todo_id, &images)?;
    fetch_todo(&conn, todo_id)
}

#[tauri::command]
pub fn delete_todo_image(state: tauri::State<AppState>, image_id: i64) -> Result<TodoItem, String> {
    let conn = state.db.lock();
    let todo_id: i64 = conn
        .query_row(
            "SELECT todo_id FROM todo_images WHERE id = ?1",
            [image_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "图片不存在".to_string())?;

    conn.execute("DELETE FROM todo_images WHERE id = ?1", [image_id])
        .map_err(|e| e.to_string())?;

    fetch_todo(&conn, todo_id)
}

#[tauri::command]
pub fn add_todo_note(
    state: tauri::State<AppState>,
    todo_id: i64,
    body: String,
    images: Option<Vec<TodoImageInput>>,
) -> Result<TodoItem, String> {
    let images = normalize_todo_note_images(images)?;
    let body = normalize_todo_note_body(body, !images.is_empty())?;
    let created_at = Local::now().to_rfc3339();
    let conn = state.db.lock();
    let _existing = fetch_todo(&conn, todo_id)?;

    conn.execute(
        "INSERT INTO todo_notes (todo_id, body, created_at) VALUES (?1, ?2, ?3)",
        params![todo_id, body, created_at],
    )
    .map_err(|e| e.to_string())?;

    let note_id = conn.last_insert_rowid();
    insert_todo_note_images(&conn, note_id, &images)?;
    fetch_todo(&conn, todo_id)
}

#[tauri::command]
pub fn delete_todo_note(state: tauri::State<AppState>, note_id: i64) -> Result<TodoItem, String> {
    let conn = state.db.lock();
    let todo_id: i64 = conn
        .query_row(
            "SELECT todo_id FROM todo_notes WHERE id = ?1",
            [note_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "备注不存在".to_string())?;

    conn.execute(
        "DELETE FROM todo_note_images WHERE note_id = ?1",
        [note_id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM todo_notes WHERE id = ?1", [note_id])
        .map_err(|e| e.to_string())?;

    fetch_todo(&conn, todo_id)
}

#[tauri::command]
pub fn delete_todo(state: tauri::State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock();
    conn.execute(
        "DELETE FROM todo_note_images WHERE note_id IN (SELECT id FROM todo_notes WHERE todo_id = ?1)",
        [id],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM todo_notes WHERE todo_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM todo_images WHERE todo_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM todos WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;

    if conn.changes() == 0 {
        return Err("待办不存在".into());
    }

    Ok(())
}

#[tauri::command]
pub fn clear_completed_todos(state: tauri::State<AppState>) -> Result<u64, String> {
    let conn = state.db.lock();
    conn.execute(
        "DELETE FROM todo_note_images WHERE note_id IN (
            SELECT todo_notes.id
            FROM todo_notes
            INNER JOIN todos ON todos.id = todo_notes.todo_id
            WHERE todos.completed = 1
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM todo_notes WHERE todo_id IN (SELECT id FROM todos WHERE completed = 1)",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "DELETE FROM todo_images WHERE todo_id IN (SELECT id FROM todos WHERE completed = 1)",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM todos WHERE completed = 1", [])
        .map_err(|e| e.to_string())?;
    Ok(conn.changes())
}

#[tauri::command]
pub fn get_known_apps(state: tauri::State<AppState>) -> Vec<AppUsage> {
    let today = today_str();
    let mut apps = {
        let conn = state.db.lock();
        crate::db::top_apps(&conn, &today, 50)
    };
    hydrate_app_icons(&state, &today, &mut apps);
    apps
}

#[tauri::command]
pub fn export_report(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    let conn = state.db.lock();
    let today = today_str();

    let mut lines = vec!["日期,应用名称,耗时(秒),耗时(格式化)".to_string()];

    let mut stmt = conn
        .prepare(
            "SELECT date, app_name, process_name, seconds FROM app_usage
             ORDER BY date DESC, seconds DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    for row in rows.filter_map(|x| x.ok()) {
        let (date, name, process, secs) = row;
        if crate::db::is_system_host_usage(&name, &process) {
            continue;
        }
        let formatted = format_duration(secs);
        lines.push(format!("{},{},{},{}", date, name, secs, formatted));
    }

    let screen_total = get_daily_total(&conn, &today);
    lines.push(String::new());
    lines.push(format!("今日屏幕总时长(秒),{}", screen_total));

    let content = "\u{FEFF}".to_string() + &lines.join("\n");
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

fn format_duration(seconds: i64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{}小时{}分钟", h, m)
    } else {
        format!("{}分钟{}秒", m, s)
    }
}

#[tauri::command]
pub fn complete_onboarding(state: tauri::State<AppState>) {
    let conn = state.db.lock();
    crate::db::set_setting(&conn, "onboarding_completed", "true");
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

pub fn hide_to_tray(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        crate::macos_dock::hide_presence(app);
    }

    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn hide_to_tray_command(app: AppHandle) {
    hide_to_tray(&app);
}

#[tauri::command]
pub fn show_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            crate::macos_dock::show_presence(&app)?;
        }

        window.show().map_err(|e| e.to_string())?;
        window.unminimize().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;

        #[cfg(target_os = "macos")]
        {
            crate::macos_dock::apply_branding(&app);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_pomodoro_state(state: tauri::State<AppState>) -> PomodoroState {
    crate::pomodoro::pomodoro_state_snapshot(&state)
}

#[tauri::command]
pub fn start_pomodoro(
    app: AppHandle,
    state: tauri::State<AppState>,
) -> Result<PomodoroState, String> {
    let snapshot = crate::pomodoro::start_pomodoro(&state)?;
    crate::pomodoro::push_pomodoro_update(&app, &state);
    Ok(snapshot)
}

#[tauri::command]
pub fn pause_pomodoro(app: AppHandle, state: tauri::State<AppState>) -> PomodoroState {
    let snapshot = crate::pomodoro::pause_pomodoro(&state);
    crate::pomodoro::push_pomodoro_update(&app, &state);
    snapshot
}

#[tauri::command]
pub fn stop_pomodoro(app: AppHandle, state: tauri::State<AppState>) -> PomodoroState {
    let snapshot = crate::pomodoro::stop_pomodoro(&state);
    crate::pomodoro::push_pomodoro_update(&app, &state);
    snapshot
}

#[tauri::command]
pub fn skip_pomodoro_phase(app: AppHandle, state: tauri::State<AppState>) -> PomodoroState {
    crate::pomodoro::skip_pomodoro_phase(&app, &state)
}

fn normalize_todo_title(title: String, allow_image_only: bool) -> Result<String, String> {
    let normalized = title
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if normalized.is_empty() {
        if allow_image_only {
            return Ok("图片待办".into());
        }
        return Err("请输入待办内容".into());
    }

    if normalized.chars().count() > 120 {
        return Err("待办内容不能超过 120 个字".into());
    }

    Ok(normalized)
}

fn normalize_due_at(due_at: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = due_at
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let parsed =
        DateTime::parse_from_rfc3339(&value).map_err(|_| "截止时间格式无效".to_string())?;
    Ok(Some(parsed.to_rfc3339()))
}

fn normalize_todo_images(
    images: Option<Vec<TodoImageInput>>,
) -> Result<Vec<TodoImageInput>, String> {
    let images = images.unwrap_or_default();

    if images.len() > MAX_TODO_IMAGES {
        return Err(format!("每个待办最多添加 {} 张图片", MAX_TODO_IMAGES));
    }

    validate_todo_image_inputs(&images)?;
    Ok(images)
}

fn normalize_todo_note_body(body: String, allow_image_only: bool) -> Result<String, String> {
    let normalized = body
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if normalized.is_empty() {
        if allow_image_only {
            return Ok(String::new());
        }
        return Err("请输入备注内容".into());
    }

    if normalized.chars().count() > MAX_TODO_NOTE_CHARS {
        return Err(format!("备注不能超过 {} 个字", MAX_TODO_NOTE_CHARS));
    }

    Ok(normalized)
}

fn normalize_todo_note_images(
    images: Option<Vec<TodoImageInput>>,
) -> Result<Vec<TodoImageInput>, String> {
    let images = images.unwrap_or_default();

    if images.len() > MAX_TODO_NOTE_IMAGES {
        return Err(format!(
            "每条备注最多添加 {} 张图片",
            MAX_TODO_NOTE_IMAGES
        ));
    }

    validate_todo_image_inputs(&images)?;
    Ok(images)
}

fn validate_todo_image_inputs(images: &[TodoImageInput]) -> Result<(), String> {
    for image in images {
        let mime = image.mime_type.trim().to_ascii_lowercase();
        if !matches!(
            mime.as_str(),
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        ) {
            return Err("仅支持 PNG、JPEG、WebP 或 GIF 图片".into());
        }

        if !image.data_url.starts_with("data:image/") {
            return Err("图片数据格式无效".into());
        }

        let Some((_, payload)) = image.data_url.split_once(',') else {
            return Err("图片数据格式无效".into());
        };

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|_| "图片数据格式无效".to_string())?;

        if bytes.len() > MAX_TODO_IMAGE_BYTES {
            return Err("单张图片不能超过 5MB".into());
        }
    }

    Ok(())
}

fn fetch_todo(conn: &Connection, id: i64) -> Result<TodoItem, String> {
    let mut todo = conn
        .query_row(
            "SELECT id, title, completed, due_at, created_at, completed_at
         FROM todos
         WHERE id = ?1",
            [id],
            todo_from_row,
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "待办不存在".to_string())?;

    hydrate_todo_images(conn, std::slice::from_mut(&mut todo))?;
    hydrate_todo_notes(conn, std::slice::from_mut(&mut todo))?;
    Ok(todo)
}

fn todo_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodoItem> {
    Ok(TodoItem {
        id: row.get(0)?,
        title: row.get(1)?,
        completed: row.get::<_, i64>(2)? != 0,
        due_at: row.get(3)?,
        created_at: row.get(4)?,
        completed_at: row.get(5)?,
        images: Vec::new(),
        notes: Vec::new(),
    })
}

fn insert_todo_images(
    conn: &Connection,
    todo_id: i64,
    images: &[TodoImageInput],
) -> Result<(), String> {
    for image in images {
        conn.execute(
            "INSERT INTO todo_images (todo_id, data_url, mime_type, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                todo_id,
                image.data_url,
                image.mime_type.trim().to_ascii_lowercase(),
                Local::now().to_rfc3339()
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn hydrate_todo_images(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
    for todo in todos {
        let mut stmt = conn
            .prepare(
                "SELECT id, todo_id, data_url, mime_type, created_at
                 FROM todo_images
                 WHERE todo_id = ?1
                 ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([todo.id], |row| {
                Ok(TodoImage {
                    id: row.get(0)?,
                    todo_id: row.get(1)?,
                    data_url: row.get(2)?,
                    mime_type: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        todo.images = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn insert_todo_note_images(
    conn: &Connection,
    note_id: i64,
    images: &[TodoImageInput],
) -> Result<(), String> {
    for image in images {
        conn.execute(
            "INSERT INTO todo_note_images (note_id, data_url, mime_type, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                note_id,
                image.data_url,
                image.mime_type.trim().to_ascii_lowercase(),
                Local::now().to_rfc3339()
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn hydrate_todo_notes(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
    for todo in todos {
        let mut stmt = conn
            .prepare(
                "SELECT id, todo_id, body, created_at
                 FROM todo_notes
                 WHERE todo_id = ?1
                 ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([todo.id], |row| {
                Ok(TodoNote {
                    id: row.get(0)?,
                    todo_id: row.get(1)?,
                    body: row.get(2)?,
                    created_at: row.get(3)?,
                    images: Vec::new(),
                })
            })
            .map_err(|e| e.to_string())?;

        let mut notes = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        hydrate_todo_note_images(conn, &mut notes)?;
        todo.notes = notes;
    }

    Ok(())
}

fn hydrate_todo_note_images(conn: &Connection, notes: &mut [TodoNote]) -> Result<(), String> {
    for note in notes {
        let mut stmt = conn
            .prepare(
                "SELECT id, note_id, data_url, mime_type, created_at
                 FROM todo_note_images
                 WHERE note_id = ?1
                 ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([note.id], |row| {
                Ok(TodoNoteImage {
                    id: row.get(0)?,
                    note_id: row.get(1)?,
                    data_url: row.get(2)?,
                    mime_type: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;

        note.images = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
