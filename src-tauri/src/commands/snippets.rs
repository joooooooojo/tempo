use crate::clipboard_db::{
    add_snippet, add_snippet_group, delete_snippet, delete_snippet_group, duplicate_snippet,
    get_snippet, list_snippet_groups, list_snippets, set_snippet_pinned, touch_snippet_usage,
    update_snippet, update_snippet_group, Snippet, SnippetGroup,
};
use crate::clipboard_watcher::use_clipboard_text;
use crate::db::AppState;
use tauri::Emitter;

#[tauri::command]
pub fn get_snippets(
    state: tauri::State<AppState>,
    query: Option<String>,
    group_id: Option<i64>,
    sort: Option<String>,
) -> Vec<Snippet> {
    let conn = state.db.lock();
    list_snippets(&conn, query.as_deref(), group_id, sort.as_deref())
}

#[tauri::command]
pub fn get_snippet_groups(state: tauri::State<AppState>) -> Vec<SnippetGroup> {
    let conn = state.db.lock();
    list_snippet_groups(&conn)
}

#[tauri::command]
pub fn create_snippet_group(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    name: String,
    color: Option<String>,
) -> Result<SnippetGroup, String> {
    let conn = state.db.lock();
    let group = add_snippet_group(&conn, &name, color.as_deref())?;
    emit_snippets_update(&app);
    Ok(group)
}

#[tauri::command]
pub fn update_snippet_group_command(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
    name: String,
    color: Option<String>,
) -> Result<SnippetGroup, String> {
    let conn = state.db.lock();
    let group = update_snippet_group(&conn, id, &name, color.as_deref())?;
    emit_snippets_update(&app);
    Ok(group)
}

#[tauri::command]
pub fn delete_snippet_group_command(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.db.lock();
    if delete_snippet_group(&conn, id) {
        emit_snippets_update(&app);
        Ok(())
    } else {
        Err("分组不存在".into())
    }
}

#[tauri::command]
pub fn create_snippet(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    title: String,
    content: String,
    tags: Vec<String>,
    group_id: Option<i64>,
    shortcut: Option<String>,
) -> Result<Snippet, String> {
    let conn = state.db.lock();
    let snippet = add_snippet(
        &conn,
        &title,
        &content,
        &tags,
        normalize_group_id(group_id),
        shortcut.as_deref(),
    )?;
    emit_snippets_update(&app);
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
    group_id: Option<i64>,
    shortcut: Option<String>,
) -> Result<Snippet, String> {
    let conn = state.db.lock();
    let snippet = update_snippet(
        &conn,
        id,
        &title,
        &content,
        &tags,
        normalize_group_id(group_id),
        shortcut.as_deref(),
    )?;
    emit_snippets_update(&app);
    Ok(snippet)
}

#[tauri::command]
pub fn duplicate_snippet_command(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
) -> Result<Snippet, String> {
    let conn = state.db.lock();
    let snippet = duplicate_snippet(&conn, id)?;
    emit_snippets_update(&app);
    Ok(snippet)
}

#[tauri::command]
pub fn pin_snippet_command(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
    pinned: bool,
) -> Result<Snippet, String> {
    let conn = state.db.lock();
    let snippet = set_snippet_pinned(&conn, id, pinned)?;
    emit_snippets_update(&app);
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
        emit_snippets_update(&app);
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
) -> Result<Snippet, String> {
    let conn = state.db.lock();
    let snippet = get_snippet(&conn, id).ok_or_else(|| "短语不存在".to_string())?;
    let content = snippet.content;
    drop(conn);

    use_clipboard_text(&state, &app, &content)?;

    let conn = state.db.lock();
    let snippet = touch_snippet_usage(&conn, id)?;
    emit_snippets_update(&app);
    Ok(snippet)
}

fn emit_snippets_update(app: &tauri::AppHandle) {
    crate::logging::debug_if_err(app.emit("snippets-update", ()), "emit snippets update");
}

fn normalize_group_id(group_id: Option<i64>) -> Option<i64> {
    group_id.filter(|value| *value > 0)
}
