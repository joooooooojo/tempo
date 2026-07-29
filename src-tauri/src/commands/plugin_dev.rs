use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};

use crate::db::AppState;
use crate::plugins::host::{generate_id, DevelopmentPlugin, DevelopmentUiSource, PluginHost};
use crate::plugins::manifest::PluginManifest;

const CONTRIBUTIONS_CHANGED_EVENT: &str = "plugin-contributions-changed";
const UI_RELOAD_EVENT: &str = "plugin-dev://ui-reload";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevProject {
    pub id: String,
    pub root_path: String,
    pub plugin_id: Option<String>,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub last_opened_at: String,
    pub created_at: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestDiagnostic {
    pub severity: String,
    pub code: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevManifestDocument {
    pub raw: String,
    pub hash: String,
    pub parsed: Option<Value>,
    pub valid: bool,
    pub diagnostics: Vec<ManifestDiagnostic>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevPreferences {
    pub ui_source_kind: Option<String>,
    pub ui_service_url: Option<String>,
    pub ui_static_root: Option<String>,
    pub runtime_dev_entry: Option<String>,
    #[serde(default = "default_true")]
    pub auto_reconnect_runtime: bool,
    #[serde(default)]
    pub receive_real_hooks: bool,
    #[serde(default)]
    pub use_production_data: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevConnectionStatus {
    pub connected: bool,
    pub plugin_id: Option<String>,
    pub state: String,
    pub ui_state: Option<String>,
    pub runtime_state: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevProjectDetail {
    pub project: PluginDevProject,
    pub manifest: PluginDevManifestDocument,
    pub preferences: PluginDevPreferences,
    pub connection: PluginDevConnectionStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectArgs {
    pub root_path: String,
    pub plugin_id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIdArgs {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectArgs {
    pub root_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteManifestArgs {
    pub project_id: String,
    pub raw: String,
    pub expected_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePreferencesArgs {
    pub project_id: String,
    pub preferences: PluginDevPreferences,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeUiUrlArgs {
    pub url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeUiUrlResult {
    pub reachable: bool,
    pub status: Option<u16>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunHookArgs {
    pub project_id: String,
    pub event: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMcpToolArgs {
    pub project_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub arguments: Value,
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

fn hash_text(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

fn path_from_input(path: &str) -> Result<PathBuf, String> {
    let value = path.trim();
    if value.to_ascii_lowercase().starts_with("file:") {
        return reqwest::Url::parse(value)
            .map_err(|error| format!("无效文件路径: {error}"))?
            .to_file_path()
            .map_err(|_| "文件 URL 不是本机路径".to_string());
    }
    Ok(PathBuf::from(value))
}

#[cfg(windows)]
fn path_to_storage_string(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}

#[cfg(not(windows))]
fn path_to_storage_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(windows)]
fn migrate_plugin_dev_paths(conn: &Connection) -> Result<(), String> {
    let projects = {
        let mut stmt = conn
            .prepare("SELECT id, root_path FROM plugin_dev_projects")
            .map_err(|error| format!("读取开发项目路径失败: {error}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| format!("读取开发项目路径失败: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取开发项目路径失败: {error}"))?
    };
    for (id, path) in projects {
        let normalized = path_to_storage_string(Path::new(&path));
        if normalized != path {
            conn.execute(
                "UPDATE plugin_dev_projects SET root_path = ?1 WHERE id = ?2",
                params![normalized, id],
            )
            .map_err(|error| format!("更新开发项目路径失败: {error}"))?;
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn migrate_plugin_dev_paths(_conn: &Connection) -> Result<(), String> {
    Ok(())
}

fn canonical_directory(path: &str) -> Result<PathBuf, String> {
    let path = path_from_input(path)?;
    if !path.is_dir() {
        return Err(format!("目录不存在: {}", path.display()));
    }
    path.canonicalize()
        .map_err(|error| format!("读取目录失败 {}: {error}", path.display()))
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join("manifest.json")
}

fn manifest_document(raw: String) -> PluginDevManifestDocument {
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

fn read_manifest_document(root: &Path) -> Result<PluginDevManifestDocument, String> {
    let path = manifest_path(root);
    let raw = std::fs::read_to_string(&path)
        .map_err(|error| format!("读取 {} 失败: {error}", path.display()))?;
    Ok(manifest_document(raw))
}

fn project_root(conn: &Connection, project_id: &str) -> Result<PathBuf, String> {
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

fn preferences_for(conn: &Connection, project_id: &str) -> Result<PluginDevPreferences, String> {
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

fn connection_status(host: &PluginHost, project_id: &str) -> PluginDevConnectionStatus {
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

fn row_to_project(
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

fn project_by_id(
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

fn update_project_manifest_metadata(
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

fn detail(
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

fn minimal_manifest(args: &CreateProjectArgs) -> Result<String, String> {
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

#[cfg(not(windows))]
fn replace_file(temp: &Path, destination: &Path) -> Result<(), String> {
    std::fs::rename(temp, destination).map_err(|error| format!("替换 manifest.json 失败: {error}"))
}

#[cfg(windows)]
fn replace_file(temp: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| format!("替换 manifest.json 失败: {error}"))
    }
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

fn validate_loopback_url(raw: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|error| format!("无效服务 URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("服务 URL 只支持 http 或 https".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "服务 URL 缺少 host".to_string())?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
        || host == "[::1]";
    if !loopback {
        return Err("开发服务 URL 必须使用 localhost、127.0.0.1 或 [::1]".into());
    }
    Ok(url)
}

#[tauri::command]
pub async fn plugin_dev_probe_ui_url(args: ProbeUiUrlArgs) -> Result<ProbeUiUrlResult, String> {
    let url = validate_loopback_url(&args.url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| format!("创建连接检测失败: {error}"))?;
    match client.get(url).send().await {
        Ok(response) => Ok(ProbeUiUrlResult {
            reachable: true,
            status: Some(response.status().as_u16()),
            message: format!("服务已响应 HTTP {}", response.status().as_u16()),
        }),
        Err(error) => Ok(ProbeUiUrlResult {
            reachable: false,
            status: None,
            message: format!("服务暂不可达: {error}"),
        }),
    }
}

fn resolve_static_source(root: &Path, configured: Option<&str>) -> Result<PathBuf, String> {
    let candidate = configured
        .filter(|value| !value.trim().is_empty())
        .map(path_from_input)
        .transpose()?
        .unwrap_or_else(|| root.to_path_buf());
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    canonical_directory(candidate.to_string_lossy().as_ref())
}

fn resolve_runtime_entry(
    root: &Path,
    manifest: &PluginManifest,
    configured: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let Some(main) = &manifest.main else {
        return Ok(None);
    };
    let candidate = configured
        .filter(|value| !value.trim().is_empty())
        .map(path_from_input)
        .transpose()?
        .unwrap_or_else(|| root.join(main));
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    let extension = candidate
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension != "js" && extension != "mjs" {
        return Err("Runtime 开发入口必须是 .js 或 .mjs 文件".into());
    }
    let resolved = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|error| format!("读取 Runtime 入口失败: {error}"))?
    } else {
        candidate
    };
    Ok(Some(resolved))
}

fn runtime_file_signature(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some((metadata.len(), metadata.modified().ok()?))
}

fn spawn_runtime_entry_watcher(
    app: AppHandle,
    host: Arc<PluginHost>,
    plugin_id: String,
    project_id: String,
    session_id: String,
    entry_path: PathBuf,
) {
    tauri::async_runtime::spawn(async move {
        let mut previous = runtime_file_signature(&entry_path);
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;
            let current_connection = host.development_plugin(&plugin_id);
            if !current_connection.is_some_and(|entry| {
                entry.project_id == project_id && entry.session_id == session_id
            }) {
                break;
            }

            let next = runtime_file_signature(&entry_path);
            if next == previous {
                continue;
            }
            previous = next;
            if next.is_none() {
                continue;
            }

            tokio::time::sleep(Duration::from_millis(250)).await;
            let stable = runtime_file_signature(&entry_path);
            if stable != next {
                previous = stable;
                continue;
            }

            let _ = app.emit(
                "plugin-dev://runtime-state",
                json!({ "pluginId": plugin_id, "state": "reconnecting" }),
            );
            host.supervisor.stop_development(&plugin_id).await;
            let result = host.supervisor.ensure_started(&plugin_id).await;
            let (state, message) = match result {
                Ok(()) => ("ready", None),
                Err(error) => ("failed", Some(error.message)),
            };
            let _ = app.emit(
                "plugin-dev://runtime-state",
                json!({ "pluginId": plugin_id, "state": state, "message": message }),
            );
        }
    });
}

#[tauri::command]
pub async fn plugin_dev_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: ProjectIdArgs,
) -> Result<PluginDevConnectionStatus, String> {
    let (root, manifest, preferences) = {
        let conn = state.db.lock();
        ensure_plugin_dev_tables(&conn)?;
        let root = project_root(&conn, &args.project_id)?;
        let raw = std::fs::read_to_string(manifest_path(&root))
            .map_err(|error| format!("读取 manifest.json 失败: {error}"))?;
        let manifest = PluginManifest::parse_str(&raw)?;
        let preferences = preferences_for(&conn, &args.project_id)?;
        (root, manifest, preferences)
    };

    let ui_source = if manifest.has_ui() {
        match preferences.ui_source_kind.as_deref() {
            Some("url") => {
                let raw = preferences
                    .ui_service_url
                    .as_deref()
                    .ok_or_else(|| "请填写 UI 服务 URL".to_string())?;
                Some(DevelopmentUiSource::Url(
                    validate_loopback_url(raw)?.to_string(),
                ))
            }
            Some("static") => {
                let static_root =
                    resolve_static_source(&root, preferences.ui_static_root.as_deref())?;
                for app_contribution in &manifest.contributes.apps {
                    let entry = static_root.join(&app_contribution.entry);
                    if !entry.is_file() {
                        return Err(format!("静态入口不存在: {}", entry.display()));
                    }
                }
                Some(DevelopmentUiSource::Static(static_root))
            }
            _ => return Err("请选择 UI 服务 URL 或静态目录".into()),
        }
    } else {
        None
    };
    let runtime_entry =
        resolve_runtime_entry(&root, &manifest, preferences.runtime_dev_entry.as_deref())?;
    let plugin_id = manifest.id.clone();

    if let Some(existing) = host.development_plugin(&plugin_id) {
        if existing.project_id != args.project_id {
            return Err(format!("插件 {} 已由另一个开发项目连接", plugin_id));
        }
    }

    let previous_connections = host
        .development_plugins()
        .into_iter()
        .filter(|entry| entry.project_id == args.project_id)
        .collect::<Vec<_>>();
    for previous in previous_connections {
        host.supervisor
            .stop_development(&previous.manifest.id)
            .await;
        crate::plugins::windows::close_plugin_windows(&app, &host, &previous.manifest.id);
        for view_id in host.views_for_plugin(&previous.manifest.id) {
            host.destroy_view(&view_id);
        }
        host.release_all_subscriptions_for_plugin(&previous.manifest.id);
        host.remove_development_plugin(&previous.manifest.id);
    }
    host.supervisor.stop_development(&plugin_id).await;

    let session_id = generate_id();
    host.register_development_plugin(DevelopmentPlugin {
        session_id: session_id.clone(),
        project_id: args.project_id.clone(),
        root_path: root,
        manifest,
        ui_source,
        runtime_entry: runtime_entry.clone(),
        receive_real_hooks: preferences.receive_real_hooks,
        use_production_data: preferences.use_production_data,
    });
    let _ = app.emit(CONTRIBUTIONS_CHANGED_EVENT, ());

    let runtime_error = if runtime_entry.is_some() {
        host.supervisor
            .ensure_started(&plugin_id)
            .await
            .err()
            .map(|error| error.message)
    } else {
        None
    };

    if preferences.auto_reconnect_runtime {
        if let Some(entry_path) = runtime_entry {
            spawn_runtime_entry_watcher(
                app.clone(),
                host.inner().clone(),
                plugin_id.clone(),
                args.project_id.clone(),
                session_id,
                entry_path,
            );
        }
    }

    let mut status = connection_status(&host, &args.project_id);
    if let Some(message) = runtime_error {
        status.state = if status.ui_state.is_some() {
            "partial".into()
        } else {
            "failed".into()
        };
        status.runtime_state = Some("failed".into());
        status.message = Some(message);
    }
    Ok(status)
}

#[tauri::command]
pub fn plugin_dev_reload_ui(
    app: AppHandle,
    host: State<'_, Arc<PluginHost>>,
    plugin_id: String,
) -> Result<(), String> {
    let entry = host
        .development_plugin(&plugin_id)
        .ok_or_else(|| "插件尚未通过开发助手连接".to_string())?;
    if !matches!(entry.ui_source, Some(DevelopmentUiSource::Static(_))) {
        return Err("只有静态目录模式需要手动刷新".into());
    }

    for view_id in host.views_for_plugin(&plugin_id) {
        host.release_all_subscriptions_for_view(&view_id);
    }
    app.emit(
        UI_RELOAD_EVENT,
        json!({ "pluginId": plugin_id, "sessionId": entry.session_id }),
    )
    .map_err(|error| format!("刷新插件页面失败: {error}"))
}

#[tauri::command]
pub async fn plugin_dev_disconnect(
    app: AppHandle,
    host: State<'_, Arc<PluginHost>>,
    args: ProjectIdArgs,
) -> Result<PluginDevConnectionStatus, String> {
    let entry = host
        .development_plugins()
        .into_iter()
        .find(|candidate| candidate.project_id == args.project_id);
    if let Some(entry) = entry {
        host.supervisor.stop_development(&entry.manifest.id).await;
        crate::plugins::windows::close_plugin_windows(&app, &host, &entry.manifest.id);
        for view_id in host.views_for_plugin(&entry.manifest.id) {
            host.destroy_view(&view_id);
        }
        host.release_all_subscriptions_for_plugin(&entry.manifest.id);
        host.remove_development_plugin(&entry.manifest.id);
        let _ = app.emit(CONTRIBUTIONS_CHANGED_EVENT, ());
    }
    Ok(connection_status(&host, &args.project_id))
}

#[tauri::command]
pub async fn plugin_dev_reconnect_runtime(
    host: State<'_, Arc<PluginHost>>,
    args: ProjectIdArgs,
) -> Result<PluginDevConnectionStatus, String> {
    let entry = host
        .development_plugins()
        .into_iter()
        .find(|candidate| candidate.project_id == args.project_id)
        .ok_or_else(|| "项目尚未连接到 Tempo".to_string())?;
    if entry.runtime_entry.is_none() {
        return Err("当前插件没有 Runtime 开发入口".into());
    }
    host.supervisor.stop_development(&entry.manifest.id).await;
    host.supervisor
        .ensure_started(&entry.manifest.id)
        .await
        .map_err(|error| error.message)?;
    Ok(connection_status(&host, &args.project_id))
}

#[tauri::command]
pub async fn plugin_dev_simulate_hook(
    host: State<'_, Arc<PluginHost>>,
    args: RunHookArgs,
) -> Result<Value, String> {
    let entry = host
        .development_plugins()
        .into_iter()
        .find(|candidate| candidate.project_id == args.project_id)
        .ok_or_else(|| "项目尚未连接到 Tempo".to_string())?;
    let hooks = entry
        .manifest
        .contributes
        .hooks
        .iter()
        .filter(|hook| hook.event == args.event)
        .collect::<Vec<_>>();
    if hooks.is_empty() {
        return Err(format!("Manifest 未声明 Hook 事件 {}", args.event));
    }
    let mut results = Vec::with_capacity(hooks.len());
    for hook in hooks {
        let result = host
            .supervisor
            .call(
                &entry.manifest.id,
                &hook.command,
                args.payload.clone(),
                crate::plugins::bridge::DEFAULT_TIMEOUT,
            )
            .await
            .map_err(|error| error.message)?;
        results.push(json!({ "command": hook.command, "result": result }));
    }
    Ok(Value::Array(results))
}

#[tauri::command]
pub async fn plugin_dev_run_mcp_tool(
    host: State<'_, Arc<PluginHost>>,
    args: RunMcpToolArgs,
) -> Result<Value, String> {
    let entry = host
        .development_plugins()
        .into_iter()
        .find(|candidate| candidate.project_id == args.project_id)
        .ok_or_else(|| "项目尚未连接到 Tempo".to_string())?;
    let tool = entry
        .manifest
        .contributes
        .mcp_tools
        .iter()
        .find(|tool| tool.name == args.tool_name)
        .ok_or_else(|| format!("Manifest 未声明 MCP Tool {}", args.tool_name))?;
    let input_validator = jsonschema::validator_for(&tool.input_schema)
        .map_err(|error| format!("MCP inputSchema 无效: {error}"))?;
    input_validator
        .validate(&args.arguments)
        .map_err(|error| format!("MCP 输入不符合 Schema: {error}"))?;
    let result = host
        .supervisor
        .call(
            &entry.manifest.id,
            &tool.command,
            args.arguments,
            crate::plugins::bridge::DEFAULT_TIMEOUT,
        )
        .await
        .map_err(|error| error.message)?;
    if let Some(schema) = &tool.output_schema {
        let output_validator = jsonschema::validator_for(schema)
            .map_err(|error| format!("MCP outputSchema 无效: {error}"))?;
        output_validator
            .validate(&result)
            .map_err(|error| format!("MCP 输出不符合 Schema: {error}"))?;
    }
    Ok(result)
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
    use super::{
        manifest_document, minimal_manifest, path_from_input, path_to_storage_string,
        resolve_runtime_entry, resolve_static_source, validate_loopback_url, CreateProjectArgs,
    };
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
        let root = std::env::temp_dir().join(format!("tempo-plugin-dev-{}", super::generate_id()));
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
