use crate::clipboard_db::{
    clear_clipboard_history, delete_clipboard_entry, list_clipboard_entries,
    set_clipboard_entry_pinned, ClipboardEntry,
};
use crate::clipboard_watcher::{copy_clipboard_entry_by_id, write_clipboard_text};
use crate::db::AppState;

#[tauri::command]
pub fn get_clipboard_history(
    state: tauri::State<AppState>,
    query: Option<String>,
    limit: Option<u32>,
) -> Vec<ClipboardEntry> {
    let conn = state.db.lock();
    list_clipboard_entries(&conn, query.as_deref(), limit.unwrap_or(200).min(500))
}

#[tauri::command]
pub fn delete_clipboard_history_entry(
    state: tauri::State<AppState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.db.lock();
    if delete_clipboard_entry(&conn, id) {
        Ok(())
    } else {
        Err("记录不存在".into())
    }
}

#[tauri::command]
pub fn clear_clipboard_history_command(state: tauri::State<AppState>) -> Result<u32, String> {
    let conn = state.db.lock();
    Ok(clear_clipboard_history(&conn))
}

#[tauri::command]
pub fn pin_clipboard_history_entry(
    state: tauri::State<AppState>,
    id: i64,
    pinned: bool,
) -> Result<ClipboardEntry, String> {
    let conn = state.db.lock();
    set_clipboard_entry_pinned(&conn, id, pinned).ok_or_else(|| "记录不存在".into())
}

#[tauri::command]
pub fn copy_text_to_clipboard(state: tauri::State<AppState>, text: String) -> Result<(), String> {
    write_clipboard_text(&state, &text)
}

#[tauri::command]
pub fn copy_clipboard_entry(state: tauri::State<AppState>, id: i64) -> Result<(), String> {
    copy_clipboard_entry_by_id(&state, id)
}
