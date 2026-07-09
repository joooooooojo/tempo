use crate::db::{
    add_app_time, add_screen_time, cleanup_old_data, current_storage_dir, default_storage_dir,
    get_daily_total, is_blocked, load_settings, save_storage_dir, today_str, AppLimit, AppState,
    AppUsage, DailyReport, HourlyData, PomodoroState, Settings, TodoImage, TodoItem, TodoNote,
    TodoNoteImage, TodoSubtask, WeeklyDay, WeeklyReport, MAX_DAILY_SECONDS, MAX_HOURLY_SECONDS,
};
use crate::platform::{get_foreground_app, should_count_screen_time, should_count_time};
use base64::Engine as _;
use chrono::{DateTime, Duration as ChronoDuration, Local, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Instant,
};
use tauri::{
    http::{
        header::{CONTENT_LENGTH, CONTENT_TYPE},
        Method, Request, Response, StatusCode,
    },
    AppHandle, Emitter, Manager,
};

const DAILY_RECOMMENDED_LIMIT_SECONDS: i64 = 8 * 60 * 60;
const MAX_TODO_IMAGES: usize = 4;
const MAX_TODO_NOTE_IMAGES: usize = 4;
const MAX_TODO_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_TODO_NOTE_CHARS: usize = 1_000;
pub const MARKDOWN_IMAGE_PROTOCOL: &str = "tempo-image";

#[derive(Debug, Clone, Deserialize)]
pub struct TodoImageInput {
    data_url: String,
    mime_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TodoBackupFile {
    format: String,
    exported_at: String,
    todos: Vec<TodoItem>,
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
                    let tracked_seconds =
                        add_screen_time(&conn, &bucket_date, bucket_hour, seconds);
                    if tracked_seconds <= 0 {
                        continue;
                    }

                    if let Some(ref app_info) = foreground {
                        if !is_blocked(&conn, &app_info.name) {
                            add_app_time(
                                &conn,
                                &bucket_date,
                                &app_info.name,
                                &app_info.process_name,
                                tracked_seconds,
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
            Err(_) => return,
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
            Err(_) => return,
        };

        let mut pending: Vec<(i64, String, String, Option<i64>, &'static str)> = Vec::new();
        for row in rows.filter_map(|row| row.ok()) {
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
            mark_due_reminder_sent(&conn, *id, flag).ok();
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

fn is_in_night_range(now: &str, start: &str, end: &str) -> bool {
    if start <= end {
        now >= start && now <= end
    } else {
        now >= start || now <= end
    }
}

#[tauri::command]
pub fn get_daily_report(state: tauri::State<AppState>, date: Option<String>) -> DailyReport {
    let date = date.unwrap_or_else(today_str);
    let (total, hourly, mut top_apps) = {
        let conn = state.db.lock();
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
                seconds: secs.clamp(0, MAX_HOURLY_SECONDS),
            });
        }
        let total = hourly
            .iter()
            .map(|item| item.seconds)
            .sum::<i64>()
            .min(MAX_DAILY_SECONDS);

        (total, hourly, crate::db::top_apps(&conn, &date, i64::MAX))
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
    let today = Local::now().date_naive();
    let today_text = today.format("%Y-%m-%d").to_string();
    let start_text = (today - chrono::Duration::days(6))
        .format("%Y-%m-%d")
        .to_string();
    let (mut days, average, mut top_apps) = {
        let conn = state.db.lock();
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

        (
            days,
            total / 7,
            weekly_top_apps(&conn, &start_text, &today_text),
        )
    };

    hydrate_app_icons(&state, &today_text, &mut top_apps);
    for day in &mut days {
        day.is_over_limit = day.seconds > DAILY_RECOMMENDED_LIMIT_SECONDS;
    }
    WeeklyReport {
        days,
        average_seconds: average,
        daily_limit_seconds: DAILY_RECOMMENDED_LIMIT_SECONDS,
        top_apps,
    }
}

fn weekly_top_apps(conn: &Connection, start_date: &str, end_date: &str) -> Vec<AppUsage> {
    let mut stmt = match conn.prepare(
        "SELECT app_name,
                COALESCE(MAX(process_name), ''),
                COALESCE(MAX(category), ''),
                COALESCE(SUM(seconds), 0),
                MAX(icon_data_url)
         FROM app_usage
         WHERE date >= ?1 AND date <= ?2
         GROUP BY app_name
         ORDER BY SUM(seconds) DESC, app_name ASC",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map(params![start_date, end_date], |r| {
        Ok(AppUsage {
            app_name: r.get(0)?,
            process_name: r.get(1)?,
            category: r.get(2)?,
            seconds: r.get(3)?,
            icon_data_url: r.get(4)?,
        })
    }) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    rows.filter_map(|row| row.ok())
        .filter(|app| !crate::db::is_system_host_usage(&app.app_name, &app.process_name))
        .collect()
}

#[tauri::command]
pub fn get_settings(app: AppHandle, state: tauri::State<AppState>) -> Settings {
    let conn = state.db.lock();
    let mut settings = load_settings(&conn);
    settings.storage_dir = current_storage_dir(&app)
        .or_else(|_| default_storage_dir(&app))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    settings
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
pub fn set_storage_dir(
    app: AppHandle,
    state: tauri::State<AppState>,
    storage_dir: String,
) -> Result<Settings, String> {
    let target_dir = normalize_storage_dir(&storage_dir)?;
    let current_dir = current_storage_dir(&app).or_else(|_| default_storage_dir(&app))?;
    let current_dir = ensure_storage_dir(&current_dir)?;
    let target_dir = ensure_storage_dir(&target_dir)?;

    if same_path(&current_dir, &target_dir) {
        return Ok(settings_with_storage_dir(&app, &state));
    }

    let current_markdown_dir = current_dir.join("markdown-images");
    let canonical_current_markdown_dir = current_markdown_dir
        .canonicalize()
        .unwrap_or_else(|_| current_markdown_dir.clone());
    if path_is_within_or_same(&target_dir, &canonical_current_markdown_dir) {
        return Err("请选择 markdown-images 之外的位置".into());
    }

    let current_db = current_dir.join("screen_time.db");
    let target_db = target_dir.join("screen_time.db");
    if target_db.exists() {
        return Err("目标位置已存在 Tempo 数据，请选择一个空目录".into());
    }

    let target_markdown_dir = target_dir.join("markdown-images");

    let mut conn_guard = state.db.lock();
    conn_guard
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .ok();
    vacuum_database_into(&conn_guard, &target_db)?;
    copy_dir_contents(&current_markdown_dir, &target_markdown_dir)?;

    let next_conn = crate::db::init_db(&target_db);
    rewrite_markdown_storage_urls(&next_conn, &current_markdown_dir, &target_markdown_dir)?;
    save_storage_dir(&app, &target_dir)?;
    *conn_guard = next_conn;
    drop(conn_guard);

    cleanup_old_storage_files(
        &current_dir,
        &target_dir,
        &current_db,
        &current_markdown_dir,
    );

    Ok(settings_with_storage_dir(&app, &state))
}

fn settings_with_storage_dir(app: &AppHandle, state: &AppState) -> Settings {
    let conn = state.db.lock();
    let mut settings = load_settings(&conn);
    settings.storage_dir = current_storage_dir(app)
        .or_else(|_| default_storage_dir(app))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    settings
}

fn normalize_storage_dir(value: &str) -> Result<PathBuf, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("请选择文件存储位置".into());
    }

    let path = PathBuf::from(trimmed);
    if path
        .file_name()
        .is_some_and(|name| name == "markdown-images")
        || path
            .file_name()
            .is_some_and(|name| name == "screen_time.db")
    {
        return Err("请选择一个文件夹作为存储位置".into());
    }

    Ok(path)
}

fn ensure_storage_dir(path: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    assert_storage_dir_writable(path)?;
    path.canonicalize().map_err(|e| e.to_string())
}

fn assert_storage_dir_writable(path: &Path) -> Result<(), String> {
    let probe = path.join(format!(".tempo-write-test-{}", std::process::id()));
    std::fs::write(&probe, b"tempo").map_err(|e| format!("目标位置不可写: {e}"))?;
    std::fs::remove_file(&probe).map_err(|e| format!("目标位置不可写: {e}"))
}

fn same_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

fn path_is_within_or_same(path: &Path, parent: &Path) -> bool {
    path.ancestors().any(|ancestor| same_path(ancestor, parent))
}

fn vacuum_database_into(conn: &Connection, target_db: &Path) -> Result<(), String> {
    if let Some(parent) = target_db.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let sql_path = target_db.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{sql_path}';"))
        .map_err(|e| e.to_string())
}

fn copy_dir_contents(source: &Path, target: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_contents(&source_path, &target_path)?;
        } else if source_path.is_file() {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(&source_path, &target_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn rewrite_markdown_storage_urls(
    conn: &Connection,
    old_markdown_dir: &Path,
    new_markdown_dir: &Path,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, content FROM todos
             WHERE content LIKE '%asset.localhost/%' OR content LIKE '%asset://localhost/%'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    for (todo_id, content) in rows {
        let mut next = content.clone();
        for src in markdown_image_sources(&content) {
            let Some((file_name, _)) = markdown_image_reference(&src, old_markdown_dir) else {
                continue;
            };
            let new_path = new_markdown_dir.join(file_name);
            let Some(new_url) = markdown_image_url_for_path(&new_path) else {
                continue;
            };
            next = next.replace(&src, &new_url);
        }

        if next != content {
            conn.execute(
                "UPDATE todos SET content = ?1 WHERE id = ?2",
                params![next, todo_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn cleanup_old_storage_files(
    old_dir: &Path,
    new_dir: &Path,
    old_db: &Path,
    old_markdown_dir: &Path,
) {
    if same_path(old_dir, new_dir) {
        return;
    }

    let _ = std::fs::remove_file(old_db);
    let _ = std::fs::remove_file(old_dir.join("screen_time.db-wal"));
    let _ = std::fs::remove_file(old_dir.join("screen_time.db-shm"));
    let _ = std::fs::remove_dir_all(old_markdown_dir);
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
pub fn get_todos(app: AppHandle, state: tauri::State<AppState>) -> Result<Vec<TodoItem>, String> {
    let spawned = {
        let conn = state.db.lock();
        process_pending_recurrences(&conn)?
    };
    for todo in spawned {
        emit_on_main(
            &app,
            "todo-created",
            serde_json::to_value(todo).unwrap_or_else(|_| json!({})),
        );
    }

    let conn = state.db.lock();
    list_todos(&conn)
}

pub fn check_pending_recurrences(app: &AppHandle, state: &AppState) {
    let spawned = match {
        let conn = state.db.lock();
        process_pending_recurrences(&conn)
    } {
        Ok(items) => items,
        Err(_) => return,
    };

    for todo in spawned {
        emit_on_main(
            app,
            "todo-created",
            serde_json::to_value(todo).unwrap_or_else(|_| json!({})),
        );
    }
}

#[tauri::command]
pub fn add_todo(
    state: tauri::State<AppState>,
    title: String,
    content: Option<String>,
    due_at: Option<String>,
    images: Option<Vec<TodoImageInput>>,
    recurrence: Option<String>,
    remind_1d: Option<bool>,
    remind_1h: Option<bool>,
    remind_custom_hours: Option<i64>,
    subtasks: Option<Vec<String>>,
) -> Result<TodoItem, String> {
    let images = normalize_todo_images(images)?;
    let content = normalize_todo_content(content.unwrap_or_default());
    let title = normalize_todo_title(title, !images.is_empty())?;
    let due_at = normalize_due_at(due_at)?;
    let (recurrence, due_at, remind_1d, remind_1h, remind_custom_hours) =
        apply_recurrence_constraints(
            recurrence.unwrap_or_else(|| "none".into()),
            due_at,
            remind_1d.unwrap_or(false),
            remind_1h.unwrap_or(false),
            remind_custom_hours,
        )?;
    let subtask_titles = normalize_subtask_titles(subtasks)?;
    let created_at = Local::now().to_rfc3339();
    let conn = state.db.lock();
    conn.execute(
        "INSERT INTO todos (title, content, completed, due_at, recurrence, remind_1d, remind_1h, remind_custom_hours, created_at)
         VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            title,
            content,
            due_at,
            recurrence,
            if remind_1d { 1 } else { 0 },
            if remind_1h { 1 } else { 0 },
            remind_custom_hours,
            created_at
        ],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    if recurrence != "none" {
        conn.execute(
            "UPDATE todos SET recurrence_root_id = ?1 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
    }
    insert_todo_images(&conn, id, &images)?;
    insert_subtasks(&conn, id, &subtask_titles)?;
    fetch_todo(&conn, id)
}

#[tauri::command]
pub fn update_todo_details(
    app: AppHandle,
    state: tauri::State<AppState>,
    id: i64,
    title: String,
    content: String,
    due_at: Option<String>,
    recurrence: Option<String>,
    remind_1d: Option<bool>,
    remind_1h: Option<bool>,
    remind_custom_hours: Option<i64>,
) -> Result<TodoItem, String> {
    let content = normalize_todo_content(content);
    let title = normalize_todo_title(title, false)?;
    let due_at = normalize_due_at(due_at)?;
    let (recurrence, due_at, remind_1d, remind_1h, remind_custom_hours) =
        apply_recurrence_constraints(
            recurrence.unwrap_or_else(|| "none".into()),
            due_at,
            remind_1d.unwrap_or(false),
            remind_1h.unwrap_or(false),
            remind_custom_hours,
        )?;
    let conn = state.db.lock();
    let existing = fetch_todo(&conn, id)?;
    let due_changed = existing.due_at != due_at
        || existing.remind_1d != remind_1d
        || existing.remind_1h != remind_1h
        || existing.remind_custom_hours != remind_custom_hours;

    conn.execute(
        "UPDATE todos
         SET title = ?1,
             content = ?2,
             due_at = ?3,
             recurrence = ?4,
             remind_1d = ?5,
             remind_1h = ?6,
             remind_custom_hours = ?7,
             due_reminded_1d = CASE WHEN ?8 THEN 0 ELSE due_reminded_1d END,
             due_reminded_1h = CASE WHEN ?8 THEN 0 ELSE due_reminded_1h END,
             due_reminded_custom = CASE WHEN ?8 THEN 0 ELSE due_reminded_custom END,
             due_reminded_at = CASE WHEN ?8 THEN 0 ELSE due_reminded_at END
         WHERE id = ?9",
        params![
            title,
            content,
            due_at,
            recurrence,
            if remind_1d { 1 } else { 0 },
            if remind_1h { 1 } else { 0 },
            remind_custom_hours,
            if due_changed { 1 } else { 0 },
            id
        ],
    )
    .map_err(|e| e.to_string())?;

    if recurrence != "none" && existing.recurrence_root_id.is_none() {
        conn.execute(
            "UPDATE todos SET recurrence_root_id = ?1 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
    }

    let todo = fetch_todo(&conn, id)?;
    cleanup_unreferenced_markdown_images(&app, &conn);
    Ok(todo)
}

#[tauri::command]
pub fn set_todo_completed(
    state: tauri::State<AppState>,
    id: i64,
    completed: bool,
) -> Result<TodoItem, String> {
    let conn = state.db.lock();
    let existing = fetch_todo(&conn, id)?;

    if completed {
        let completed_at = Local::now().to_rfc3339();
        let next_recurrence_at = if existing.recurrence != "none" {
            if existing.recurrence_root_id.is_none() {
                conn.execute(
                    "UPDATE todos SET recurrence_root_id = ?1 WHERE id = ?1",
                    params![id],
                )
                .map_err(|e| e.to_string())?;
            }
            next_recurrence_midnight(Local::now(), &existing.recurrence)
        } else {
            None
        };
        let subtasks_snapshot = if existing.subtasks.is_empty() {
            None
        } else {
            Some(encode_subtask_completion_snapshot(&existing.subtasks))
        };

        conn.execute(
            "UPDATE todos
             SET completed = 1,
                 completed_at = ?1,
                 next_recurrence_at = ?2,
                 subtasks_completion_snapshot = ?3
             WHERE id = ?4",
            params![completed_at, next_recurrence_at, subtasks_snapshot, id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE todo_subtasks SET completed = 1 WHERE todo_id = ?1",
            [id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        let snapshot: Option<String> = conn
            .query_row(
                "SELECT subtasks_completion_snapshot FROM todos WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        conn.execute(
            "UPDATE todos
             SET completed = 0,
                 completed_at = NULL,
                 next_recurrence_at = NULL,
                 subtasks_completion_snapshot = NULL
             WHERE id = ?1",
            [id],
        )
        .map_err(|e| e.to_string())?;

        if let Some(snapshot) = snapshot {
            restore_subtask_completion_snapshot(&conn, id, &snapshot)?;
        }
    }

    if conn.changes() == 0 {
        return Err("待办不存在".into());
    }

    fetch_todo(&conn, id)
}

#[tauri::command]
pub fn set_todo_pinned(
    state: tauri::State<AppState>,
    id: i64,
    pinned: bool,
) -> Result<TodoItem, String> {
    let pinned_at = pinned.then(|| Local::now().to_rfc3339());
    let conn = state.db.lock();
    conn.execute(
        "UPDATE todos SET pinned_at = ?1 WHERE id = ?2",
        params![pinned_at, id],
    )
    .map_err(|e| e.to_string())?;

    if conn.changes() == 0 {
        return Err("待办不存在".into());
    }

    fetch_todo(&conn, id)
}

#[tauri::command]
pub fn add_todo_subtask(
    state: tauri::State<AppState>,
    todo_id: i64,
    title: String,
) -> Result<TodoItem, String> {
    let title = normalize_subtask_title(title)?;
    let created_at = Local::now().to_rfc3339();
    let conn = state.db.lock();
    let _existing = fetch_todo(&conn, todo_id)?;
    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM todo_subtasks WHERE todo_id = ?1",
            [todo_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT INTO todo_subtasks (todo_id, title, completed, sort_order, created_at)
         VALUES (?1, ?2, 0, ?3, ?4)",
        params![todo_id, title, sort_order, created_at],
    )
    .map_err(|e| e.to_string())?;

    fetch_todo(&conn, todo_id)
}

#[tauri::command]
pub fn set_todo_subtask_completed(
    state: tauri::State<AppState>,
    subtask_id: i64,
    completed: bool,
) -> Result<TodoItem, String> {
    let conn = state.db.lock();
    let todo_id: i64 = conn
        .query_row(
            "SELECT todo_id FROM todo_subtasks WHERE id = ?1",
            [subtask_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "子任务不存在".to_string())?;

    conn.execute(
        "UPDATE todo_subtasks SET completed = ?1 WHERE id = ?2",
        params![if completed { 1 } else { 0 }, subtask_id],
    )
    .map_err(|e| e.to_string())?;

    fetch_todo(&conn, todo_id)
}

#[tauri::command]
pub fn update_todo_subtask(
    state: tauri::State<AppState>,
    subtask_id: i64,
    title: String,
) -> Result<TodoItem, String> {
    let title = normalize_subtask_title(title)?;
    let conn = state.db.lock();
    let todo_id: i64 = conn
        .query_row(
            "SELECT todo_id FROM todo_subtasks WHERE id = ?1",
            [subtask_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "子任务不存在".to_string())?;

    conn.execute(
        "UPDATE todo_subtasks SET title = ?1 WHERE id = ?2",
        params![title, subtask_id],
    )
    .map_err(|e| e.to_string())?;

    fetch_todo(&conn, todo_id)
}

#[tauri::command]
pub fn delete_todo_subtask(
    state: tauri::State<AppState>,
    subtask_id: i64,
) -> Result<TodoItem, String> {
    let conn = state.db.lock();
    let todo_id: i64 = conn
        .query_row(
            "SELECT todo_id FROM todo_subtasks WHERE id = ?1",
            [subtask_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "子任务不存在".to_string())?;

    conn.execute("DELETE FROM todo_subtasks WHERE id = ?1", [subtask_id])
        .map_err(|e| e.to_string())?;

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

    conn.execute("DELETE FROM todo_note_images WHERE note_id = ?1", [note_id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM todo_notes WHERE id = ?1", [note_id])
        .map_err(|e| e.to_string())?;

    fetch_todo(&conn, todo_id)
}

#[tauri::command]
pub fn restore_todo_note(
    state: tauri::State<AppState>,
    note: TodoNote,
) -> Result<TodoItem, String> {
    let conn = state.db.lock();
    let _existing = fetch_todo(&conn, note.todo_id)?;

    conn.execute(
        "INSERT INTO todo_notes (id, todo_id, body, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![note.id, note.todo_id, note.body, note.created_at],
    )
    .map_err(|e| e.to_string())?;

    for image in note.images {
        conn.execute(
            "INSERT INTO todo_note_images (id, note_id, data_url, mime_type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                image.id,
                note.id,
                image.data_url,
                image.mime_type,
                image.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    fetch_todo(&conn, note.todo_id)
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
    conn.execute("DELETE FROM todo_subtasks WHERE todo_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM todos WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;

    if conn.changes() == 0 {
        return Err("待办不存在".into());
    }

    Ok(())
}

#[tauri::command]
pub fn restore_todo(state: tauri::State<AppState>, todo: TodoItem) -> Result<TodoItem, String> {
    let conn = state.db.lock();

    conn.execute(
        "INSERT INTO todos (id, title, content, completed, due_at, pinned_at, created_at, completed_at, recurrence, remind_1d, remind_1h, remind_custom_hours, recurrence_root_id, next_recurrence_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            todo.id,
            todo.title,
            todo.content,
            if todo.completed { 1 } else { 0 },
            todo.due_at,
            todo.pinned_at,
            todo.created_at,
            todo.completed_at,
            todo.recurrence,
            if todo.remind_1d { 1 } else { 0 },
            if todo.remind_1h { 1 } else { 0 },
            todo.remind_custom_hours,
            todo.recurrence_root_id,
            todo.next_recurrence_at,
        ],
    )
    .map_err(|e| e.to_string())?;

    for image in todo.images {
        conn.execute(
            "INSERT INTO todo_images (id, todo_id, data_url, mime_type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                image.id,
                todo.id,
                image.data_url,
                image.mime_type,
                image.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    for note in todo.notes {
        conn.execute(
            "INSERT INTO todo_notes (id, todo_id, body, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![note.id, todo.id, note.body, note.created_at],
        )
        .map_err(|e| e.to_string())?;

        for image in note.images {
            conn.execute(
                "INSERT INTO todo_note_images (id, note_id, data_url, mime_type, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    image.id,
                    note.id,
                    image.data_url,
                    image.mime_type,
                    image.created_at
                ],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    for subtask in todo.subtasks {
        conn.execute(
            "INSERT INTO todo_subtasks (id, todo_id, title, completed, sort_order, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                subtask.id,
                todo.id,
                subtask.title,
                if subtask.completed { 1 } else { 0 },
                subtask.sort_order,
                subtask.created_at
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    fetch_todo(&conn, todo.id)
}

#[tauri::command]
pub fn export_todos_backup(
    app: AppHandle,
    state: tauri::State<AppState>,
    path: String,
) -> Result<(), String> {
    let conn = state.db.lock();
    let mut todos = list_todos(&conn)?;
    let markdown_dir = markdown_images_dir(&app)?;
    let mut markdown_images = HashMap::<String, PathBuf>::new();

    for todo in &mut todos {
        todo.content =
            rewrite_markdown_images_for_backup(&todo.content, &markdown_dir, &mut markdown_images);
    }

    let backup = TodoBackupFile {
        format: "tempo.todos.v3".into(),
        exported_at: Local::now().to_rfc3339(),
        todos,
    };

    let mut entries = vec![ZipEntryInput {
        name: "todos.json".into(),
        data: serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?,
    }];

    let mut images = markdown_images.into_iter().collect::<Vec<_>>();
    images.sort_by(|a, b| a.0.cmp(&b.0));
    for (file_name, file_path) in images {
        if let Ok(data) = std::fs::read(&file_path) {
            entries.push(ZipEntryInput {
                name: format!("markdown-images/{file_name}"),
                data,
            });
        }
    }

    write_zip_archive(Path::new(&path), &entries)
}

#[tauri::command]
pub fn import_todos_backup(
    app: AppHandle,
    state: tauri::State<AppState>,
    path: String,
) -> Result<Vec<TodoItem>, String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let entries = read_backup_entries(&bytes)?;
    let backup_bytes = entries
        .get("todos.json")
        .ok_or_else(|| "备份文件缺少 todos.json".to_string())?;
    let backup: TodoBackupFile = serde_json::from_slice(backup_bytes).map_err(|e| e.to_string())?;

    if !backup.format.starts_with("tempo.todos.") {
        return Err("不是有效的待办备份文件".into());
    }

    let markdown_dir = markdown_images_dir(&app)?;
    std::fs::create_dir_all(&markdown_dir).map_err(|e| e.to_string())?;
    let mut markdown_image_urls = HashMap::<String, String>::new();

    for (name, data) in &entries {
        let Some(file_name) = backup_markdown_image_file_name(name) else {
            continue;
        };
        let target = unique_markdown_image_path(&markdown_dir, &file_name);
        std::fs::write(&target, data).map_err(|e| e.to_string())?;
        let image_url =
            markdown_image_url_for_path(&target).ok_or_else(|| "图片文件名无效".to_string())?;
        markdown_image_urls.insert(name.clone(), image_url);
    }

    let conn = state.db.lock();
    insert_imported_todos(&conn, &backup.todos, &markdown_image_urls)?;
    cleanup_unreferenced_markdown_images(&app, &conn);
    list_todos(&conn)
}

#[tauri::command]
pub fn save_markdown_image(
    app: AppHandle,
    data_url: String,
    mime_type: String,
) -> Result<String, String> {
    let mime = mime_type.trim().to_ascii_lowercase();
    let extension = markdown_image_extension(&mime)?;
    let prefix = format!("data:{mime};base64,");
    let payload = data_url
        .strip_prefix(&prefix)
        .ok_or_else(|| "图片数据格式无效".to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| "图片数据格式无效".to_string())?;

    if bytes.len() > MAX_TODO_IMAGE_BYTES {
        return Err("单张图片不能超过 5MB".into());
    }

    let dir = markdown_images_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let mut attempt = 0;
    loop {
        let timestamp = Local::now().timestamp_nanos_opt().unwrap_or_default();
        let file_name = format!(
            "todo-md-{timestamp}-{}-{attempt}.{extension}",
            std::process::id()
        );
        let path = dir.join(file_name);

        if path.exists() {
            attempt += 1;
            continue;
        }

        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        return markdown_image_url_for_path(&path).ok_or_else(|| "图片文件名无效".to_string());
    }
}

struct ZipEntryInput {
    name: String,
    data: Vec<u8>,
}

struct ZipCentralEntry {
    name: String,
    crc32: u32,
    size: u32,
    local_offset: u32,
}

fn write_zip_archive(path: &Path, entries: &[ZipEntryInput]) -> Result<(), String> {
    let mut data = Vec::<u8>::new();
    let mut central_entries = Vec::<ZipCentralEntry>::new();

    for entry in entries {
        let name = entry.name.as_bytes();
        if name.len() > u16::MAX as usize || entry.data.len() > u32::MAX as usize {
            return Err("备份文件过大".into());
        }

        let local_offset = data.len() as u32;
        let crc32 = crc32(&entry.data);
        let size = entry.data.len() as u32;

        push_u32(&mut data, 0x0403_4b50);
        push_u16(&mut data, 20);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u32(&mut data, crc32);
        push_u32(&mut data, size);
        push_u32(&mut data, size);
        push_u16(&mut data, name.len() as u16);
        push_u16(&mut data, 0);
        data.extend_from_slice(name);
        data.extend_from_slice(&entry.data);

        central_entries.push(ZipCentralEntry {
            name: entry.name.clone(),
            crc32,
            size,
            local_offset,
        });
    }

    let central_offset = data.len() as u32;
    for entry in &central_entries {
        let name = entry.name.as_bytes();
        push_u32(&mut data, 0x0201_4b50);
        push_u16(&mut data, 20);
        push_u16(&mut data, 20);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u32(&mut data, entry.crc32);
        push_u32(&mut data, entry.size);
        push_u32(&mut data, entry.size);
        push_u16(&mut data, name.len() as u16);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u32(&mut data, 0);
        push_u32(&mut data, entry.local_offset);
        data.extend_from_slice(name);
    }

    let central_size = data.len() as u32 - central_offset;
    if central_entries.len() > u16::MAX as usize {
        return Err("备份条目过多".into());
    }

    push_u32(&mut data, 0x0605_4b50);
    push_u16(&mut data, 0);
    push_u16(&mut data, 0);
    push_u16(&mut data, central_entries.len() as u16);
    push_u16(&mut data, central_entries.len() as u16);
    push_u32(&mut data, central_size);
    push_u32(&mut data, central_offset);
    push_u16(&mut data, 0);

    std::fs::write(path, data).map_err(|e| e.to_string())
}

fn read_backup_entries(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    match read_zip_archive(bytes) {
        Ok(entries) => Ok(entries),
        Err(_) => {
            let _: TodoBackupFile = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            Ok(HashMap::from([("todos.json".to_string(), bytes.to_vec())]))
        }
    }
}

fn read_zip_archive(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let eocd = find_eocd(bytes).ok_or_else(|| "备份文件格式无效".to_string())?;
    let entry_count = read_u16(bytes, eocd + 10)? as usize;
    let central_offset = read_u32(bytes, eocd + 16)? as usize;
    let mut cursor = central_offset;
    let mut entries = HashMap::new();

    for _ in 0..entry_count {
        if read_u32(bytes, cursor)? != 0x0201_4b50 {
            return Err("备份文件目录损坏".into());
        }

        let method = read_u16(bytes, cursor + 10)?;
        let compressed_size = read_u32(bytes, cursor + 20)? as usize;
        let name_len = read_u16(bytes, cursor + 28)? as usize;
        let extra_len = read_u16(bytes, cursor + 30)? as usize;
        let comment_len = read_u16(bytes, cursor + 32)? as usize;
        let local_offset = read_u32(bytes, cursor + 42)? as usize;
        let name_start = cursor + 46;
        let name_end = name_start + name_len;
        let name = std::str::from_utf8(
            bytes
                .get(name_start..name_end)
                .ok_or_else(|| "备份文件目录损坏".to_string())?,
        )
        .map_err(|_| "备份文件目录名称无效".to_string())?
        .replace('\\', "/");

        if method != 0 {
            return Err("备份文件使用了不支持的压缩方式".into());
        }
        if !is_safe_zip_name(&name) {
            return Err("备份文件包含不安全的路径".into());
        }

        if read_u32(bytes, local_offset)? != 0x0403_4b50 {
            return Err("备份文件内容损坏".into());
        }
        let local_name_len = read_u16(bytes, local_offset + 26)? as usize;
        let local_extra_len = read_u16(bytes, local_offset + 28)? as usize;
        let data_start = local_offset + 30 + local_name_len + local_extra_len;
        let data_end = data_start + compressed_size;
        let data = bytes
            .get(data_start..data_end)
            .ok_or_else(|| "备份文件内容损坏".to_string())?
            .to_vec();
        entries.insert(name, data);
        cursor = name_end + extra_len + comment_len;
    }

    Ok(entries)
}

fn insert_imported_todos(
    conn: &Connection,
    todos: &[TodoItem],
    markdown_image_urls: &HashMap<String, String>,
) -> Result<(), String> {
    for todo in todos {
        let content = restore_backup_markdown_image_urls(&todo.content, markdown_image_urls);
        let (recurrence, due_at, remind_1d, remind_1h, remind_custom_hours) =
            apply_recurrence_constraints(
                todo.recurrence.clone(),
                todo.due_at.clone(),
                todo.remind_1d,
                todo.remind_1h,
                todo.remind_custom_hours,
            )?;
        conn.execute(
            "INSERT INTO todos (title, content, completed, due_at, pinned_at, created_at, completed_at, recurrence, remind_1d, remind_1h, remind_custom_hours, recurrence_root_id, next_recurrence_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                todo.title,
                content,
                if todo.completed { 1 } else { 0 },
                due_at,
                todo.pinned_at,
                todo.created_at,
                todo.completed_at,
                recurrence,
                if remind_1d { 1 } else { 0 },
                if remind_1h { 1 } else { 0 },
                remind_custom_hours,
                todo.recurrence_root_id,
                todo.next_recurrence_at,
            ],
        )
        .map_err(|e| e.to_string())?;
        let todo_id = conn.last_insert_rowid();
        if recurrence != "none" {
            conn.execute(
                "UPDATE todos SET recurrence_root_id = COALESCE(recurrence_root_id, ?1) WHERE id = ?1",
                params![todo_id],
            )
            .map_err(|e| e.to_string())?;
        }

        for image in &todo.images {
            conn.execute(
                "INSERT INTO todo_images (todo_id, data_url, mime_type, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![todo_id, image.data_url, image.mime_type, image.created_at],
            )
            .map_err(|e| e.to_string())?;
        }

        for note in &todo.notes {
            conn.execute(
                "INSERT INTO todo_notes (todo_id, body, created_at) VALUES (?1, ?2, ?3)",
                params![todo_id, note.body, note.created_at],
            )
            .map_err(|e| e.to_string())?;
            let note_id = conn.last_insert_rowid();

            for image in &note.images {
                conn.execute(
                    "INSERT INTO todo_note_images (note_id, data_url, mime_type, created_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![note_id, image.data_url, image.mime_type, image.created_at],
                )
                .map_err(|e| e.to_string())?;
            }
        }

        for subtask in &todo.subtasks {
            conn.execute(
                "INSERT INTO todo_subtasks (todo_id, title, completed, sort_order, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    todo_id,
                    subtask.title,
                    if subtask.completed { 1 } else { 0 },
                    subtask.sort_order,
                    subtask.created_at
                ],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn rewrite_markdown_images_for_backup(
    content: &str,
    markdown_dir: &Path,
    markdown_images: &mut HashMap<String, PathBuf>,
) -> String {
    let mut next = content.to_string();
    for src in markdown_image_sources(content) {
        let Some((file_name, file_path)) = markdown_image_reference(&src, markdown_dir) else {
            continue;
        };
        markdown_images.insert(file_name.clone(), file_path);
        next = next.replace(&src, &format!("markdown-images/{file_name}"));
    }
    next
}

fn restore_backup_markdown_image_urls(
    content: &str,
    markdown_image_urls: &HashMap<String, String>,
) -> String {
    let mut next = content.to_string();
    for (relative, url) in markdown_image_urls {
        next = next.replace(relative, url);
    }
    next
}

fn cleanup_unreferenced_markdown_images(app: &AppHandle, conn: &Connection) {
    let Ok(markdown_dir) = markdown_images_dir(app) else {
        return;
    };
    let mut referenced = HashSet::<String>::new();

    if let Ok(mut stmt) = conn.prepare("SELECT content FROM todos") {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for content in rows.filter_map(|row| row.ok()) {
                for src in markdown_image_sources(&content) {
                    if let Some((file_name, _)) = markdown_image_reference(&src, &markdown_dir) {
                        referenced.insert(file_name);
                    }
                }
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(&markdown_dir) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !referenced.contains(file_name) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

fn markdown_image_sources(content: &str) -> Vec<String> {
    let mut sources = Vec::new();
    let mut rest = content;

    while let Some(start) = rest.find("![") {
        let after_start = &rest[start + 2..];
        let Some(label_end) = after_start.find("](") else {
            break;
        };
        let src_start = start + 2 + label_end + 2;
        let after_src = &rest[src_start..];
        let Some(src_end) = after_src.find(')') else {
            break;
        };
        let src = after_src[..src_end].trim();
        if !src.is_empty() {
            sources.push(src.to_string());
        }
        rest = &after_src[src_end + 1..];
    }

    sources
}

fn markdown_image_reference(src: &str, markdown_dir: &Path) -> Option<(String, PathBuf)> {
    if let Some(file_name) = markdown_image_file_name_from_url(src) {
        return Some((file_name.clone(), markdown_dir.join(file_name)));
    }

    let decoded = decode_asset_source_path(src);
    if let Some(relative) = decoded.strip_prefix("markdown-images/") {
        let file_name = backup_markdown_image_file_name(&format!("markdown-images/{relative}"))?;
        return Some((file_name.clone(), markdown_dir.join(file_name)));
    }

    let path = PathBuf::from(&decoded);
    let file_name = path.file_name()?.to_str()?.to_string();
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return None;
    }

    let canonical_path = path.canonicalize().ok()?;
    let canonical_markdown_dir = markdown_dir.canonicalize().ok()?;
    if !canonical_path.starts_with(&canonical_markdown_dir) {
        return None;
    }

    Some((file_name, canonical_path))
}

fn decode_asset_source_path(src: &str) -> String {
    let path_part = src
        .strip_prefix("http://asset.localhost/")
        .or_else(|| src.strip_prefix("https://asset.localhost/"))
        .or_else(|| src.strip_prefix("asset://localhost/"))
        .unwrap_or(src);
    percent_decode(path_part).replace('\\', "/")
}

fn backup_markdown_image_file_name(name: &str) -> Option<String> {
    let rest = name.strip_prefix("markdown-images/")?;
    if rest.is_empty()
        || rest.contains('/')
        || rest.contains('\\')
        || rest.contains("..")
        || rest.contains('\0')
    {
        return None;
    }
    Some(rest.to_string())
}

fn unique_markdown_image_path(markdown_dir: &Path, file_name: &str) -> PathBuf {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("image");
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("png");

    for attempt in 0..10_000 {
        let candidate = if attempt == 0 {
            markdown_dir.join(format!("{stem}.{extension}"))
        } else {
            markdown_dir.join(format!("{stem}-import-{attempt}.{extension}"))
        };
        if !candidate.exists() {
            return candidate;
        }
    }

    markdown_dir.join(format!(
        "{stem}-import-{}.{extension}",
        Local::now().timestamp_nanos_opt().unwrap_or_default()
    ))
}

fn markdown_images_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(current_storage_dir(app)?.join("markdown-images"))
}

pub fn markdown_image_protocol_response(
    app: &AppHandle,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return empty_markdown_image_response(StatusCode::METHOD_NOT_ALLOWED);
    }

    let Some(file_name) = markdown_image_file_name_from_request_path(request.uri().path()) else {
        return empty_markdown_image_response(StatusCode::BAD_REQUEST);
    };
    let Some(content_type) = markdown_image_content_type(&file_name) else {
        return empty_markdown_image_response(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    };

    let markdown_dir = match markdown_images_dir(app) {
        Ok(dir) => dir,
        Err(_) => return empty_markdown_image_response(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let path = markdown_dir.join(&file_name);
    let canonical_path = match path.canonicalize() {
        Ok(path) => path,
        Err(_) => return empty_markdown_image_response(StatusCode::NOT_FOUND),
    };
    let canonical_markdown_dir = match markdown_dir.canonicalize() {
        Ok(path) => path,
        Err(_) => return empty_markdown_image_response(StatusCode::NOT_FOUND),
    };
    if !canonical_path.starts_with(&canonical_markdown_dir) {
        return empty_markdown_image_response(StatusCode::FORBIDDEN);
    }

    if request.method() == Method::HEAD {
        return Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, content_type)
            .body(Vec::new())
            .unwrap();
    }

    match std::fs::read(canonical_path) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, content_type)
            .header(CONTENT_LENGTH, bytes.len())
            .body(bytes)
            .unwrap(),
        Err(_) => empty_markdown_image_response(StatusCode::NOT_FOUND),
    }
}

fn empty_markdown_image_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(status).body(Vec::new()).unwrap()
}

fn markdown_image_url_for_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let valid_file_name = backup_markdown_image_file_name(&format!("markdown-images/{file_name}"))?;
    markdown_image_content_type(&valid_file_name)?;
    Some(markdown_image_url_for_file_name(&valid_file_name))
}

fn markdown_image_url_for_file_name(file_name: &str) -> String {
    let encoded = percent_encode(file_name);
    if cfg!(target_os = "windows") {
        format!("http://{MARKDOWN_IMAGE_PROTOCOL}.localhost/{encoded}")
    } else {
        format!("{MARKDOWN_IMAGE_PROTOCOL}://localhost/{encoded}")
    }
}

fn percent_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~') {
            output.push(*byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

fn percent_decode(value: &str) -> String {
    let mut bytes = Vec::new();
    let mut index = 0;
    let raw = value.as_bytes();
    while index < raw.len() {
        if raw[index] == b'%' && index + 2 < raw.len() {
            if let Ok(hex) = std::str::from_utf8(&raw[index + 1..index + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    bytes.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        bytes.push(raw[index]);
        index += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn markdown_image_file_name_from_request_path(path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');
    let decoded = percent_decode(path);
    backup_markdown_image_file_name(&format!("markdown-images/{decoded}"))
}

fn markdown_image_file_name_from_url(src: &str) -> Option<String> {
    let windows_prefix = format!("http://{MARKDOWN_IMAGE_PROTOCOL}.localhost/");
    let windows_https_prefix = format!("https://{MARKDOWN_IMAGE_PROTOCOL}.localhost/");
    let unix_prefix = format!("{MARKDOWN_IMAGE_PROTOCOL}://localhost/");
    let path = src
        .strip_prefix(&windows_prefix)
        .or_else(|| src.strip_prefix(&windows_https_prefix))
        .or_else(|| src.strip_prefix(&unix_prefix))?;
    let path = path
        .split_once(['?', '#'])
        .map(|(value, _)| value)
        .unwrap_or(path);
    let decoded = percent_decode(path);
    backup_markdown_image_file_name(&format!("markdown-images/{decoded}"))
}

fn markdown_image_content_type(file_name: &str) -> Option<&'static str> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

fn is_safe_zip_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.contains(':')
        && !name.contains('\0')
        && !name.split('/').any(|part| part == ".." || part.is_empty())
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    (0..=bytes.len() - 22)
        .rev()
        .find(|index| bytes.get(*index..*index + 4) == Some(&[0x50, 0x4b, 0x05, 0x06]))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "备份文件内容损坏".to_string())?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "备份文件内容损坏".to_string())?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn push_u16(data: &mut Vec<u8>, value: u16) {
    data.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(data: &mut Vec<u8>, value: u32) {
    data.extend_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
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
pub fn set_pomodoro_todo(
    app: AppHandle,
    state: tauri::State<AppState>,
    todo_id: Option<i64>,
) -> Result<PomodoroState, String> {
    let snapshot = crate::pomodoro::set_pomodoro_todo(&state, todo_id)?;
    crate::pomodoro::push_pomodoro_update(&app, &state);
    Ok(snapshot)
}

#[tauri::command]
pub fn start_pomodoro(
    app: AppHandle,
    state: tauri::State<AppState>,
    todo_id: Option<i64>,
) -> Result<PomodoroState, String> {
    let snapshot = crate::pomodoro::start_pomodoro(&state, todo_id)?;
    crate::pomodoro::push_pomodoro_update(&app, &state);
    Ok(snapshot)
}

#[tauri::command]
pub fn get_todo_focus_summary(
    state: tauri::State<AppState>,
    todo_id: i64,
) -> crate::db::TodoFocusSummary {
    let conn = state.db.lock();
    crate::db::get_todo_focus_summary(&conn, todo_id)
}

#[tauri::command]
pub fn get_todo_focus_summaries(
    state: tauri::State<AppState>,
    todo_ids: Vec<i64>,
) -> Vec<crate::db::TodoFocusSummary> {
    let conn = state.db.lock();
    crate::db::get_todo_focus_summaries(&conn, &todo_ids)
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
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    if normalized.is_empty() {
        if allow_image_only {
            return Ok("图片待办".into());
        }
        return Err("请输入标题".into());
    }

    if normalized.chars().count() > 120 {
        return Err("待办标题不能超过 120 个字".into());
    }

    Ok(normalized)
}

fn normalize_todo_content(content: String) -> String {
    content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
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
        return Err(format!("每条备注最多添加 {} 张图片", MAX_TODO_NOTE_IMAGES));
    }

    validate_todo_image_inputs(&images)?;
    Ok(images)
}

fn markdown_image_extension(mime_type: &str) -> Result<&'static str, String> {
    match mime_type {
        "image/png" => Ok("png"),
        "image/jpeg" => Ok("jpg"),
        "image/webp" => Ok("webp"),
        "image/gif" => Ok("gif"),
        _ => Err("仅支持 PNG、JPEG、WebP 或 GIF 图片".into()),
    }
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
            "SELECT id, title, content, completed, due_at, pinned_at, created_at, completed_at,
                    recurrence, remind_1d, remind_1h, remind_custom_hours,
                    recurrence_root_id, next_recurrence_at
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
    hydrate_todo_subtasks(conn, std::slice::from_mut(&mut todo))?;
    Ok(todo)
}

fn list_todos(conn: &Connection) -> Result<Vec<TodoItem>, String> {
    let mut todos = {
        let mut stmt = conn
            .prepare(
                "SELECT id, title, content, completed, due_at, pinned_at, created_at, completed_at,
                        recurrence, remind_1d, remind_1h, remind_custom_hours,
                        recurrence_root_id, next_recurrence_at
                 FROM todos
                 ORDER BY completed ASC,
                   CASE WHEN completed = 0 AND pinned_at IS NOT NULL THEN 0 ELSE 1 END ASC,
                   CASE WHEN completed = 0 THEN datetime(pinned_at) END DESC,
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

    hydrate_todo_images(conn, &mut todos)?;
    hydrate_todo_notes(conn, &mut todos)?;
    hydrate_todo_subtasks(conn, &mut todos)?;
    Ok(todos)
}

fn todo_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodoItem> {
    Ok(TodoItem {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        completed: row.get::<_, i64>(3)? != 0,
        due_at: row.get(4)?,
        pinned_at: row.get(5)?,
        created_at: row.get(6)?,
        completed_at: row.get(7)?,
        recurrence: row
            .get::<_, Option<String>>(8)?
            .unwrap_or_else(|| "none".into()),
        remind_1d: row.get::<_, i64>(9)? != 0,
        remind_1h: row.get::<_, i64>(10)? != 0,
        remind_custom_hours: row.get(11)?,
        recurrence_root_id: row.get(12)?,
        next_recurrence_at: row.get(13)?,
        images: Vec::new(),
        notes: Vec::new(),
        subtasks: Vec::new(),
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

fn hydrate_todo_subtasks(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
    for todo in todos {
        let mut stmt = conn
            .prepare(
                "SELECT id, todo_id, title, completed, sort_order, created_at
                 FROM todo_subtasks
                 WHERE todo_id = ?1
                 ORDER BY sort_order ASC, id ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([todo.id], |row| {
                Ok(TodoSubtask {
                    id: row.get(0)?,
                    todo_id: row.get(1)?,
                    title: row.get(2)?,
                    completed: row.get::<_, i64>(3)? != 0,
                    sort_order: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;

        todo.subtasks = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn insert_subtasks(conn: &Connection, todo_id: i64, titles: &[String]) -> Result<(), String> {
    let created_at = Local::now().to_rfc3339();
    for (index, title) in titles.iter().enumerate() {
        conn.execute(
            "INSERT INTO todo_subtasks (todo_id, title, completed, sort_order, created_at)
             VALUES (?1, ?2, 0, ?3, ?4)",
            params![todo_id, title, index as i64, created_at],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn encode_subtask_completion_snapshot(subtasks: &[TodoSubtask]) -> String {
    let snapshot = subtasks
        .iter()
        .map(|subtask| {
            json!({
                "id": subtask.id,
                "completed": subtask.completed,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&snapshot).unwrap_or_else(|_| "[]".into())
}

fn restore_subtask_completion_snapshot(
    conn: &Connection,
    todo_id: i64,
    snapshot: &str,
) -> Result<(), String> {
    let entries =
        serde_json::from_str::<Vec<serde_json::Value>>(snapshot).map_err(|e| e.to_string())?;

    for entry in entries {
        let Some(subtask_id) = entry.get("id").and_then(|value| value.as_i64()) else {
            continue;
        };
        let completed = entry
            .get("completed")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        conn.execute(
            "UPDATE todo_subtasks SET completed = ?1 WHERE id = ?2 AND todo_id = ?3",
            params![if completed { 1 } else { 0 }, subtask_id, todo_id],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn spawn_recurring_todo(
    conn: &Connection,
    source: &TodoItem,
    root_id: i64,
) -> Result<TodoItem, String> {
    let created_at = Local::now().to_rfc3339();
    conn.execute(
        "INSERT INTO todos (title, content, completed, due_at, recurrence, remind_1d, remind_1h, remind_custom_hours, recurrence_root_id, created_at)
         VALUES (?1, ?2, 0, NULL, ?3, 0, 0, NULL, ?4, ?5)",
        params![
            source.title,
            source.content,
            source.recurrence,
            root_id,
            created_at
        ],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    let subtask_titles = source
        .subtasks
        .iter()
        .map(|subtask| subtask.title.clone())
        .collect::<Vec<_>>();
    insert_subtasks(conn, id, &subtask_titles)?;
    fetch_todo(conn, id)
}

fn process_pending_recurrences(conn: &Connection) -> Result<Vec<TodoItem>, String> {
    let now = Local::now();
    let pending = list_due_recurrence_spawns(conn, now)?;
    let mut spawned = Vec::new();

    for (completed_id, root_id) in pending {
        if has_active_recurrence_instance(conn, root_id)? {
            conn.execute(
                "UPDATE todos SET next_recurrence_at = NULL WHERE id = ?1",
                [completed_id],
            )
            .map_err(|e| e.to_string())?;
            continue;
        }

        let source = fetch_todo(conn, completed_id)?;
        let new_todo = spawn_recurring_todo(conn, &source, root_id)?;
        conn.execute(
            "UPDATE todos SET next_recurrence_at = NULL WHERE id = ?1",
            [completed_id],
        )
        .map_err(|e| e.to_string())?;
        spawned.push(new_todo);
    }

    Ok(spawned)
}

fn list_due_recurrence_spawns(
    conn: &Connection,
    now: DateTime<Local>,
) -> Result<Vec<(i64, i64)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, COALESCE(recurrence_root_id, id), next_recurrence_at
             FROM todos
             WHERE completed = 1
               AND recurrence != 'none'
               AND next_recurrence_at IS NOT NULL",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut pending = Vec::new();
    for row in rows {
        let (id, root_id, next_at) = row.map_err(|e| e.to_string())?;
        let Ok(next_dt) = DateTime::parse_from_rfc3339(&next_at) else {
            continue;
        };
        if next_dt.with_timezone(&Local) <= now {
            pending.push((id, root_id));
        }
    }

    Ok(pending)
}

fn has_active_recurrence_instance(conn: &Connection, root_id: i64) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM todos WHERE recurrence_root_id = ?1 AND completed = 0",
            [root_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

fn next_recurrence_midnight(from: DateTime<Local>, recurrence: &str) -> Option<String> {
    let date = from.date_naive();
    let next_date = match recurrence {
        "daily" => date + ChronoDuration::days(1),
        "weekly" => date + ChronoDuration::weeks(1),
        "monthly" => date + ChronoDuration::days(30),
        _ => return None,
    };
    next_date
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| naive.and_local_timezone(Local).single())
        .map(|value| value.to_rfc3339())
}

fn apply_recurrence_constraints(
    recurrence: String,
    due_at: Option<String>,
    remind_1d: bool,
    remind_1h: bool,
    remind_custom_hours: Option<i64>,
) -> Result<(String, Option<String>, bool, bool, Option<i64>), String> {
    let recurrence = normalize_recurrence(recurrence)?;
    if recurrence != "none" {
        if due_at.is_some() {
            return Err("重复待办不能设置截止时间".into());
        }
        return Ok((recurrence, None, false, false, None));
    }

    let remind_custom_hours = normalize_remind_custom_hours(remind_custom_hours, due_at.is_some())?;
    let has_due_at = due_at.is_some();
    Ok((
        recurrence,
        due_at,
        remind_1d && has_due_at,
        remind_1h && has_due_at,
        remind_custom_hours,
    ))
}

fn normalize_recurrence(recurrence: String) -> Result<String, String> {
    let normalized = recurrence.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "none" => Ok("none".into()),
        "daily" | "weekly" | "monthly" => Ok(normalized),
        _ => Err("重复规则无效".into()),
    }
}

fn normalize_remind_custom_hours(
    value: Option<i64>,
    has_due_at: bool,
) -> Result<Option<i64>, String> {
    if !has_due_at {
        return Ok(None);
    }
    let Some(hours) = value else {
        return Ok(None);
    };
    if !(1..=168).contains(&hours) {
        return Err("自定义提醒需在 1-168 小时之间".into());
    }
    Ok(Some(hours))
}

fn normalize_subtask_titles(titles: Option<Vec<String>>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for title in titles.unwrap_or_default() {
        let title = normalize_subtask_title(title)?;
        if normalized.len() >= 20 {
            return Err("每个待办最多添加 20 个子任务".into());
        }
        normalized.push(title);
    }
    Ok(normalized)
}

fn normalize_subtask_title(title: String) -> Result<String, String> {
    let normalized = title.trim().to_string();
    if normalized.is_empty() {
        return Err("子任务标题不能为空".into());
    }
    if normalized.chars().count() > 120 {
        return Err("子任务标题不能超过 120 个字".into());
    }
    Ok(normalized)
}

fn mark_due_reminder_sent(conn: &Connection, id: i64, flag: &str) -> Result<(), String> {
    match flag {
        "due_reminded_1d" => {
            conn.execute("UPDATE todos SET due_reminded_1d = 1 WHERE id = ?1", [id])
        }
        "due_reminded_1h" => {
            conn.execute("UPDATE todos SET due_reminded_1h = 1 WHERE id = ?1", [id])
        }
        "due_reminded_at" => {
            conn.execute("UPDATE todos SET due_reminded_at = 1 WHERE id = ?1", [id])
        }
        "due_reminded_custom" => conn.execute(
            "UPDATE todos SET due_reminded_custom = 1 WHERE id = ?1",
            [id],
        ),
        _ => return Ok(()),
    }
    .map_err(|e| e.to_string())?;
    Ok(())
}
