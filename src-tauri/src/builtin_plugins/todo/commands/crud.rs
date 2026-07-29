use crate::builtin_plugins::todo::commands::helpers::*;
use crate::builtin_plugins::todo::commands::recurrence::{
    apply_recurrence_constraints, next_recurrence_midnight, process_pending_recurrences,
};
use crate::commands::markdown::cleanup_unreferenced_markdown_images;
use crate::commands::TodoImageInput;
use crate::db::{AppState, TodoItem};
use chrono::Local;
use rusqlite::{params, OptionalExtension};
use tauri::AppHandle;

#[tauri::command]
pub fn get_todos(app: AppHandle, state: tauri::State<AppState>) -> Result<Vec<TodoItem>, String> {
    let spawned = {
        let conn = state.db.lock();
        process_pending_recurrences(&conn)?
    };
    for todo in spawned {
        emit_todo_created(&app, &todo);
    }

    let conn = state.db.lock();
    list_todos_light(&conn)
}

#[tauri::command]
pub fn get_todo(state: tauri::State<AppState>, id: i64) -> Result<TodoItem, String> {
    let conn = state.db.lock();
    fetch_todo(&conn, id)
}

#[tauri::command]
pub fn add_todo(
    app: AppHandle,
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
    tags: Option<Vec<String>>,
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
    let tag_names = normalize_todo_tags(tags)?;
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
    insert_todo_images(&app, &conn, id, &images)?;
    insert_subtasks(&conn, id, &subtask_titles)?;
    insert_todo_tags(&conn, id, &tag_names)?;
    let todo = fetch_todo(&conn, id)?;
    emit_todo_created(&app, &todo);
    Ok(todo)
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
    tags: Option<Vec<String>>,
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

    if tags.is_some() {
        replace_todo_tags(&conn, id, &normalize_todo_tags(tags)?)?;
    }

    let todo = fetch_todo(&conn, id)?;
    cleanup_unreferenced_markdown_images(&app, &conn);
    emit_todo_updated(&app, &todo);
    Ok(todo)
}

#[tauri::command]
pub fn set_todo_completed(
    app: AppHandle,
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
        let todo_updated = conn.changes();
        conn.execute(
            "UPDATE todo_subtasks SET completed = 1 WHERE todo_id = ?1",
            [id],
        )
        .map_err(|e| e.to_string())?;

        if todo_updated == 0 {
            return Err("待办不存在".into());
        }
    } else {
        let snapshot: Option<String> = conn
            .query_row(
                "SELECT subtasks_completion_snapshot FROM todos WHERE id = ?1",
                [id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .flatten();

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

        if conn.changes() == 0 {
            return Err("待办不存在".into());
        }

        if let Some(snapshot) = snapshot {
            restore_subtask_completion_snapshot(&conn, id, &snapshot)?;
        }
    }

    let todo = fetch_todo(&conn, id)?;
    emit_todo_updated(&app, &todo);
    Ok(todo)
}

#[tauri::command]
pub fn set_todo_pinned(
    app: AppHandle,
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

    let todo = fetch_todo(&conn, id)?;
    emit_todo_updated(&app, &todo);
    Ok(todo)
}

#[tauri::command]
pub fn add_todo_subtask(
    app: AppHandle,
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

    let todo = fetch_todo(&conn, todo_id)?;
    emit_todo_updated(&app, &todo);
    Ok(todo)
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
pub fn delete_todo(app: AppHandle, state: tauri::State<AppState>, id: i64) -> Result<(), String> {
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
    conn.execute("DELETE FROM todo_tags WHERE todo_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM todos WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;

    if conn.changes() == 0 {
        return Err("待办不存在".into());
    }

    drop(conn);
    emit_todo_deleted(&app, id);
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

    insert_todo_tags(&conn, todo.id, &todo.tags)?;

    fetch_todo(&conn, todo.id)
}

