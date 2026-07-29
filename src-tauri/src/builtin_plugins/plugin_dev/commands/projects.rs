use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use tauri::{AppHandle, State};

use crate::db::AppState;
use crate::plugins::host::{generate_id, PluginHost};
use crate::plugins::manifest::PluginManifest;

use super::connect::plugin_dev_disconnect;
use super::paths::*;
use super::types::*;

pub(super) fn manifest_document(raw: String) -> PluginDevManifestDocument {
    let hash = hash_text(&raw);
    match serde_json::from_str::<Value>(&raw) {
        Ok(parsed) => match PluginManifest::parse_str(&raw) {
            Ok(_) => PluginDevManifestDocument {
                raw,
                hash,
                parsed: Some(parsed),
                valid: true,
                diagnostics: Vec::new(),
            },
            Err(message) => PluginDevManifestDocument {
                raw,
                hash,
                parsed: Some(parsed),
                valid: false,
                diagnostics: vec![ManifestDiagnostic {
                    severity: "error".into(),
                    code: "MANIFEST_SEMANTIC".into(),
                    line: None,
                    column: None,
                    message,
                }],
            },
        },
        Err(error) => PluginDevManifestDocument {
            raw,
            hash,
            parsed: None,
            valid: false,
            diagnostics: vec![ManifestDiagnostic {
                severity: "error".into(),
                code: "MANIFEST_JSON".into(),
                line: Some(error.line()),
                column: Some(error.column()),
                message: format!("invalid manifest.json: {error}"),
            }],
        },
    }
}

pub(super) fn read_manifest_document(root: &Path) -> Result<PluginDevManifestDocument, String> {
    let path = manifest_path(root);
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("读取 {} 失败: {error}", path.display()))?;
    Ok(manifest_document(raw))
}

pub(super) fn project_root(conn: &Connection, project_id: &str) -> Result<PathBuf, String> {
    let root: Option<String> = conn
        .query_row(
            "SELECT root_path FROM plugin_dev_projects WHERE id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取项目失败: {error}"))?;
    let root = root.ok_or_else(|| "开发项目不存在".to_string())?;
    canonical_directory(&root)
}

pub(super) fn preferences_for(conn: &Connection, project_id: &str) -> Result<PluginDevPreferences, String> {
    ensure_plugin_dev_tables(conn)?;
    conn.query_row(
        "SELECT ui_source_kind, ui_service_url, ui_static_root, runtime_dev_entry,
                auto_reconnect_runtime, receive_real_hooks, use_production_data
         FROM plugin_dev_preferences WHERE project_id = ?1",
        [project_id],
        |row| {
            Ok(PluginDevPreferences {
                ui_source_kind: row.get(0)?,
                ui_service_url: row.get(1)?,
                ui_static_root: row.get(2)?,
                runtime_dev_entry: row.get(3)?,
                auto_reconnect_runtime: row.get::<_, i64>(4)? != 0,
                receive_real_hooks: row.get::<_, i64>(5)? != 0,
                use_production_data: row.get::<_, i64>(6)? != 0,
            })
        },
    )
    .optional()
    .map(|value| value.unwrap_or_default())
    .map_err(|error| format!("读取开发连接设置失败: {error}"))
}

pub(super) fn connection_status(host: &PluginHost, project_id: &str) -> PluginDevConnectionStatus {
    let entry = host
        .development_plugins()
        .into_iter()
        .find(|candidate| candidate.project_id == project_id);
    let Some(entry) = entry else {
        return PluginDevConnectionStatus {
            connected: false,
            plugin_id: None,
            state: "disconnected".into(),
            ui_state: None,
            runtime_state: None,
            message: None,
        };
    };

    let runtime_state = entry.runtime_entry.as_ref().map(|_| {
        if host.supervisor.is_running(&entry.manifest.id) {
            "ready".to_string()
        } else {
            "stopped".to_string()
        }
    });
    let runtime_stopped = runtime_state.as_deref() == Some("stopped");
    PluginDevConnectionStatus {
        connected: true,
        plugin_id: Some(entry.manifest.id.clone()),
        state: if runtime_stopped && entry.ui_source.is_some() {
            "partial".into()
        } else if runtime_stopped {
            "failed".into()
        } else {
            "connected".into()
        },
        ui_state: entry.ui_source.as_ref().map(|_| "connected".into()),
        runtime_state,
        message: None,
    }
}

pub(super) fn row_to_project(
    row: &rusqlite::Row<'_>,
    connected_projects: &[String],
) -> rusqlite::Result<PluginDevProject> {
    let id: String = row.get(0)?;
    let root_path: String = row.get(1)?;
    Ok(PluginDevProject {
        connected: connected_projects.iter().any(|candidate| candidate == &id),
        id,
        root_path: path_to_storage_string(Path::new(&root_path)),
        plugin_id: row.get(2)?,
        name: row.get(3)?,
        kind: row.get(4)?,
        last_opened_at: row.get(5)?,
        created_at: row.get(6)?,
    })
}

pub(super) fn project_by_id(
    conn: &Connection,
    host: &PluginHost,
    project_id: &str,
) -> Result<PluginDevProject, String> {
    let connected = host
        .development_plugins()
        .into_iter()
        .map(|entry| entry.project_id)
        .collect::<Vec<_>>();
    conn.query_row(
        "SELECT id, root_path, plugin_id, name, kind, last_opened_at, created_at
         FROM plugin_dev_projects WHERE id = ?1",
        [project_id],
        |row| row_to_project(row, &connected),
    )
    .optional()
    .map_err(|error| format!("读取项目失败: {error}"))?
    .ok_or_else(|| "开发项目不存在".into())
}

pub(super) fn update_project_manifest_metadata(
    conn: &Connection,
    project_id: &str,
    document: &PluginDevManifestDocument,
) -> Result<(), String> {
    let plugin_id = document
        .parsed
        .as_ref()
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str);
    let name = document
        .parsed
        .as_ref()
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str);
    let kind = PluginManifest::parse_str(&document.raw)
        .ok()
        .map(|manifest| manifest.resolved_kind().to_string())
        .or_else(|| {
            document
                .parsed
                .as_ref()
                .and_then(|value| value.get("kind"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    conn.execute(
        "UPDATE plugin_dev_projects
         SET plugin_id = ?1, name = ?2, kind = ?3, last_opened_at = ?4
         WHERE id = ?5",
        params![plugin_id, name, kind, Local::now().to_rfc3339(), project_id],
    )
    .map_err(|error| format!("更新项目信息失败: {error}"))?;
    Ok(())
}

pub(super) fn detail(
    conn: &Connection,
    host: &PluginHost,
    project_id: &str,
) -> Result<PluginDevProjectDetail, String> {
    let root = project_root(conn, project_id)?;
    let manifest = read_manifest_document(&root)?;
    update_project_manifest_metadata(conn, project_id, &manifest)?;
    Ok(PluginDevProjectDetail {
        project: project_by_id(conn, host, project_id)?,
        preferences: preferences_for(conn, project_id)?,
        connection: connection_status(host, project_id),
        manifest,
    })
}

pub(super) fn minimal_manifest(args: &CreateProjectArgs) -> Result<String, String> {
    let mut contributes = json!({
        "apps": [],
        "actions": [],
        "commands": [],
        "hooks": [],
        "settings": [],
        "mcpTools": []
    });
    let main = match args.kind.as_str() {
        "ui" => {
            contributes["apps"] = json!([{
                "id": "main",
                "name": args.name.trim(),
                "keywords": [],
                "entry": "index.html",
                "windowMode": "normal"
            }]);
            None
        }
        "headless" => Some("main.mjs"),
        "hybrid" => {
            contributes["apps"] = json!([{
                "id": "main",
                "name": args.name.trim(),
                "keywords": [],
                "entry": "index.html",
                "windowMode": "normal"
            }]);
            Some("main.mjs")
        }
        _ => return Err("插件类型必须是 ui、headless 或 hybrid".into()),
    };
    let mut value = json!({
        "manifestVersion": 1,
        "id": args.plugin_id.trim(),
        "name": args.name.trim(),
        "version": "0.1.0",
        "engines": { "tempo": ">=2", "pluginApi": "^1.3.0" },
        "kind": args.kind,
        "contributes": contributes
    });
    if let Some(main) = main {
        value["main"] = Value::String(main.into());
    }
    let raw = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("生成 Manifest 失败: {error}"))?;
    PluginManifest::parse_str(&raw)?;
    Ok(format!("{raw}\n"))
}


pub fn ensure_plugin_dev_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS plugin_dev_projects (
            id TEXT PRIMARY KEY,
            root_path TEXT NOT NULL UNIQUE,
            plugin_id TEXT,
            name TEXT,
            kind TEXT,
            last_opened_at TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS plugin_dev_preferences (
            project_id TEXT PRIMARY KEY,
            ui_source_kind TEXT,
            ui_service_url TEXT,
            ui_static_root TEXT,
            runtime_dev_entry TEXT,
            auto_reconnect_runtime INTEGER NOT NULL DEFAULT 1,
            receive_real_hooks INTEGER NOT NULL DEFAULT 0,
            use_production_data INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(project_id) REFERENCES plugin_dev_projects(id) ON DELETE CASCADE
        );",
    )
    .map_err(|error| format!("prepare plugin dev tables: {error}"))?;
    migrate_plugin_dev_paths(conn)
}


#[tauri::command]
pub fn plugin_dev_list_projects(
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
) -> Result<Vec<PluginDevProject>, String> {
    let conn = state.db.lock();
    ensure_plugin_dev_tables(&conn)?;
    let connected = host
        .development_plugins()
        .into_iter()
        .map(|entry| entry.project_id)
        .collect::<Vec<_>>();
    let mut stmt = conn
        .prepare(
            "SELECT id, root_path, plugin_id, name, kind, last_opened_at, created_at
             FROM plugin_dev_projects ORDER BY last_opened_at DESC",
        )
        .map_err(|error| format!("读取项目列表失败: {error}"))?;
    let rows = stmt
        .query_map([], |row| row_to_project(row, &connected))
        .map_err(|error| format!("读取项目列表失败: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取项目列表失败: {error}"))
}

#[tauri::command]
pub fn plugin_dev_create_project(
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: CreateProjectArgs,
) -> Result<PluginDevProjectDetail, String> {
    let root = path_from_input(&args.root_path)?;
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("创建项目目录失败 {}: {error}", root.display()))?;
    let root = root
        .canonicalize()
        .map_err(|error| format!("读取项目目录失败: {error}"))?;
    let path = manifest_path(&root);
    if path.exists() {
        return Err("目标目录已经存在 manifest.json，请使用“打开项目”".into());
    }
    let raw = minimal_manifest(&args)?;
    std::fs::write(&path, raw.as_bytes())
        .map_err(|error| format!("写入 {} 失败: {error}", path.display()))?;

    let now = Local::now().to_rfc3339();
    let id = format!("project-{}", generate_id());
    let conn = state.db.lock();
    ensure_plugin_dev_tables(&conn)?;
    conn.execute(
        "INSERT INTO plugin_dev_projects
           (id, root_path, plugin_id, name, kind, last_opened_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            path_to_storage_string(&root),
            args.plugin_id.trim(),
            args.name.trim(),
            args.kind,
            now,
            now
        ],
    )
    .map_err(|error| format!("登记开发项目失败: {error}"))?;
    detail(&conn, &host, &id)
}

#[tauri::command]
pub fn plugin_dev_open_project(
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: OpenProjectArgs,
) -> Result<PluginDevProjectDetail, String> {
    let root = canonical_directory(&args.root_path)?;
    if !manifest_path(&root).is_file() {
        return Err("所选目录根部没有 manifest.json".into());
    }
    let document = read_manifest_document(&root)?;
    let now = Local::now().to_rfc3339();
    let root_path = path_to_storage_string(&root);
    let conn = state.db.lock();
    ensure_plugin_dev_tables(&conn)?;
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM plugin_dev_projects WHERE root_path = ?1",
            [&root_path],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("查找开发项目失败: {error}"))?;
    let id = existing_id.unwrap_or_else(|| format!("project-{}", generate_id()));
    conn.execute(
        "INSERT INTO plugin_dev_projects
           (id, root_path, last_opened_at, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(root_path) DO UPDATE SET last_opened_at = excluded.last_opened_at",
        params![id, root_path, now, now],
    )
    .map_err(|error| format!("登记开发项目失败: {error}"))?;
    update_project_manifest_metadata(&conn, &id, &document)?;
    detail(&conn, &host, &id)
}

#[tauri::command]
pub fn plugin_dev_get_project(
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: ProjectIdArgs,
) -> Result<PluginDevProjectDetail, String> {
    let conn = state.db.lock();
    ensure_plugin_dev_tables(&conn)?;
    detail(&conn, &host, &args.project_id)
}

#[tauri::command]
pub fn plugin_dev_write_manifest(
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: WriteManifestArgs,
) -> Result<PluginDevProjectDetail, String> {
    let conn = state.db.lock();
    ensure_plugin_dev_tables(&conn)?;
    let root = project_root(&conn, &args.project_id)?;
    let path = manifest_path(&root);
    let current = std::fs::read_to_string(&path)
        .map_err(|error| format!("读取 {} 失败: {error}", path.display()))?;
    if hash_text(&current) != args.expected_hash {
        return Err("manifest.json 已被外部修改，请重新载入后再保存".into());
    }
    let temp = root.join(format!(".manifest.json.{}.tmp", generate_id()));
    std::fs::write(&temp, args.raw.as_bytes())
        .map_err(|error| format!("写入 Manifest 临时文件失败: {error}"))?;
    if let Err(error) = replace_file(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    detail(&conn, &host, &args.project_id)
}

#[tauri::command]
pub fn plugin_dev_update_preferences(
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: UpdatePreferencesArgs,
) -> Result<PluginDevProjectDetail, String> {
    let conn = state.db.lock();
    ensure_plugin_dev_tables(&conn)?;
    project_root(&conn, &args.project_id)?;
    let preferences = args.preferences;
    conn.execute(
        "INSERT INTO plugin_dev_preferences
           (project_id, ui_source_kind, ui_service_url, ui_static_root, runtime_dev_entry,
            auto_reconnect_runtime, receive_real_hooks, use_production_data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(project_id) DO UPDATE SET
           ui_source_kind = excluded.ui_source_kind,
           ui_service_url = excluded.ui_service_url,
           ui_static_root = excluded.ui_static_root,
           runtime_dev_entry = excluded.runtime_dev_entry,
           auto_reconnect_runtime = excluded.auto_reconnect_runtime,
           receive_real_hooks = excluded.receive_real_hooks,
           use_production_data = excluded.use_production_data",
        params![
            args.project_id,
            preferences.ui_source_kind,
            preferences.ui_service_url,
            preferences.ui_static_root,
            preferences.runtime_dev_entry,
            preferences.auto_reconnect_runtime as i64,
            preferences.receive_real_hooks as i64,
            preferences.use_production_data as i64,
        ],
    )
    .map_err(|error| format!("保存开发连接设置失败: {error}"))?;
    detail(&conn, &host, &args.project_id)
}

#[tauri::command]
pub async fn plugin_dev_forget_project(
    app: AppHandle,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: ProjectIdArgs,
) -> Result<(), String> {
    plugin_dev_disconnect(
        app,
        host.clone(),
        ProjectIdArgs {
            project_id: args.project_id.clone(),
        },
    )
    .await?;
    let conn = state.db.lock();
    ensure_plugin_dev_tables(&conn)?;
    conn.execute(
        "DELETE FROM plugin_dev_projects WHERE id = ?1",
        [&args.project_id],
    )
    .map_err(|error| format!("删除项目记录失败: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{manifest_document, minimal_manifest, CreateProjectArgs};
    use super::super::connect::{resolve_runtime_entry, resolve_static_source};
    use super::super::paths::{path_from_input, path_to_storage_string};
    use super::super::probe::validate_loopback_url;
    use crate::plugins::host::generate_id;
    use crate::plugins::manifest::PluginManifest;

    #[test]
    fn generated_manifests_are_valid_for_all_kinds() {
        for kind in ["ui", "headless", "hybrid"] {
            let raw = minimal_manifest(&CreateProjectArgs {
                root_path: String::new(),
                plugin_id: format!("com.example.{kind}"),
                name: format!("Example {kind}"),
                kind: kind.into(),
            })
            .unwrap();
            assert!(manifest_document(raw).valid);
        }
    }

    #[test]
    fn loopback_url_validation_rejects_remote_hosts() {
        for accepted in [
            "http://localhost:5173/",
            "https://127.0.0.1:4173/app",
            "http://[::1]:3000/",
        ] {
            assert!(validate_loopback_url(accepted).is_ok(), "{accepted}");
        }
        for rejected in [
            "http://0.0.0.0:5173/",
            "https://example.com/",
            "file:///tmp/index.html",
        ] {
            assert!(validate_loopback_url(rejected).is_err(), "{rejected}");
        }
    }

    #[test]
    fn relative_development_paths_resolve_from_project_root() {
        let root = std::env::temp_dir().join(format!("tempo-plugin-dev-{}", generate_id()));
        let static_root = root.join("web");
        std::fs::create_dir_all(&static_root).unwrap();
        let manifest = PluginManifest::parse_str(
            r#"{
              "manifestVersion": 1,
              "id": "com.example.paths",
              "name": "Paths",
              "version": "0.1.0",
              "engines": { "tempo": ">=2", "pluginApi": "^1.3.0" },
              "kind": "headless",
              "main": "main.mjs",
              "contributes": { "commands": [] }
            }"#,
        )
        .unwrap();

        assert_eq!(
            resolve_static_source(&root, Some("web")).unwrap(),
            static_root.canonicalize().unwrap()
        );
        assert_eq!(
            resolve_runtime_entry(&root, &manifest, Some("build/dev.mjs")).unwrap(),
            Some(root.join("build/dev.mjs"))
        );
        assert!(resolve_runtime_entry(&root, &manifest, Some("src/main.ts")).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_hide_extended_prefixes_and_decode_file_urls() {
        assert_eq!(
            path_to_storage_string(std::path::Path::new(r"\\?\C:\Users\Tempo\plugin")),
            r"C:\Users\Tempo\plugin"
        );
        assert_eq!(
            path_to_storage_string(std::path::Path::new(r"\\?\UNC\server\plugins\example")),
            r"\\server\plugins\example"
        );
        assert_eq!(
            path_from_input("file:///C:/Users/Tempo/plugin").unwrap(),
            std::path::PathBuf::from(r"C:\Users\Tempo\plugin")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_file_urls_decode_to_posix_paths() {
        assert_eq!(
            path_from_input("file:///Users/tempo/My%20Plugin").unwrap(),
            std::path::PathBuf::from("/Users/tempo/My Plugin")
        );
    }
}
