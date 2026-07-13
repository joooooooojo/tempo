use crate::clipboard_db::{
    clear_clipboard_history, count_clipboard_entries, delete_clipboard_entry,
    list_clipboard_entries, set_clipboard_entry_pinned, ClipboardEntry,
};
use crate::clipboard_images::{hydrate_clipboard_image_urls, maybe_delete_clipboard_image_file};
use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ClipboardHistoryPage {
    pub entries: Vec<ClipboardEntry>,
    pub total: u32,
    pub has_more: bool,
}
use crate::clipboard_watcher::prewarm_clipboard_image_cache;
use crate::clipboard_watcher::{copy_clipboard_entry_by_id, write_clipboard_text};
use crate::db::AppState;

#[cfg(not(target_os = "windows"))]
fn hydrate_clipboard_icons(
    app: &tauri::AppHandle,
    _conn: &Connection,
    entries: &mut [ClipboardEntry],
) {
    for entry in entries.iter_mut() {
        let app_name = entry.source_app.as_deref().unwrap_or("").trim();
        if app_name.is_empty() {
            continue;
        }
        let process_name = entry
            .source_process
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(app_name);
        entry.source_icon_data_url =
            crate::app_icons::resolve_app_icon_protocol_url(app, app_name, process_name);
    }
}

#[cfg(target_os = "windows")]
fn hydrate_clipboard_icons(
    _app: &tauri::AppHandle,
    conn: &Connection,
    entries: &mut [ClipboardEntry],
) {
    use std::collections::HashMap;

    let mut icon_cache = HashMap::<(String, String), Option<String>>::new();
    for entry in entries.iter_mut() {
        let app_name = entry.source_app.as_deref().unwrap_or("").trim();
        if app_name.is_empty() {
            continue;
        }
        let process_name = entry
            .source_process
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(app_name)
            .trim();

        let cache_key = (
            app_name.to_ascii_lowercase(),
            process_name.to_ascii_lowercase(),
        );
        let icon_value = icon_cache
            .entry(cache_key)
            .or_insert_with(|| lookup_existing_app_icon(conn, app_name, process_name));
        entry.source_icon_data_url = icon_value
            .as_deref()
            .and_then(crate::app_icons::hydrate_app_icon_url);
    }
}

#[cfg(target_os = "windows")]
fn lookup_existing_app_icon(
    conn: &Connection,
    app_name: &str,
    process_name: &str,
) -> Option<String> {
    conn.query_row(
        "SELECT icon_data_url
         FROM app_usage
         WHERE icon_data_url IS NOT NULL
           AND icon_data_url <> ''
           AND (app_name = ?1 OR process_name = ?1 OR app_name = ?2 OR process_name = ?2)
         ORDER BY date DESC
         LIMIT 1",
        rusqlite::params![app_name, process_name],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

#[tauri::command]
pub fn get_clipboard_history(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    query: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ClipboardHistoryPage {
    let conn = state.db.lock();
    let limit = limit.unwrap_or(200).min(500);
    let offset = offset.unwrap_or(0);
    let total = count_clipboard_entries(&conn, query.as_deref());
    let mut entries = list_clipboard_entries(&conn, query.as_deref(), limit, offset);
    hydrate_clipboard_icons(&app, &conn, &mut entries);
    hydrate_clipboard_image_urls(&app, &conn, &mut entries);
    drop(conn);
    let image_contents = entries
        .iter()
        .filter(|entry| entry.kind == "image")
        .map(|entry| entry.content.clone())
        .collect::<Vec<_>>();
    if !image_contents.is_empty() {
        prewarm_clipboard_image_cache(app.clone(), state.inner().clone(), image_contents);
    }
    let loaded = offset.saturating_add(entries.len() as u32);
    ClipboardHistoryPage {
        has_more: loaded < total,
        total,
        entries,
    }
}

#[tauri::command]
pub fn delete_clipboard_history_entry(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.db.lock();
    match delete_clipboard_entry(&conn, id) {
        Ok(image_content) => {
            drop(conn);
            if let Some(content) = image_content {
                let conn = state.db.lock();
                maybe_delete_clipboard_image_file(&conn, &app, &content);
            }
            Ok(())
        }
        Err(()) => Err("记录不存在".into()),
    }
}

#[tauri::command]
pub fn clear_clipboard_history_command(state: tauri::State<AppState>) -> Result<u32, String> {
    let conn = state.db.lock();
    Ok(clear_clipboard_history(&conn))
}

#[tauri::command]
pub fn pin_clipboard_history_entry(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
    pinned: bool,
) -> Result<ClipboardEntry, String> {
    let conn = state.db.lock();
    let mut entry =
        set_clipboard_entry_pinned(&conn, id, pinned).ok_or("记录不存在".to_string())?;
    hydrate_clipboard_icons(&app, &conn, std::slice::from_mut(&mut entry));
    hydrate_clipboard_image_urls(&app, &conn, std::slice::from_mut(&mut entry));
    drop(conn);
    Ok(entry)
}

#[tauri::command]
pub fn copy_text_to_clipboard(state: tauri::State<AppState>, text: String) -> Result<(), String> {
    write_clipboard_text(&state, &text)
}

#[tauri::command]
pub fn copy_clipboard_entry(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
) -> Result<(), String> {
    #[cfg(debug_assertions)]
    eprintln!("[tempo-debug][clipboard-command] copy_clipboard_entry id={id}");
    copy_clipboard_entry_by_id(&state, &app, id)
}
