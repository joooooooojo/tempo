use chrono::Local;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

pub const MAX_CLIPBOARD_CONTENT_CHARS: usize = 32 * 1024;
pub const MAX_CLIPBOARD_IMAGE_PIXELS: u64 = 4096 * 4096;
pub const MAX_CLIPBOARD_IMAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: i64,
    pub content: String,
    pub kind: String,
    pub source_app: Option<String>,
    pub source_process: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_icon_data_url: Option<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub pinned: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub fn hash_content(content: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(bytes)
}

pub fn insert_clipboard_text(
    conn: &Connection,
    content: &str,
    source_app: Option<&str>,
    source_process: Option<&str>,
    max_entries: u32,
) -> Option<ClipboardEntry> {
    if content.is_empty() {
        return None;
    }
    if content.chars().count() > MAX_CLIPBOARD_CONTENT_CHARS {
        return None;
    }

    upsert_clipboard_entry(
        conn,
        content,
        &hash_content(content),
        "text",
        source_app,
        source_process,
        None,
        None,
        max_entries,
    )
}

pub fn insert_clipboard_image(
    conn: &Connection,
    storage_key: &str,
    content_hash: &str,
    width: u32,
    height: u32,
    source_app: Option<&str>,
    source_process: Option<&str>,
    max_entries: u32,
) -> Option<ClipboardEntry> {
    if storage_key.is_empty() || width == 0 || height == 0 {
        return None;
    }

    upsert_clipboard_entry(
        conn,
        storage_key,
        content_hash,
        "image",
        source_app,
        source_process,
        Some(width),
        Some(height),
        max_entries,
    )
}

fn upsert_clipboard_entry(
    conn: &Connection,
    content: &str,
    content_hash: &str,
    kind: &str,
    source_app: Option<&str>,
    source_process: Option<&str>,
    image_width: Option<u32>,
    image_height: Option<u32>,
    max_entries: u32,
) -> Option<ClipboardEntry> {
    if let Ok(existing) = conn.query_row(
        "SELECT id FROM clipboard_history WHERE content_hash = ?1 ORDER BY id DESC LIMIT 1",
        [content_hash],
        |row| row.get::<_, i64>(0),
    ) {
        conn.execute(
            "UPDATE clipboard_history
             SET created_at = ?1,
                 source_app = COALESCE(?2, source_app),
                 source_process = COALESCE(?3, source_process)
             WHERE id = ?4",
            params![now_iso(), source_app, source_process, existing],
        )
        .ok();
        return get_clipboard_entry(conn, existing);
    }

    let created_at = now_iso();
    conn.execute(
        "INSERT INTO clipboard_history (
            content, content_hash, kind, source_app, source_process,
            image_width, image_height, pinned, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
        params![
            content,
            content_hash,
            kind,
            source_app,
            source_process,
            image_width,
            image_height,
            created_at
        ],
    )
    .ok()?;

    let id = conn.last_insert_rowid();
    trim_clipboard_history(conn, max_entries);
    get_clipboard_entry(conn, id)
}

fn trim_clipboard_history(conn: &Connection, max_entries: u32) {
    let _ = conn.execute(
        "DELETE FROM clipboard_history
         WHERE pinned = 0
           AND id NOT IN (
             SELECT id FROM clipboard_history
             ORDER BY pinned DESC, created_at DESC, id DESC
             LIMIT ?1
           )",
        params![max_entries],
    );
}

pub fn get_clipboard_entry(conn: &Connection, id: i64) -> Option<ClipboardEntry> {
    conn.query_row(
        "SELECT id, content, kind, source_app, source_process, image_width, image_height, pinned, created_at
         FROM clipboard_history WHERE id = ?1",
        [id],
        map_clipboard_row,
    )
    .ok()
}

pub fn get_clipboard_entry_content_hash(conn: &Connection, id: i64) -> Option<String> {
    conn.query_row(
        "SELECT content_hash FROM clipboard_history WHERE id = ?1",
        [id],
        |row| row.get(0),
    )
    .ok()
}

pub fn touch_clipboard_entry(conn: &Connection, id: i64) -> bool {
    conn.execute(
        "UPDATE clipboard_history SET created_at = ?1 WHERE id = ?2",
        params![now_iso(), id],
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

pub fn count_clipboard_entries(conn: &Connection, query: Option<&str>) -> u32 {
    let like = query.map(|value| format!("%{value}%"));
    if let Some(pattern) = like {
        conn.query_row(
            "SELECT COUNT(*) FROM clipboard_history WHERE kind = 'text' AND content LIKE ?1",
            [pattern],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count as u32)
        .unwrap_or(0)
    } else {
        conn.query_row("SELECT COUNT(*) FROM clipboard_history", [], |row| row.get::<_, i64>(0))
            .map(|count| count as u32)
            .unwrap_or(0)
    }
}

pub fn list_clipboard_entries(
    conn: &Connection,
    query: Option<&str>,
    limit: u32,
    offset: u32,
) -> Vec<ClipboardEntry> {
    let like = query.map(|value| format!("%{value}%"));
    let mut sql = String::from(
        "SELECT id, content, kind, source_app, source_process, image_width, image_height, pinned, created_at
         FROM clipboard_history",
    );
    if like.is_some() {
        sql.push_str(" WHERE kind = 'text' AND content LIKE ?1");
    }
    sql.push_str(" ORDER BY pinned DESC, created_at DESC, id DESC LIMIT ");
    sql.push_str(&limit.to_string());
    sql.push_str(" OFFSET ");
    sql.push_str(&offset.to_string());

    if let Some(pattern) = like {
        conn.prepare(&sql)
            .and_then(|mut stmt| {
                stmt.query_map([pattern], map_clipboard_row)
                    .and_then(|rows| rows.collect())
            })
            .unwrap_or_default()
    } else {
        conn.prepare(&sql)
            .and_then(|mut stmt| stmt.query_map([], map_clipboard_row).and_then(|rows| rows.collect()))
            .unwrap_or_default()
    }
}

pub fn delete_clipboard_entry(conn: &Connection, id: i64) -> Result<Option<String>, ()> {
    let image_content = conn
        .query_row(
            "SELECT content FROM clipboard_history WHERE id = ?1 AND kind = 'image'",
            [id],
            |row| row.get::<_, String>(0),
        )
        .ok();
    if conn
        .execute("DELETE FROM clipboard_history WHERE id = ?1", [id])
        .map(|count| count > 0)
        .unwrap_or(false)
    {
        Ok(image_content)
    } else {
        Err(())
    }
}

pub fn clear_clipboard_history(conn: &Connection) -> u32 {
    conn.execute("DELETE FROM clipboard_history WHERE pinned = 0", [])
        .unwrap_or(0) as u32
}

pub fn set_clipboard_entry_pinned(conn: &Connection, id: i64, pinned: bool) -> Option<ClipboardEntry> {
    conn.execute(
        "UPDATE clipboard_history SET pinned = ?1 WHERE id = ?2",
        params![pinned as i32, id],
    )
    .ok()?;
    get_clipboard_entry(conn, id)
}

pub fn list_snippets(conn: &Connection, query: Option<&str>) -> Vec<Snippet> {
    let like = query.map(|value| format!("%{value}%"));
    if let Some(pattern) = like {
        conn.prepare(
            "SELECT id, title, content, tags, sort_order, created_at, updated_at
             FROM snippets
             WHERE title LIKE ?1 OR content LIKE ?1 OR tags LIKE ?1
             ORDER BY sort_order ASC, updated_at DESC, id DESC",
        )
        .and_then(|mut stmt| {
            stmt.query_map([pattern], map_snippet_row)
                .and_then(|rows| rows.collect())
        })
        .unwrap_or_default()
    } else {
        conn.prepare(
            "SELECT id, title, content, tags, sort_order, created_at, updated_at
             FROM snippets
             ORDER BY sort_order ASC, updated_at DESC, id DESC",
        )
        .and_then(|mut stmt| stmt.query_map([], map_snippet_row).and_then(|rows| rows.collect()))
        .unwrap_or_default()
    }
}

pub fn get_snippet(conn: &Connection, id: i64) -> Option<Snippet> {
    conn.query_row(
        "SELECT id, title, content, tags, sort_order, created_at, updated_at
         FROM snippets WHERE id = ?1",
        [id],
        map_snippet_row,
    )
    .ok()
}

pub fn add_snippet(conn: &Connection, title: &str, content: &str, tags: &[String]) -> Option<Snippet> {
    let title = title.trim();
    let content = content.trim();
    if title.is_empty() || content.is_empty() {
        return None;
    }
    let now = now_iso();
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO snippets (title, content, tags, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, 0, ?4, ?4)",
        params![title, content, tags_json, now],
    )
    .ok()?;
    get_snippet(conn, conn.last_insert_rowid())
}

pub fn update_snippet(
    conn: &Connection,
    id: i64,
    title: &str,
    content: &str,
    tags: &[String],
) -> Option<Snippet> {
    let title = title.trim();
    let content = content.trim();
    if title.is_empty() || content.is_empty() {
        return None;
    }
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".into());
    let updated_at = now_iso();
    conn.execute(
        "UPDATE snippets SET title = ?1, content = ?2, tags = ?3, updated_at = ?4 WHERE id = ?5",
        params![title, content, tags_json, updated_at, id],
    )
    .ok()?;
    get_snippet(conn, id)
}

pub fn delete_snippet(conn: &Connection, id: i64) -> bool {
    conn.execute("DELETE FROM snippets WHERE id = ?1", [id])
        .map(|count| count > 0)
        .unwrap_or(false)
}

fn map_clipboard_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardEntry> {
    Ok(ClipboardEntry {
        id: row.get(0)?,
        content: row.get(1)?,
        kind: row.get(2)?,
        source_app: row.get(3)?,
        source_process: row.get(4)?,
        source_icon_data_url: None,
        image_width: row.get::<_, Option<i64>>(5)?.map(|value| value as u32),
        image_height: row.get::<_, Option<i64>>(6)?.map(|value| value as u32),
        pinned: row.get::<_, i32>(7)? != 0,
        created_at: row.get(8)?,
    })
}

fn map_snippet_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snippet> {
    let tags_raw: String = row.get(3)?;
    let tags = serde_json::from_str(&tags_raw).unwrap_or_default();
    Ok(Snippet {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        tags,
        sort_order: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn now_iso() -> String {
    Local::now().to_rfc3339()
}
