use crate::db::{TodoImage, TodoItem, TodoNote, TodoNoteImage, TodoSubtask, AppState};
use crate::todo_images::{
    backup_todo_image_file_name, hydrate_todo_images as hydrate_todo_image_urls,
    hydrate_todo_note_images as hydrate_todo_note_image_urls, maybe_delete_todo_image_file,
    normalize_todo_image_reference, save_todo_image_input, todo_images_dir, TODO_IMAGE_SUBDIR,
};
use base64::Engine as _;
use chrono::{DateTime, Duration as ChronoDuration, Local};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use super::markdown::{
    backup_markdown_image_file_name, cleanup_unreferenced_markdown_images,
    markdown_images_dir, markdown_image_url_for_path, read_backup_entries,
    restore_backup_markdown_image_urls, rewrite_markdown_images_for_backup, unique_markdown_image_path,
    write_zip_archive, ZipEntryInput,
};
use super::tracker::emit_on_main;
use super::{TodoBackupFile, TodoImageInput, MAX_TODO_IMAGE_BYTES, MAX_TODO_IMAGES, MAX_TODO_NOTE_CHARS, MAX_TODO_NOTE_IMAGES};

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
    list_todos_light(&conn)
}

#[tauri::command]
pub fn get_todo(state: tauri::State<AppState>, id: i64) -> Result<TodoItem, String> {
    let conn = state.db.lock();
    fetch_todo(&conn, id)
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
    conn.execute("DELETE FROM todo_tags WHERE todo_id = ?1", [id])
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

    insert_todo_tags(&conn, todo.id, &todo.tags)?;

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
pub(crate) fn insert_imported_todos(
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

        insert_todo_tags(&conn, todo_id, &todo.tags)?;
    }

    Ok(())
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

    load_todo_images_from_db(conn, std::slice::from_mut(&mut todo))?;
    load_todo_notes_from_db(conn, std::slice::from_mut(&mut todo))?;
    hydrate_todo_subtasks(conn, std::slice::from_mut(&mut todo))?;
    hydrate_todo_tags(conn, std::slice::from_mut(&mut todo))?;
    hydrate_todo_image_urls(&mut todo.images);
    for note in &mut todo.notes {
        hydrate_todo_note_image_urls(&mut note.images);
    }
    todo.image_count = todo.images.len() as u32;
    todo.lightweight = false;
    Ok(todo)
}

fn list_todos(conn: &Connection) -> Result<Vec<TodoItem>, String> {
    let mut todos = query_todo_rows(conn)?;
    load_todo_images_from_db(conn, &mut todos)?;
    load_todo_notes_from_db(conn, &mut todos)?;
    hydrate_todo_subtasks(conn, &mut todos)?;
    hydrate_todo_tags(conn, &mut todos)?;
    for todo in &mut todos {
        hydrate_todo_image_urls(&mut todo.images);
        for note in &mut todo.notes {
            hydrate_todo_note_image_urls(&mut note.images);
        }
        todo.image_count = todo.images.len() as u32;
        todo.lightweight = false;
    }
    Ok(todos)
}

fn list_todos_light(conn: &Connection) -> Result<Vec<TodoItem>, String> {
    let mut todos = query_todo_rows(conn)?;
    hydrate_todo_image_counts(conn, &mut todos)?;
    load_todo_notes_from_db(conn, &mut todos)?;
    hydrate_todo_subtasks(conn, &mut todos)?;
    hydrate_todo_tags(conn, &mut todos)?;
    for todo in &mut todos {
        todo.lightweight = true;
    }
    Ok(todos)
}

fn query_todo_rows(conn: &Connection) -> Result<Vec<TodoItem>, String> {
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
        .map_err(|e| e.to_string())
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
        tags: Vec::new(),
        image_count: 0,
        lightweight: false,
    })
}

fn insert_todo_images(
    app: &AppHandle,
    conn: &Connection,
    todo_id: i64,
    images: &[TodoImageInput],
) -> Result<(), String> {
    for image in images {
        let storage_key = save_todo_image_input(app, image)?;
        conn.execute(
            "INSERT INTO todo_images (todo_id, data_url, mime_type, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                todo_id,
                storage_key,
                image.mime_type.trim().to_ascii_lowercase(),
                Local::now().to_rfc3339()
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn hydrate_todo_image_counts(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
    for todo in todos {
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM todo_images WHERE todo_id = ?1",
                [todo.id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        todo.image_count = count;
        todo.images.clear();
    }
    Ok(())
}

fn load_todo_images_from_db(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
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
    app: &AppHandle,
    conn: &Connection,
    note_id: i64,
    images: &[TodoImageInput],
) -> Result<(), String> {
    for image in images {
        let storage_key = save_todo_image_input(app, image)?;
        conn.execute(
            "INSERT INTO todo_note_images (note_id, data_url, mime_type, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                note_id,
                storage_key,
                image.mime_type.trim().to_ascii_lowercase(),
                Local::now().to_rfc3339()
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn load_todo_notes_from_db(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
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

        load_todo_note_images_from_db(conn, &mut notes)?;
        todo.notes = notes;
    }

    Ok(())
}

fn load_todo_note_images_from_db(conn: &Connection, notes: &mut [TodoNote]) -> Result<(), String> {
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
    insert_todo_tags(conn, id, &source.tags)?;
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

fn normalize_todo_tag(name: String) -> Result<String, String> {
    let normalized = name.trim().to_string();
    if normalized.is_empty() {
        return Err("标签不能为空".into());
    }
    if normalized.chars().count() > 32 {
        return Err("标签不能超过 32 个字".into());
    }
    Ok(normalized)
}

fn normalize_todo_tags(tags: Option<Vec<String>>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();

    for tag in tags.unwrap_or_default() {
        let tag = normalize_todo_tag(tag)?;
        let key = tag.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        if normalized.len() >= 10 {
            return Err("每个待办最多添加 10 个标签".into());
        }
        seen.insert(key);
        normalized.push(tag);
    }

    Ok(normalized)
}

fn insert_todo_tags(conn: &Connection, todo_id: i64, tags: &[String]) -> Result<(), String> {
    let created_at = Local::now().to_rfc3339();
    for tag in tags {
        conn.execute(
            "INSERT INTO todo_tags (todo_id, name, created_at) VALUES (?1, ?2, ?3)",
            params![todo_id, tag, created_at],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn replace_todo_tags(conn: &Connection, todo_id: i64, tags: &[String]) -> Result<(), String> {
    conn.execute("DELETE FROM todo_tags WHERE todo_id = ?1", [todo_id])
        .map_err(|e| e.to_string())?;
    insert_todo_tags(conn, todo_id, tags)
}

fn hydrate_todo_tags(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
    for todo in todos {
        let mut stmt = conn
            .prepare(
                "SELECT name
                 FROM todo_tags
                 WHERE todo_id = ?1
                 ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([todo.id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;

        todo.tags = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub(crate) fn mark_due_reminder_sent(conn: &Connection, id: i64, flag: &str) -> Result<(), String> {
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
