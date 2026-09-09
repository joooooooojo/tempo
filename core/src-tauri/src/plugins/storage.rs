//! Plugin private key-value storage (design §7 `storage.plugin.*`).
//!
//! This is the primary persistence path for pure-UI plugins (no Node `fs`); plugins with a
//! `main` may also read/write their own data directory directly.

use rusqlite::{params, Connection, OptionalExtension};

/// Per-value cap (design §7).
pub const MAX_VALUE_BYTES: usize = 256 * 1024;
/// Per-plugin total cap across all keys (design §7).
pub const MAX_TOTAL_BYTES: usize = 5 * 1024 * 1024;

pub fn get(
    conn: &Connection,
    plugin_id: &str,
    key: &str,
) -> Result<Option<serde_json::Value>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
            params![plugin_id, key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("read plugin storage: {e}"))?;
    match raw {
        Some(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| format!("parse plugin storage value: {e}")),
        None => Ok(None),
    }
}

/// Returns a user-facing quota error (surfaced to the plugin as `RESOURCE_EXHAUSTED`, not
/// `INTERNAL` — this is an expected, actionable condition, not an infrastructure fault).
pub fn set(
    conn: &Connection,
    plugin_id: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    if key.is_empty() || key.len() > 256 {
        return Err("storage key must be 1-256 characters".into());
    }
    let serialized = serde_json::to_string(value).map_err(|e| format!("serialize value: {e}"))?;
    if serialized.len() > MAX_VALUE_BYTES {
        return Err(format!(
            "value exceeds the {MAX_VALUE_BYTES} byte per-key limit"
        ));
    }
    let current_total: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(LENGTH(value)), 0) FROM plugin_storage
             WHERE plugin_id = ?1 AND key <> ?2",
            params![plugin_id, key],
            |row| row.get(0),
        )
        .map_err(|e| format!("sum plugin storage: {e}"))?;
    if current_total as usize + serialized.len() > MAX_TOTAL_BYTES {
        return Err(format!(
            "plugin storage quota ({MAX_TOTAL_BYTES} bytes total) exceeded"
        ));
    }

    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "INSERT INTO plugin_storage (plugin_id, key, value, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(plugin_id, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![plugin_id, key, serialized, now],
    )
    .map_err(|e| format!("write plugin storage: {e}"))?;
    Ok(())
}

pub fn delete(conn: &Connection, plugin_id: &str, key: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM plugin_storage WHERE plugin_id = ?1 AND key = ?2",
        params![plugin_id, key],
    )
    .map_err(|e| format!("delete plugin storage: {e}"))?;
    Ok(())
}

pub fn list(conn: &Connection, plugin_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT key FROM plugin_storage WHERE plugin_id = ?1 ORDER BY key")
        .map_err(|e| format!("prepare plugin storage list: {e}"))?;
    let rows = stmt
        .query_map(params![plugin_id], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query plugin storage list: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("plugin storage row: {e}"))?);
    }
    Ok(out)
}

/// Wipe everything for a plugin (uninstall with `deleteData: true`).
pub fn delete_all(conn: &Connection, plugin_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM plugin_storage WHERE plugin_id = ?1",
        params![plugin_id],
    )
    .map_err(|e| format!("delete all plugin storage: {e}"))?;
    Ok(())
}
