use crate::builtin_plugins::todo::images::{
    hydrate_todo_images as hydrate_todo_image_urls,
    hydrate_todo_note_images as hydrate_todo_note_image_urls, save_todo_image_input,
};
use crate::commands::tracker::emit_on_main;
use crate::commands::{
    TodoImageInput, MAX_TODO_IMAGES, MAX_TODO_IMAGE_BYTES, MAX_TODO_NOTE_CHARS, MAX_TODO_NOTE_IMAGES,
};
use crate::db::{TodoImage, TodoItem, TodoNote, TodoNoteImage, TodoSubtask};
use base64::Engine as _;
use chrono::{DateTime, Local};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use tauri::AppHandle;

pub(super) fn emit_todo_created(app: &AppHandle, todo: &TodoItem) {
    emit_on_main(
        app,
        "todo-created",
        serde_json::to_value(todo).unwrap_or_else(|_| json!({})),
    );
}

pub(super) fn emit_todo_updated(app: &AppHandle, todo: &TodoItem) {
    emit_on_main(
        app,
        "todo-updated",
        serde_json::to_value(todo).unwrap_or_else(|_| json!({})),
    );
}

pub(super) fn emit_todo_deleted(app: &AppHandle, id: i64) {
    emit_on_main(app, "todo-deleted", json!({ "id": id }));
}
pub(super) fn normalize_todo_title(title: String, allow_image_only: bool) -> Result<String, String> {
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

pub(super) fn normalize_todo_content(content: String) -> String {
    content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

pub(super) fn normalize_due_at(due_at: Option<String>) -> Result<Option<String>, String> {
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

pub(super) fn normalize_todo_images(
    images: Option<Vec<TodoImageInput>>,
) -> Result<Vec<TodoImageInput>, String> {
    let images = images.unwrap_or_default();

    if images.len() > MAX_TODO_IMAGES {
        return Err(format!("每个待办最多添加 {} 张图片", MAX_TODO_IMAGES));
    }

    validate_todo_image_inputs(&images)?;
    Ok(images)
}

pub(super) fn normalize_todo_note_body(body: String, allow_image_only: bool) -> Result<String, String> {
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

pub(super) fn normalize_todo_note_images(
    images: Option<Vec<TodoImageInput>>,
) -> Result<Vec<TodoImageInput>, String> {
    let images = images.unwrap_or_default();

    if images.len() > MAX_TODO_NOTE_IMAGES {
        return Err(format!("每条备注最多添加 {} 张图片", MAX_TODO_NOTE_IMAGES));
    }

    validate_todo_image_inputs(&images)?;
    Ok(images)
}

pub(super) fn validate_todo_image_inputs(images: &[TodoImageInput]) -> Result<(), String> {
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

pub(super) fn fetch_todo(conn: &Connection, id: i64) -> Result<TodoItem, String> {
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

pub(super) fn list_todos(conn: &Connection) -> Result<Vec<TodoItem>, String> {
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

pub(super) fn list_todos_light(conn: &Connection) -> Result<Vec<TodoItem>, String> {
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

pub(super) fn query_todo_rows(conn: &Connection) -> Result<Vec<TodoItem>, String> {
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

pub(super) fn todo_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodoItem> {
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

pub(super) fn insert_todo_images(
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

pub(super) fn hydrate_todo_image_counts(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
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

pub(super) fn load_todo_images_from_db(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
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

pub(super) fn insert_todo_note_images(
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

pub(super) fn load_todo_notes_from_db(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
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

pub(super) fn load_todo_note_images_from_db(conn: &Connection, notes: &mut [TodoNote]) -> Result<(), String> {
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

pub(super) fn hydrate_todo_subtasks(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
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

pub(super) fn insert_subtasks(conn: &Connection, todo_id: i64, titles: &[String]) -> Result<(), String> {
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

pub(super) fn encode_subtask_completion_snapshot(subtasks: &[TodoSubtask]) -> String {
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

pub(super) fn restore_subtask_completion_snapshot(
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

pub(super) fn normalize_subtask_titles(titles: Option<Vec<String>>) -> Result<Vec<String>, String> {
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

pub(super) fn normalize_subtask_title(title: String) -> Result<String, String> {
    let normalized = title.trim().to_string();
    if normalized.is_empty() {
        return Err("子任务标题不能为空".into());
    }
    if normalized.chars().count() > 120 {
        return Err("子任务标题不能超过 120 个字".into());
    }
    Ok(normalized)
}

pub(super) fn normalize_todo_tag(name: String) -> Result<String, String> {
    let normalized = name.trim().to_string();
    if normalized.is_empty() {
        return Err("标签不能为空".into());
    }
    if normalized.chars().count() > 32 {
        return Err("标签不能超过 32 个字".into());
    }
    Ok(normalized)
}

pub(super) fn normalize_todo_tags(tags: Option<Vec<String>>) -> Result<Vec<String>, String> {
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

pub(super) fn insert_todo_tags(conn: &Connection, todo_id: i64, tags: &[String]) -> Result<(), String> {
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

pub(super) fn replace_todo_tags(conn: &Connection, todo_id: i64, tags: &[String]) -> Result<(), String> {
    conn.execute("DELETE FROM todo_tags WHERE todo_id = ?1", [todo_id])
        .map_err(|e| e.to_string())?;
    insert_todo_tags(conn, todo_id, tags)
}

pub(super) fn hydrate_todo_tags(conn: &Connection, todos: &mut [TodoItem]) -> Result<(), String> {
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

