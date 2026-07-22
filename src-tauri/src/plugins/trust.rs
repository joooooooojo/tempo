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
        ",
    )
    .map_err(|e| format!("create plugin tables: {e}"))?;
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

    conn.execute(
        "INSERT INTO plugins (
            id, current_version, enabled, runtime_state, installed_at, updated_at
         ) VALUES (?1, ?2, 0, 'disabled', ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
            current_version = excluded.current_version,
            updated_at = excluded.updated_at",
        params![plugin_id, version, now],
    )
    .map_err(|e| format!("upsert plugins: {e}"))?;

    Ok(())
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
    pub enabled: bool,
    pub runtime_state: String,
    pub package_hash: Option<String>,
    pub trusted: bool,
    pub install_source: String,
    pub signature_status: String,
    pub display_publisher: Option<String>,
    pub requires_node_runtime: bool,
    pub last_error: Option<String>,
}

pub fn list_installed_plugins(conn: &Connection) -> Result<Vec<InstalledPluginRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.current_version, p.enabled, p.runtime_state, p.last_error,
                    v.package_hash, v.trusted_at, v.install_source, v.signature_status,
                    v.display_publisher
             FROM plugins p
             LEFT JOIN plugin_versions v
               ON v.plugin_id = p.id AND v.version = p.current_version
             ORDER BY p.id",
        )
        .map_err(|e| format!("prepare list plugins: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            let trusted_at: Option<String> = row.get(6)?;
            Ok(InstalledPluginRow {
                id: row.get(0)?,
                current_version: row.get(1)?,
                enabled: row.get::<_, i64>(2)? != 0,
                runtime_state: row.get(3)?,
                last_error: row.get(4)?,
                package_hash: row.get(5)?,
                trusted: trusted_at.is_some(),
                install_source: row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "local".into()),
                signature_status: row
                    .get::<_, Option<String>>(8)?
                    .unwrap_or_else(|| "unsigned".into()),
                display_publisher: row.get(9)?,
                // Filled by caller after reading manifest from disk when needed.
                requires_node_runtime: false,
            })
        })
        .map_err(|e| format!("query plugins: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("plugin row: {e}"))?);
    }
    Ok(out)
}
