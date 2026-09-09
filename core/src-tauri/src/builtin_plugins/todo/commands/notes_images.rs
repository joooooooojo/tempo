use crate::builtin_plugins::todo::commands::helpers::*;
use crate::commands::TodoImageInput;
use crate::db::{AppState, TodoItem, TodoNote};
use chrono::Local;
use rusqlite::{params, OptionalExtension};
use tauri::AppHandle;

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
    app: AppHandle,
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
    insert_todo_note_images(&app, &conn, note_id, &images)?;
    let todo = fetch_todo(&conn, todo_id)?;
    emit_todo_updated(&app, &todo);
    Ok(todo)
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
