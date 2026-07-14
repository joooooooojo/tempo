use chrono::{Duration, Local};
use rusqlite::{params, params_from_iter, types::Value, Connection, Error as SqliteError};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

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
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub shortcut: Option<String>,
    pub pinned: bool,
    pub use_count: i64,
    pub last_used_at: Option<String>,
    pub archived_at: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetGroup {
    pub id: i64,
    pub name: String,
    pub color: String,
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
        let mut writer = match encoder.write_header() {
            Ok(writer) => writer,
            Err(error) => {
                tracing::debug!(
                    width = width,
                    height = height,
                    error = %error,
                    "failed to prepare clipboard image png encoder"
                );
                return None;
            }
        };
        if let Err(error) = writer.write_image_data(rgba) {
            tracing::debug!(
                width = width,
                height = height,
                bytes = rgba.len(),
                error = %error,
                "failed to encode clipboard image png"
            );
            return None;
        }
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
        if let Err(error) = conn.execute(
            "UPDATE clipboard_history
             SET created_at = ?1,
                 source_app = COALESCE(?2, source_app),
                 source_process = COALESCE(?3, source_process)
             WHERE id = ?4",
            params![now_iso(), source_app, source_process, existing],
        ) {
            tracing::warn!(
                entry_id = existing,
                kind = %kind,
                error = %error,
                "failed to refresh existing clipboard entry"
            );
        }
        return get_clipboard_entry(conn, existing);
    }

    let created_at = now_iso();
    if let Err(error) = conn.execute(
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
    ) {
        tracing::warn!(
            kind = %kind,
            image_width = image_width,
            image_height = image_height,
            error = %error,
            "failed to insert clipboard entry"
        );
        return None;
    }

    let id = conn.last_insert_rowid();
    trim_clipboard_history(conn, max_entries);
    get_clipboard_entry(conn, id)
}

fn trim_clipboard_history(conn: &Connection, max_entries: u32) {
    if let Err(error) = conn.execute(
        "DELETE FROM clipboard_history
         WHERE pinned = 0
           AND id NOT IN (
             SELECT id FROM clipboard_history
             ORDER BY pinned DESC, created_at DESC, id DESC
             LIMIT ?1
           )",
        params![max_entries],
    ) {
        tracing::warn!(
            max_entries = max_entries,
            error = %error,
            "failed to trim clipboard history"
        );
    }
}

pub fn get_clipboard_entry(conn: &Connection, id: i64) -> Option<ClipboardEntry> {
    optional_db_row(
        conn.query_row(
            "SELECT id, content, kind, source_app, source_process, image_width, image_height, pinned, created_at
         FROM clipboard_history WHERE id = ?1",
            [id],
            map_clipboard_row,
        ),
        "load clipboard entry",
    )
}

pub fn touch_clipboard_entry(conn: &Connection, id: i64) -> bool {
    match conn.execute(
        "UPDATE clipboard_history SET created_at = ?1 WHERE id = ?2",
        params![now_iso(), id],
    ) {
        Ok(count) => count > 0,
        Err(error) => {
            tracing::warn!(
                entry_id = id,
                error = %error,
                "failed to touch clipboard entry"
            );
            false
        }
    }
}

pub fn count_clipboard_entries(conn: &Connection, query: Option<&str>) -> u32 {
    let like = query.map(|value| format!("%{value}%"));
    let result = if let Some(pattern) = like {
        conn.query_row(
            "SELECT COUNT(*) FROM clipboard_history WHERE kind = 'text' AND content LIKE ?1",
            [pattern],
            |row| row.get::<_, i64>(0),
        )
    } else {
        conn.query_row("SELECT COUNT(*) FROM clipboard_history", [], |row| {
            row.get::<_, i64>(0)
        })
    };

    match result {
        Ok(count) => count.max(0) as u32,
        Err(error) => {
            tracing::warn!(
                filtered = query.is_some(),
                error = %error,
                "failed to count clipboard entries"
            );
            0
        }
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

    let result = if let Some(pattern) = like {
        conn.prepare(&sql).and_then(|mut stmt| {
            stmt.query_map([pattern], map_clipboard_row)
                .and_then(|rows| rows.collect())
        })
    } else {
        conn.prepare(&sql).and_then(|mut stmt| {
            stmt.query_map([], map_clipboard_row)
                .and_then(|rows| rows.collect())
        })
    };

    match result {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(
                filtered = query.is_some(),
                limit = limit,
                offset = offset,
                error = %error,
                "failed to list clipboard entries"
            );
            Vec::new()
        }
    }
}

pub fn delete_clipboard_entry(conn: &Connection, id: i64) -> Result<Option<String>, ()> {
    let image_content = optional_db_row(
        conn.query_row(
            "SELECT content FROM clipboard_history WHERE id = ?1 AND kind = 'image'",
            [id],
            |row| row.get::<_, String>(0),
        ),
        "load clipboard image content for delete",
    );
    match conn.execute("DELETE FROM clipboard_history WHERE id = ?1", [id]) {
        Ok(count) if count > 0 => Ok(image_content),
        Ok(_) => Err(()),
        Err(error) => {
            tracing::warn!(
                entry_id = id,
                error = %error,
                "failed to delete clipboard entry"
            );
            Err(())
        }
    }
}

pub fn clear_clipboard_history(conn: &Connection) -> u32 {
    match conn.execute("DELETE FROM clipboard_history WHERE pinned = 0", []) {
        Ok(count) => count as u32,
        Err(error) => {
            tracing::warn!(error = %error, "failed to clear clipboard history");
            0
        }
    }
}

pub fn purge_clipboard_history_by_retention(conn: &Connection, retention: &str) -> u32 {
    let cutoff = match retention {
        "days" => Local::now() - Duration::days(1),
        "weeks" => Local::now() - Duration::days(7),
        "months" => Local::now() - Duration::days(30),
        "years" => Local::now() - Duration::days(365),
        _ => return 0,
    };
    match conn.execute(
        "DELETE FROM clipboard_history
         WHERE pinned = 0 AND created_at < ?1",
        [cutoff.to_rfc3339()],
    ) {
        Ok(count) => count as u32,
        Err(error) => {
            tracing::warn!(
                retention = %retention,
                error = %error,
                "failed to purge clipboard history by retention"
            );
            0
        }
    }
}

pub fn set_clipboard_entry_pinned(
    conn: &Connection,
    id: i64,
    pinned: bool,
) -> Option<ClipboardEntry> {
    if let Err(error) = conn.execute(
        "UPDATE clipboard_history SET pinned = ?1 WHERE id = ?2",
        params![pinned as i32, id],
    ) {
        tracing::warn!(
            entry_id = id,
            pinned = pinned,
            error = %error,
            "failed to set clipboard entry pinned state"
        );
        return None;
    }
    get_clipboard_entry(conn, id)
}

const SNIPPET_SELECT: &str = "
    SELECT
        s.id,
        s.title,
        s.content,
        s.tags,
        s.group_id,
        g.name,
        s.shortcut,
        s.pinned,
        s.use_count,
        s.last_used_at,
        s.archived_at,
        s.sort_order,
        s.created_at,
        s.updated_at
    FROM snippets s
    LEFT JOIN snippet_groups g ON g.id = s.group_id
";

pub fn list_snippet_groups(conn: &Connection) -> Vec<SnippetGroup> {
    match conn.prepare(
        "SELECT id, name, color, sort_order, created_at, updated_at
         FROM snippet_groups
         ORDER BY sort_order ASC, name COLLATE NOCASE ASC, id ASC",
    ) {
        Ok(mut stmt) => match stmt
            .query_map([], map_snippet_group_row)
            .and_then(|rows| rows.collect())
        {
            Ok(groups) => groups,
            Err(error) => {
                tracing::warn!(error = %error, "failed to list snippet groups");
                Vec::new()
            }
        },
        Err(error) => {
            tracing::warn!(error = %error, "failed to prepare snippet groups query");
            Vec::new()
        }
    }
}

pub fn add_snippet_group(
    conn: &Connection,
    name: &str,
    color: Option<&str>,
) -> Result<SnippetGroup, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("分组名称不能为空".into());
    }
    let color = color
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("default");
    let now = now_iso();
    let sort_order = next_group_sort_order(conn);
    conn.execute(
        "INSERT INTO snippet_groups (name, color, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![name, color, sort_order, now],
    )
    .map_err(map_snippet_db_error)?;
    get_snippet_group(conn, conn.last_insert_rowid()).ok_or_else(|| "分组保存失败".to_string())
}

pub fn update_snippet_group(
    conn: &Connection,
    id: i64,
    name: &str,
    color: Option<&str>,
) -> Result<SnippetGroup, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("分组名称不能为空".into());
    }
    let color = color
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("default");
    let updated_at = now_iso();
    let changed = conn
        .execute(
            "UPDATE snippet_groups
             SET name = ?1, color = ?2, updated_at = ?3
             WHERE id = ?4",
            params![name, color, updated_at, id],
        )
        .map_err(map_snippet_db_error)?;
    if changed == 0 {
        return Err("分组不存在".into());
    }
    get_snippet_group(conn, id).ok_or_else(|| "分组不存在".to_string())
}

pub fn delete_snippet_group(conn: &Connection, id: i64) -> bool {
    if let Err(error) = conn.execute(
        "UPDATE snippets SET group_id = NULL WHERE group_id = ?1",
        [id],
    ) {
        tracing::warn!(
            group_id = id,
            error = %error,
            "failed to unassign snippets before deleting group"
        );
    }
    match conn.execute("DELETE FROM snippet_groups WHERE id = ?1", [id]) {
        Ok(count) => count > 0,
        Err(error) => {
            tracing::warn!(
                group_id = id,
                error = %error,
                "failed to delete snippet group"
            );
            false
        }
    }
}

pub fn list_snippets(
    conn: &Connection,
    query: Option<&str>,
    group_id: Option<i64>,
    sort: Option<&str>,
) -> Vec<Snippet> {
    let mut sql = String::from(SNIPPET_SELECT);
    sql.push_str(" WHERE s.archived_at IS NULL");

    let mut values = Vec::<Value>::new();
    if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        let pattern = format!("%{query}%");
        sql.push_str(
            " AND (
                s.title LIKE ?
                OR s.content LIKE ?
                OR s.tags LIKE ?
                OR s.shortcut LIKE ?
                OR g.name LIKE ?
             )",
        );
        for _ in 0..5 {
            values.push(Value::Text(pattern.clone()));
        }
    }

    if let Some(group_id) = group_id {
        if group_id <= 0 {
            sql.push_str(" AND s.group_id IS NULL");
        } else {
            sql.push_str(" AND s.group_id = ?");
            values.push(Value::Integer(group_id));
        }
    }

    sql.push_str(match sort.unwrap_or("smart") {
        "title" => " ORDER BY s.pinned DESC, s.title COLLATE NOCASE ASC, s.id DESC",
        "used" => {
            " ORDER BY s.pinned DESC, s.use_count DESC, s.last_used_at DESC, s.updated_at DESC, s.id DESC"
        }
        "updated" => " ORDER BY s.pinned DESC, s.updated_at DESC, s.id DESC",
        _ => {
            " ORDER BY s.pinned DESC, s.sort_order ASC, s.last_used_at DESC, s.updated_at DESC, s.id DESC"
        }
    });

    match conn.prepare(&sql).and_then(|mut stmt| {
        stmt.query_map(params_from_iter(values.iter()), map_snippet_row)
            .and_then(|rows| rows.collect())
    }) {
        Ok(snippets) => snippets,
        Err(error) => {
            tracing::warn!(
                filtered = query.is_some(),
                group_id = group_id,
                sort = %sort.unwrap_or("smart"),
                error = %error,
                "failed to list snippets"
            );
            Vec::new()
        }
    }
}

pub fn get_snippet(conn: &Connection, id: i64) -> Option<Snippet> {
    let sql = format!("{SNIPPET_SELECT} WHERE s.id = ?1 AND s.archived_at IS NULL");
    optional_db_row(conn.query_row(&sql, [id], map_snippet_row), "load snippet")
}

pub fn add_snippet(
    conn: &Connection,
    title: &str,
    content: &str,
    tags: &[String],
    group_id: Option<i64>,
    shortcut: Option<&str>,
) -> Result<Snippet, String> {
    let title = title.trim();
    let content = content.trim();
    if title.is_empty() || content.is_empty() {
        return Err("标题和内容不能为空".into());
    }
    let now = now_iso();
    let tags_json = serde_json::to_string(&normalize_tags(tags)).unwrap_or_else(|_| "[]".into());
    let shortcut = normalize_shortcut(shortcut);
    let sort_order = next_snippet_sort_order(conn);
    conn.execute(
        "INSERT INTO snippets (
            title, content, tags, group_id, shortcut, pinned,
            use_count, sort_order, created_at, updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7, ?7)",
        params![title, content, tags_json, group_id, shortcut, sort_order, now],
    )
    .map_err(map_snippet_db_error)?;
    get_snippet(conn, conn.last_insert_rowid()).ok_or_else(|| "短语保存失败".to_string())
}

pub fn update_snippet(
    conn: &Connection,
    id: i64,
    title: &str,
    content: &str,
    tags: &[String],
    group_id: Option<i64>,
    shortcut: Option<&str>,
) -> Result<Snippet, String> {
    let title = title.trim();
    let content = content.trim();
    if title.is_empty() || content.is_empty() {
        return Err("标题和内容不能为空".into());
    }
    let tags_json = serde_json::to_string(&normalize_tags(tags)).unwrap_or_else(|_| "[]".into());
    let shortcut = normalize_shortcut(shortcut);
    let updated_at = now_iso();
    let changed = conn
        .execute(
            "UPDATE snippets
             SET title = ?1,
                 content = ?2,
                 tags = ?3,
                 group_id = ?4,
                 shortcut = ?5,
                 updated_at = ?6
             WHERE id = ?7 AND archived_at IS NULL",
            params![title, content, tags_json, group_id, shortcut, updated_at, id],
        )
        .map_err(map_snippet_db_error)?;
    if changed == 0 {
        return Err("短语不存在".into());
    }
    get_snippet(conn, id).ok_or_else(|| "短语不存在".to_string())
}

pub fn duplicate_snippet(conn: &Connection, id: i64) -> Result<Snippet, String> {
    let snippet = get_snippet(conn, id).ok_or_else(|| "短语不存在".to_string())?;
    add_snippet(
        conn,
        &format!("{} 副本", snippet.title),
        &snippet.content,
        &snippet.tags,
        snippet.group_id,
        None,
    )
}

pub fn set_snippet_pinned(conn: &Connection, id: i64, pinned: bool) -> Result<Snippet, String> {
    let updated_at = now_iso();
    let changed = conn
        .execute(
            "UPDATE snippets
             SET pinned = ?1, updated_at = ?2
             WHERE id = ?3 AND archived_at IS NULL",
            params![pinned as i32, updated_at, id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("短语不存在".into());
    }
    get_snippet(conn, id).ok_or_else(|| "短语不存在".to_string())
}

pub fn touch_snippet_usage(conn: &Connection, id: i64) -> Result<Snippet, String> {
    let last_used_at = now_iso();
    let changed = conn
        .execute(
            "UPDATE snippets
             SET use_count = use_count + 1,
                 last_used_at = ?1
             WHERE id = ?2 AND archived_at IS NULL",
            params![last_used_at, id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("短语不存在".into());
    }
    get_snippet(conn, id).ok_or_else(|| "短语不存在".to_string())
}

pub fn delete_snippet(conn: &Connection, id: i64) -> bool {
    match conn.execute("DELETE FROM snippets WHERE id = ?1", [id]) {
        Ok(count) => count > 0,
        Err(error) => {
            tracing::warn!(
                snippet_id = id,
                error = %error,
                "failed to delete snippet"
            );
            false
        }
    }
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
    let id = row.get(0)?;
    let tags_raw: String = row.get(3)?;
    let tags = match serde_json::from_str(&tags_raw) {
        Ok(tags) => tags,
        Err(error) => {
            tracing::debug!(
                snippet_id = id,
                error = %error,
                "failed to parse snippet tags"
            );
            Vec::new()
        }
    };
    Ok(Snippet {
        id,
        title: row.get(1)?,
        content: row.get(2)?,
        tags,
        group_id: row.get(4)?,
        group_name: row.get(5)?,
        shortcut: row.get(6)?,
        pinned: row.get::<_, i32>(7)? != 0,
        use_count: row.get(8)?,
        last_used_at: row.get(9)?,
        archived_at: row.get(10)?,
        sort_order: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn map_snippet_group_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SnippetGroup> {
    Ok(SnippetGroup {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn get_snippet_group(conn: &Connection, id: i64) -> Option<SnippetGroup> {
    optional_db_row(
        conn.query_row(
            "SELECT id, name, color, sort_order, created_at, updated_at
         FROM snippet_groups WHERE id = ?1",
            [id],
            map_snippet_group_row,
        ),
        "load snippet group",
    )
}

fn next_snippet_sort_order(conn: &Connection) -> i64 {
    match conn.query_row(
        "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM snippets",
        [],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(sort_order) => sort_order,
        Err(error) => {
            tracing::warn!(error = %error, "failed to load next snippet sort order");
            1
        }
    }
}

fn next_group_sort_order(conn: &Connection) -> i64 {
    match conn.query_row(
        "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM snippet_groups",
        [],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(sort_order) => sort_order,
        Err(error) => {
            tracing::warn!(error = %error, "failed to load next snippet group sort order");
            1
        }
    }
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() || normalized.iter().any(|item| item == tag) {
            continue;
        }
        normalized.push(tag.to_string());
    }
    normalized
}

fn normalize_shortcut(shortcut: Option<&str>) -> Option<String> {
    shortcut
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn map_snippet_db_error(error: rusqlite::Error) -> String {
    let message = error.to_string();
    if message.contains("snippet_groups.name") {
        "分组名称已存在".into()
    } else if message.contains("idx_snippets_shortcut")
        || message.contains("snippets.shortcut")
        || message.contains("UNIQUE constraint failed")
    {
        "快捷词已存在".into()
    } else {
        message
    }
}

fn now_iso() -> String {
    Local::now().to_rfc3339()
}

fn optional_db_row<T>(result: rusqlite::Result<T>, operation: &'static str) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(SqliteError::QueryReturnedNoRows) => None,
        Err(error) => {
            tracing::warn!(
                operation = %operation,
                error = %error,
                "database lookup failed"
            );
            None
        }
    }
}
