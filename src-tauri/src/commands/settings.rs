use crate::clipboard_db::purge_clipboard_history_by_retention;
use crate::db::{
    current_storage_dir, default_storage_dir, load_settings, save_storage_dir, today_str, AppState,
    Settings,
};
use crate::db::{normalize_clipboard_history_retention, normalize_clipboard_paste_mode};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use super::markdown::{
    markdown_image_reference, markdown_image_sources, markdown_image_url_for_path,
};

#[tauri::command]
pub fn get_settings(app: AppHandle, state: tauri::State<AppState>) -> Settings {
    let conn = state.db.lock();
    let mut settings = load_settings(&conn);
    settings.storage_dir = current_storage_dir(&app)
        .or_else(|_| default_storage_dir(&app))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|error| {
            tracing::debug!(error = %error, "failed to resolve settings storage directory");
            String::new()
        });
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
    if let Some(v) = settings
        .get("pomodoro_float_enabled")
        .and_then(|v| v.as_bool())
    {
        current.pomodoro_float_enabled = v;
        if !v {
            crate::logging::debug_if_err(
                crate::auxiliary_windows::hide_pomodoro_float_window(&app),
                "hide pomodoro float after disabling setting",
            );
        }
    }
    if let Some(v) = settings
        .get("pomodoro_float_auto_show")
        .and_then(|v| v.as_bool())
    {
        current.pomodoro_float_auto_show = v;
    }
    if let Some(v) = settings
        .get("clipboard_monitor_enabled")
        .and_then(|v| v.as_bool())
    {
        current.clipboard_monitor_enabled = v;
    }
    if let Some(v) = settings
        .get("clipboard_max_entries")
        .and_then(|v| v.as_u64())
    {
        current.clipboard_max_entries = (v as u32).clamp(1, 1000);
    }
    if let Some(v) = settings
        .get("clipboard_paste_mode")
        .and_then(|v| v.as_str())
    {
        current.clipboard_paste_mode = normalize_clipboard_paste_mode(v);
    }
    if let Some(v) = settings
        .get("clipboard_plain_text_only")
        .and_then(|v| v.as_bool())
    {
        current.clipboard_plain_text_only = v;
    }
    let retention_changed = settings
        .get("clipboard_history_retention")
        .and_then(|v| v.as_str())
        .map(|value| {
            let next = normalize_clipboard_history_retention(value);
            let changed = current.clipboard_history_retention != next;
            current.clipboard_history_retention = next;
            changed
        })
        .unwrap_or(false);

    let mut shortcuts_changed = false;
    if let Some(v) = settings
        .get("shortcut_quick_todo")
        .and_then(|v| v.as_str())
    {
        if current.shortcut_quick_todo != v {
            current.shortcut_quick_todo = v.into();
            shortcuts_changed = true;
        }
    }
    if let Some(v) = settings
        .get("shortcut_clipboard_picker")
        .and_then(|v| v.as_str())
    {
        if current.shortcut_clipboard_picker != v {
            current.shortcut_clipboard_picker = v.into();
            shortcuts_changed = true;
        }
    }
    if let Some(v) = settings
        .get("shortcut_snippet_picker")
        .and_then(|v| v.as_str())
    {
        if current.shortcut_snippet_picker != v {
            current.shortcut_snippet_picker = v.into();
            shortcuts_changed = true;
        }
    }

    if shortcuts_changed {
        let (quick, clipboard, snippet) = crate::validate_shortcut_bindings(
            &current.shortcut_quick_todo,
            &current.shortcut_clipboard_picker,
            &current.shortcut_snippet_picker,
        )?;
        current.shortcut_quick_todo = quick;
        current.shortcut_clipboard_picker = clipboard;
        current.shortcut_snippet_picker = snippet;
        crate::apply_global_shortcuts(
            &app,
            &current.shortcut_quick_todo,
            &current.shortcut_clipboard_picker,
            &current.shortcut_snippet_picker,
        )?;
    }

    let conn = state.db.lock();
    crate::db::save_settings(&conn, &current);
    if retention_changed {
        purge_clipboard_history_by_retention(&conn, &current.clipboard_history_retention);
    }
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
    let current_clipboard_images_dir = current_dir.join("clipboard-images");
    let canonical_current_markdown_dir = current_markdown_dir
        .canonicalize()
        .unwrap_or_else(|_| current_markdown_dir.clone());
    let canonical_current_clipboard_images_dir = current_clipboard_images_dir
        .canonicalize()
        .unwrap_or_else(|_| current_clipboard_images_dir.clone());
    if path_is_within_or_same(&target_dir, &canonical_current_markdown_dir)
        || path_is_within_or_same(&target_dir, &canonical_current_clipboard_images_dir)
    {
        return Err("请选择数据目录之外的位置".into());
    }

    let current_db = current_dir.join("screen_time.db");
    let target_db = target_dir.join("screen_time.db");
    if target_db.exists() {
        return Err("目标位置已存在 Tempo 数据，请选择一个空目录".into());
    }

    let target_markdown_dir = target_dir.join("markdown-images");
    let target_clipboard_images_dir = target_dir.join("clipboard-images");

    let mut conn_guard = state.db.lock();
    if let Err(error) = conn_guard.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);") {
        tracing::warn!(error = %error, "failed to checkpoint database before storage migration");
    }
    vacuum_database_into(&conn_guard, &target_db)?;
    copy_dir_contents(&current_markdown_dir, &target_markdown_dir)?;
    copy_dir_contents(&current_clipboard_images_dir, &target_clipboard_images_dir)?;

    let next_conn = crate::db::init_db(&target_db)?;
    rewrite_markdown_storage_urls(&next_conn, &current_markdown_dir, &target_markdown_dir)?;
    save_storage_dir(&app, &target_dir)?;
    *conn_guard = next_conn;
    drop(conn_guard);

    cleanup_old_storage_files(
        &current_dir,
        &target_dir,
        &current_db,
        &current_markdown_dir,
        &current_clipboard_images_dir,
    );

    Ok(settings_with_storage_dir(&app, &state))
}
fn settings_with_storage_dir(app: &AppHandle, state: &AppState) -> Settings {
    let conn = state.db.lock();
    let mut settings = load_settings(&conn);
    settings.storage_dir = current_storage_dir(app)
        .or_else(|_| default_storage_dir(app))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|error| {
            tracing::debug!(error = %error, "failed to resolve settings storage directory");
            String::new()
        });
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
            .is_some_and(|name| name == "clipboard-images")
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
    old_clipboard_images_dir: &Path,
) {
    if same_path(old_dir, new_dir) {
        return;
    }

    remove_old_storage_file(old_db, "old database file");
    remove_old_storage_file(&old_dir.join("screen_time.db-wal"), "old database wal file");
    remove_old_storage_file(&old_dir.join("screen_time.db-shm"), "old database shm file");
    remove_old_storage_dir(old_markdown_dir, "old markdown image directory");
    remove_old_storage_dir(old_clipboard_images_dir, "old clipboard image directory");
}

fn remove_old_storage_file(path: &Path, label: &'static str) {
    if !path.exists() {
        return;
    }
    if let Err(error) = std::fs::remove_file(path) {
        tracing::warn!(label = %label, error = %error, "failed to remove old storage file");
    }
}

fn remove_old_storage_dir(path: &Path, label: &'static str) {
    if !path.exists() {
        return;
    }
    if let Err(error) = std::fs::remove_dir_all(path) {
        tracing::warn!(label = %label, error = %error, "failed to remove old storage directory");
    }
}
#[tauri::command]
pub fn reset_today(state: tauri::State<AppState>) {
    do_reset_today(&state);
}

pub fn do_reset_today(state: &AppState) {
    let conn = state.db.lock();
    let today = today_str();
    if let Err(error) = conn.execute("DELETE FROM screen_time_daily WHERE date = ?1", [&today]) {
        tracing::warn!(error = %error, "failed to reset today's daily screen time");
    }
    if let Err(error) = conn.execute("DELETE FROM screen_time_hourly WHERE date = ?1", [&today]) {
        tracing::warn!(error = %error, "failed to reset today's hourly screen time");
    }
    if let Err(error) = conn.execute("DELETE FROM app_usage WHERE date = ?1", [&today]) {
        tracing::warn!(error = %error, "failed to reset today's app usage");
    }
}

#[tauri::command]
pub fn reset_all(state: tauri::State<AppState>) {
    let conn = state.db.lock();
    if let Err(error) = conn.execute("DELETE FROM screen_time_daily", []) {
        tracing::warn!(error = %error, "failed to reset all daily screen time");
    }
    if let Err(error) = conn.execute("DELETE FROM screen_time_hourly", []) {
        tracing::warn!(error = %error, "failed to reset all hourly screen time");
    }
    if let Err(error) = conn.execute("DELETE FROM app_usage", []) {
        tracing::warn!(error = %error, "failed to reset all app usage");
    }
}
#[tauri::command]
pub fn complete_onboarding(state: tauri::State<AppState>) {
    let conn = state.db.lock();
    crate::db::set_setting(&conn, "onboarding_completed", "true");
}
