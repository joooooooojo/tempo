use crate::clipboard_db::{add_snippet, delete_snippet, get_snippet, list_snippets, update_snippet, Snippet};
use crate::clipboard_watcher::use_clipboard_text;
use crate::db::AppState;
use tauri::Emitter;

#[tauri::command]
pub fn get_snippets(state: tauri::State<AppState>, query: Option<String>) -> Vec<Snippet> {
    let conn = state.db.lock();
    list_snippets(&conn, query.as_deref())
}

#[tauri::command]
pub fn create_snippet(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    title: String,
    content: String,
    tags: Vec<String>,
) -> Result<Snippet, String> {
    let conn = state.db.lock();
    let snippet = add_snippet(&conn, &title, &content, &tags).ok_or_else(|| "标题和内容不能为空".to_string())?;
    let _ = app.emit("snippets-update", ());
    Ok(snippet)
}

#[tauri::command]
pub fn update_snippet_command(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
    title: String,
    content: String,
    tags: Vec<String>,
) -> Result<Snippet, String> {
    let conn = state.db.lock();
    let snippet =
        update_snippet(&conn, id, &title, &content, &tags).ok_or_else(|| "短语不存在或内容无效".to_string())?;
    let _ = app.emit("snippets-update", ());
    Ok(snippet)
}

#[tauri::command]
pub fn delete_snippet_command(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.db.lock();
    if delete_snippet(&conn, id) {
        let _ = app.emit("snippets-update", ());
        Ok(())
    } else {
        Err("短语不存在".into())
    }
}

#[tauri::command]
pub fn copy_snippet_to_clipboard(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.db.lock();
    let snippet = get_snippet(&conn, id).ok_or_else(|| "短语不存在".to_string())?;
    let content = snippet.content;
    drop(conn);
    use_clipboard_text(&state, &app, &content)
}
