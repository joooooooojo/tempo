//! Package / publisher trust records (Phase 1: package hash confirmation).

use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageTrustRecord {
    pub plugin_id: String,
    pub version: String,
    pub package_hash: Option<String>,
    pub trusted: bool,
    pub install_source: String,
    pub signature_status: String,
}

pub fn ensure_plugin_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS plugins (
          id TEXT PRIMARY KEY,
          current_version TEXT NOT NULL,
          pending_version TEXT,
          enabled INTEGER NOT NULL DEFAULT 0,
          runtime_state TEXT NOT NULL DEFAULT 'disabled',
          installed_at TEXT NOT NULL,
          updated_at TEXT,
          last_error TEXT
        );

        CREATE TABLE IF NOT EXISTS plugin_versions (
          plugin_id TEXT NOT NULL,
          version TEXT NOT NULL,
          package_hash TEXT,
          dev_path TEXT,
          display_publisher TEXT,
          verified_publisher_key TEXT,
          install_source TEXT NOT NULL,
          signature_status TEXT NOT NULL,
          trusted_at TEXT,
          installed_at TEXT NOT NULL,
          PRIMARY KEY (plugin_id, version)
        );

        CREATE TABLE IF NOT EXISTS publisher_trust (
          signing_key_id TEXT PRIMARY KEY,
          publisher_id TEXT NOT NULL,
          trusted_at TEXT NOT NULL,
          revoked_at TEXT
        );

        CREATE TABLE IF NOT EXISTS plugin_sessions (
          plugin_id TEXT NOT NULL,
          app_id TEXT NOT NULL,
          plugin_version TEXT NOT NULL,
          session_version INTEGER NOT NULL,
          payload TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (plugin_id, app_id)
        );

        CREATE TABLE IF NOT EXISTS plugin_storage (
          plugin_id TEXT NOT NULL,
          key TEXT NOT NULL,
          value TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (plugin_id, key)
        );

        CREATE TABLE IF NOT EXISTS plugin_mcp_exposure (
          plugin_id TEXT PRIMARY KEY,
          exposed INTEGER NOT NULL DEFAULT 0,
          updated_at TEXT NOT NULL
        );
        ",
    )
    .map_err(|e| format!("create plugin tables: {e}"))?;
    Ok(())
}

/// User opt-in for exposing a plugin's `contributes.mcpTools` to MCP/AI callers (design §11,
/// Phase 2). Defaults to `false` for every plugin — nothing is exposed until this is set.
pub fn set_plugin_mcp_exposed(conn: &Connection, plugin_id: &str, exposed: bool) -> Result<(), String> {
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "INSERT INTO plugin_mcp_exposure (plugin_id, exposed, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(plugin_id) DO UPDATE SET exposed = excluded.exposed, updated_at = excluded.updated_at",
        params![plugin_id, exposed as i64, now],
    )
    .map_err(|e| format!("upsert plugin_mcp_exposure: {e}"))?;
    Ok(())
}

pub fn is_plugin_mcp_exposed(conn: &Connection, plugin_id: &str) -> bool {
    conn.query_row(
        "SELECT exposed FROM plugin_mcp_exposure WHERE plugin_id = ?1",
        params![plugin_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|v| v != 0)
    .unwrap_or(false)
}

/// Update the in-memory/persisted runtime state machine column (design §6.2):
/// `disabled | enabled | starting | active | draining | failed`.
pub fn set_runtime_state(conn: &Connection, plugin_id: &str, state: &str) -> Result<(), String> {
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "UPDATE plugins SET runtime_state = ?1, updated_at = ?2 WHERE id = ?3",
        params![state, now, plugin_id],
    )
    .map_err(|e| format!("update runtime_state: {e}"))?;
    Ok(())
}

/// Record (or clear with `None`) the last user-facing Runtime error for diagnostics.
pub fn set_last_error(
    conn: &Connection,
    plugin_id: &str,
    error: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "UPDATE plugins SET last_error = ?1 WHERE id = ?2",
        params![error, plugin_id],
    )
    .map_err(|e| format!("update last_error: {e}"))?;
    Ok(())
}

/// Tempo restarts must never assume old child processes still exist (design §6.2): any
/// `active|starting|draining` persisted state collapses back to `enabled` on boot.
pub fn normalize_runtime_states_on_boot(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "UPDATE plugins SET runtime_state = 'enabled'
         WHERE enabled = 1 AND runtime_state IN ('active', 'starting', 'draining')",
        [],
    )
    .map_err(|e| format!("normalize runtime states: {e}"))?;
    Ok(())
}

pub fn record_installed_version(
    conn: &Connection,
    plugin_id: &str,
    version: &str,
    package_hash: &str,
    display_publisher: Option<&str>,
    install_source: &str,
) -> Result<(), String> {
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "INSERT INTO plugin_versions (
            plugin_id, version, package_hash, display_publisher,
            install_source, signature_status, installed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'unsigned', ?6)
         ON CONFLICT(plugin_id, version) DO UPDATE SET
            package_hash = excluded.package_hash,
            display_publisher = excluded.display_publisher,
            install_source = excluded.install_source,
            installed_at = excluded.installed_at",
        params![
            plugin_id,
            version,
            package_hash,
            display_publisher,
            install_source,
            now
        ],
    )
    .map_err(|e| format!("insert plugin_versions: {e}"))?;

    let existing: Option<(String, i64)> = conn
        .query_row(
            "SELECT current_version, enabled FROM plugins WHERE id = ?1",
            params![plugin_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    match existing {
        None => {
            // First install: current_version points at the new package, stays disabled.
            conn.execute(
                "INSERT INTO plugins (
                    id, current_version, enabled, runtime_state, installed_at, updated_at
                 ) VALUES (?1, ?2, 0, 'disabled', ?3, ?3)",
                params![plugin_id, version, now],
            )
            .map_err(|e| format!("insert plugins: {e}"))?;
        }
        Some((current, _enabled)) if current == version => {
            // Same version re-imported (identical or replaced hash already guarded by package.rs).
            conn.execute(
                "UPDATE plugins SET updated_at = ?1, pending_version = NULL WHERE id = ?2",
                params![now, plugin_id],
            )
            .map_err(|e| format!("touch plugins: {e}"))?;
        }
        Some((_current, enabled)) if enabled == 0 => {
            // Disabled plugin: switch current immediately; user must re-trust + enable.
            conn.execute(
                "UPDATE plugins SET current_version = ?1, pending_version = NULL,
                    enabled = 0, runtime_state = 'disabled', updated_at = ?2 WHERE id = ?3",
                params![version, now, plugin_id],
            )
            .map_err(|e| format!("update plugins current: {e}"))?;
        }
        Some(_) => {
            // Enabled plugin update: keep current running, stage as pending until promote.
            conn.execute(
                "UPDATE plugins SET pending_version = ?1, updated_at = ?2 WHERE id = ?3",
                params![version, now, plugin_id],
            )
            .map_err(|e| format!("set pending_version: {e}"))?;
        }
    }

    Ok(())
}

/// Promote `pending_version` → `current_version` after the user trusts the new package
/// (design §8.4). Caller must stop the old Runtime / UI before invoking this.
pub fn promote_pending_version(conn: &Connection, plugin_id: &str) -> Result<String, String> {
    let pending: Option<String> = conn
        .query_row(
            "SELECT pending_version FROM plugins WHERE id = ?1",
            params![plugin_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("read pending_version: {e}"))?;
    let Some(pending) = pending.filter(|v| !v.is_empty()) else {
        return Err("没有待切换的插件版本".into());
    };
    let trusted: bool = conn
        .query_row(
            "SELECT trusted_at IS NOT NULL FROM plugin_versions
             WHERE plugin_id = ?1 AND version = ?2",
            params![plugin_id, pending],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if !trusted {
        return Err("请先信任待切换的新版本包".into());
    }
    let now = chrono::Local::now().to_rfc3339();
    conn.execute(
        "UPDATE plugins SET current_version = ?1, pending_version = NULL, updated_at = ?2
         WHERE id = ?3",
        params![pending, now, plugin_id],
    )
    .map_err(|e| format!("promote pending_version: {e}"))?;
    Ok(pending)
}

pub fn set_package_trusted(
    conn: &Connection,
    plugin_id: &str,
    version: &str,
    trusted: bool,
) -> Result<(), String> {
    let trusted_at = if trusted {
        Some(chrono::Local::now().to_rfc3339())
    } else {
        None
    };
    let changed = conn
        .execute(
            "UPDATE plugin_versions SET trusted_at = ?1
             WHERE plugin_id = ?2 AND version = ?3",
            params![trusted_at, plugin_id, version],
        )
        .map_err(|e| format!("update trust: {e}"))?;
    if changed == 0 {
        return Err("plugin version not found".into());
    }
    Ok(())
}

pub fn set_plugin_enabled(conn: &Connection, plugin_id: &str, enabled: bool) -> Result<(), String> {
    let state = if enabled { "enabled" } else { "disabled" };
    let now = chrono::Local::now().to_rfc3339();
    let changed = conn
        .execute(
            "UPDATE plugins SET enabled = ?1, runtime_state = ?2, updated_at = ?3 WHERE id = ?4",
            params![enabled as i64, state, now, plugin_id],
        )
        .map_err(|e| format!("update enabled: {e}"))?;
    if changed == 0 {
        return Err("plugin not found".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginRow {
    pub id: String,
    pub current_version: String,
    pub pending_version: Option<String>,
    pub enabled: bool,
    pub runtime_state: String,
    pub package_hash: Option<String>,
    pub trusted: bool,
    pub install_source: String,
    pub signature_status: String,
    pub display_publisher: Option<String>,
    pub requires_node_runtime: bool,
    pub last_error: Option<String>,
    pub mcp_exposed: bool,
    /// Filled by callers that read the manifest from disk (design §11): number of
    /// `contributes.mcpTools` entries. `0` for plugins with none, or when unread.
    pub mcp_tool_count: usize,
}

pub fn list_installed_plugins(conn: &Connection) -> Result<Vec<InstalledPluginRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.current_version, p.pending_version, p.enabled, p.runtime_state, p.last_error,
                    v.package_hash, v.trusted_at, v.install_source, v.signature_status,
                    v.display_publisher, COALESCE(m.exposed, 0)
             FROM plugins p
             LEFT JOIN plugin_versions v
               ON v.plugin_id = p.id AND v.version = p.current_version
             LEFT JOIN plugin_mcp_exposure m
               ON m.plugin_id = p.id
             ORDER BY p.id",
        )
        .map_err(|e| format!("prepare list plugins: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            let trusted_at: Option<String> = row.get(7)?;
            Ok(InstalledPluginRow {
                id: row.get(0)?,
                current_version: row.get(1)?,
                pending_version: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                runtime_state: row.get(4)?,
                last_error: row.get(5)?,
                package_hash: row.get(6)?,
                trusted: trusted_at.is_some(),
                install_source: row.get::<_, Option<String>>(8)?.unwrap_or_else(|| "local".into()),
                signature_status: row
                    .get::<_, Option<String>>(9)?
                    .unwrap_or_else(|| "unsigned".into()),
                display_publisher: row.get(10)?,
                mcp_exposed: row.get::<_, i64>(11)? != 0,
                // Filled by caller after reading manifest from disk when needed.
                requires_node_runtime: false,
                mcp_tool_count: 0,
            })
        })
        .map_err(|e| format!("query plugins: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("plugin row: {e}"))?);
    }
    Ok(out)
}

/// Remove all DB bookkeeping for a plugin (uninstall). Package files / data directory removal
/// is handled by the caller (`commands::plugins::plugin_uninstall`).
pub fn delete_plugin_records(conn: &Connection, plugin_id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM plugin_versions WHERE plugin_id = ?1", params![plugin_id])
        .map_err(|e| format!("delete plugin_versions: {e}"))?;
    conn.execute(
        "DELETE FROM plugin_mcp_exposure WHERE plugin_id = ?1",
        params![plugin_id],
    )
    .map_err(|e| format!("delete plugin_mcp_exposure: {e}"))?;
    conn.execute("DELETE FROM plugins WHERE id = ?1", params![plugin_id])
        .map_err(|e| format!("delete plugins: {e}"))?;
    Ok(())
}

pub fn get_installed_plugin(
    conn: &Connection,
    plugin_id: &str,
) -> Result<Option<InstalledPluginRow>, String> {
    Ok(list_installed_plugins(conn)?
        .into_iter()
        .find(|row| row.id == plugin_id))
}
