use crate::builtin_plugins::todo::commands::helpers::*;
use crate::db::{AppState, TodoItem};
use chrono::{DateTime, Duration as ChronoDuration, Local};
use rusqlite::{params, Connection};
use tauri::AppHandle;

pub fn check_pending_recurrences(app: &AppHandle, state: &AppState) {
    let spawned = match {
        let conn = state.db.lock();
        process_pending_recurrences(&conn)
    } {
        Ok(items) => items,
        Err(error) => {
            tracing::warn!(error = %error, "failed to process pending todo recurrences");
            return;
        }
    };

    for todo in spawned {
        emit_todo_created(app, &todo);
    }
}


pub(super) fn spawn_recurring_todo(
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

pub(super) fn process_pending_recurrences(conn: &Connection) -> Result<Vec<TodoItem>, String> {
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

pub(super) fn list_due_recurrence_spawns(
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

pub(super) fn has_active_recurrence_instance(conn: &Connection, root_id: i64) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM todos WHERE recurrence_root_id = ?1 AND completed = 0",
            [root_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

pub(super) fn next_recurrence_midnight(from: DateTime<Local>, recurrence: &str) -> Option<String> {
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

pub(super) fn apply_recurrence_constraints(
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

pub(super) fn normalize_recurrence(recurrence: String) -> Result<String, String> {
    let normalized = recurrence.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "none" => Ok("none".into()),
        "daily" | "weekly" | "monthly" => Ok(normalized),
        _ => Err("重复规则无效".into()),
    }
}

pub(super) fn normalize_remind_custom_hours(
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

pub fn mark_due_reminder_sent(conn: &Connection, id: i64, flag: &str) -> Result<(), String> {
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
