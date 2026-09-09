use crate::builtin_plugins::todo::commands::helpers::*;
use crate::builtin_plugins::todo::commands::recurrence::apply_recurrence_constraints;
use crate::commands::markdown::{
    backup_markdown_image_file_name, cleanup_unreferenced_markdown_images,
    markdown_image_url_for_path, markdown_images_dir, read_backup_entries,
    restore_backup_markdown_image_urls, rewrite_markdown_images_for_backup,
    unique_markdown_image_path, write_zip_archive, ZipEntryInput,
};
use crate::commands::TodoBackupFile;
use crate::db::{AppState, TodoItem};
use chrono::Local;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

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
