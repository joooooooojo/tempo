use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::db::AppState;
use crate::plugins::bridge::{self, ConnectionContext, RpcError};
use crate::plugins::host::PluginHost;
use crate::plugins::ids::{is_valid_local_id, runtime_id};
use crate::plugins::loader::{
    scan_enabled_contributions, with_development_contributions, PluginContributionBundle,
};
use crate::plugins::manifest::PluginManifest;
use crate::plugins::package::{import_directory, import_zip, InstalledPackage};
use crate::plugins::paths::{packages_dir, plugin_data_dir, trash_dir};
use crate::plugins::runtime::{
    get_plugin_runtime_status, uninstall_plugin_runtime, PluginRuntimeStatus,
};
use crate::plugins::storage;
use crate::plugins::trust::{
    delete_plugin_records, ensure_plugin_tables, get_installed_plugin, list_installed_plugins,
    promote_pending_version, record_installed_version, set_package_trusted, set_plugin_enabled,
    set_plugin_mcp_exposed as store_plugin_mcp_exposed, InstalledPluginRow,
};
use crate::plugins::ui;

const CONTRIBUTIONS_CHANGED_EVENT: &str = "plugin-contributions-changed";

#[tauri::command]
pub fn plugin_runtime_status(app: AppHandle) -> Result<PluginRuntimeStatus, String> {
    get_plugin_runtime_status(&app)
}

#[tauri::command]
pub async fn plugin_runtime_install(app: AppHandle) -> Result<PluginRuntimeStatus, String> {
    // Detach from the invoke future so closing the main panel cannot cancel the download.
    crate::plugins::runtime::start_plugin_runtime_install(app)
}

#[tauri::command]
pub fn plugin_runtime_uninstall(app: AppHandle) -> Result<PluginRuntimeStatus, String> {
    uninstall_plugin_runtime(&app)
}

/// Imports either a plugin directory or a `.zip` archive (design §8.2). Nothing in the package
/// executes before the user explicitly trusts it via `trust_plugin`.
#[tauri::command]
pub fn import_local_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<InstalledPackage, String> {
    let source = PathBuf::from(&path);
    let installed = if source.is_dir() {
        import_directory(&app, &source)?
    } else {
        import_zip(&app, &source)?
    };
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let publisher = {
        let manifest_path = PathBuf::from(&installed.install_path).join("manifest.json");
        std::fs::read_to_string(manifest_path)
            .ok()
            .and_then(|raw| PluginManifest::parse_str(&raw).ok())
            .and_then(|m| m.publisher)
    };
    record_installed_version(
        &conn,
        &installed.plugin_id,
        &installed.version,
        &installed.package_hash,
        publisher.as_deref(),
        "local",
    )?;
    Ok(installed)
}

#[tauri::command]
pub fn list_plugins(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<InstalledPluginRow>, String> {
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let mut rows = list_installed_plugins(&conn)?;
    let packages = packages_dir(&app).ok();
    for row in &mut rows {
        // Fallback until the package manifest is read.
        if row.name.is_empty() {
            row.name = row.id.clone();
        }
        if let Some(root) = &packages {
            let install_path = root.join(&row.id).join(&row.current_version);
            let manifest_path = install_path.join("manifest.json");
            if let Ok(raw) = std::fs::read_to_string(manifest_path) {
                if let Ok(manifest) = PluginManifest::parse_str(&raw) {
                    row.name = manifest.name.clone();
                    row.requires_node_runtime = manifest.requires_node_runtime();
                    row.kind = manifest.resolved_kind().to_string();
                    row.mcp_tool_count = manifest.contributes.mcp_tools.len();
                    let disabled_tools =
                        crate::plugins::trust::get_plugin_mcp_disabled_tools(&conn, &row.id)?;
                    row.mcp_enabled_tool_count = manifest
                        .contributes
                        .mcp_tools
                        .iter()
                        .filter(|tool| !disabled_tools.iter().any(|name| name == &tool.name))
                        .count();
                    row.settings_count = manifest.contributes.settings.len();
                    let fingerprint = manifest.mcp_toolset_fingerprint().unwrap_or_default();
                    row.mcp_exposed = row.mcp_exposed
                        && row.mcp_tool_count > 0
                        && row.mcp_approved_fingerprint == fingerprint;
                    let icon_rel = manifest
                        .contributes
                        .apps
                        .iter()
                        .find_map(|app| app.icon.as_ref())
                        .or_else(|| {
                            manifest
                                .contributes
                                .actions
                                .iter()
                                .find_map(|action| action.icon.as_ref())
                        });
                    row.icon_url =
                        icon_rel.and_then(|icon| plugin_icon_data_url(&install_path, icon));
                }
            }
        }
    }
    Ok(rows)
}

fn plugin_icon_data_url(install_path: &std::path::Path, rel_path: &str) -> Option<String> {
    use base64::Engine as _;

    let candidate = install_path.join(rel_path);
    let canonical_root = install_path.canonicalize().ok()?;
    let canonical_path = candidate.canonicalize().ok()?;
    if !canonical_path.starts_with(&canonical_root) || !canonical_path.is_file() {
        return None;
    }
    let bytes = std::fs::read(&canonical_path).ok()?;
    if bytes.len() > 256 * 1024 {
        return None;
    }
    let mime = match canonical_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPluginMcpExposedArgs {
    pub plugin_id: String,
    pub exposed: bool,
}

/// User opt-in toggle for exposing a plugin's `contributes.mcpTools` to MCP/AI callers
/// (design §11, Phase 2). Defaults to off for every plugin.
#[tauri::command]
pub fn set_plugin_mcp_exposed(
    app: AppHandle,
    state: State<'_, AppState>,
    args: SetPluginMcpExposedArgs,
) -> Result<(), String> {
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let fingerprint = if args.exposed {
        let row = get_installed_plugin(&conn, &args.plugin_id)?
            .ok_or_else(|| "plugin not found".to_string())?;
        if !row.trusted {
            return Err("信任插件后才能向 MCP 暴露工具".into());
        }
        let install_path = packages_dir(&app)?
            .join(&args.plugin_id)
            .join(&row.current_version);
        let manifest = ui::read_manifest(&install_path)?;
        if manifest.contributes.mcp_tools.is_empty() {
            return Err("plugin does not declare MCP tools".into());
        }
        manifest.mcp_toolset_fingerprint()?
    } else {
        String::new()
    };
    store_plugin_mcp_exposed(&conn, &args.plugin_id, args.exposed, &fingerprint)?;
    drop(conn);
    crate::mcp::notify_plugin_tools_changed(&app);
    Ok(())
}

/// Switch an enabled plugin from `current_version` to its staged `pending_version` after the
/// user has trusted the new package (design §8.4). Stops the old Runtime / UI first.
#[tauri::command]
pub async fn promote_plugin_pending_version(
    app: AppHandle,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    plugin_id: String,
) -> Result<String, String> {
    host.supervisor.stop(&plugin_id).await;
    crate::plugins::windows::close_plugin_windows(&app, &host, &plugin_id);
    for view_instance_id in host.views_for_plugin(&plugin_id) {
        host.destroy_view(&view_instance_id);
    }
    host.release_all_subscriptions_for_plugin(&plugin_id);

    let new_version = {
        let conn = state.db.lock();
        ensure_plugin_tables(&conn)?;
        promote_pending_version(&conn, &plugin_id)?
    };

    host.forget_plugin(&plugin_id);
    refresh_contributions(&app, &state, &host)?;
    crate::mcp::notify_plugin_tools_changed(&app);
    Ok(new_version)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMcpToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
    pub annotations: Option<crate::plugins::manifest::ContributedMcpToolAnnotations>,
    /// Per-tool preference; overall plugin exposure still gates MCP visibility.
    pub enabled: bool,
}

/// The `contributes.mcpTools` a plugin declares (manifest-local names), for the settings UI to
/// show the user before they flip the MCP exposure switch (design §11 "查看工具清单后显式开启").
#[tauri::command]
pub fn list_plugin_mcp_tools(
    app: AppHandle,
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<Vec<PluginMcpToolInfo>, String> {
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let row =
        get_installed_plugin(&conn, &plugin_id)?.ok_or_else(|| "plugin not found".to_string())?;
    let disabled = crate::plugins::trust::get_plugin_mcp_disabled_tools(&conn, &plugin_id)?;
    let install_path = packages_dir(&app)?
        .join(&plugin_id)
        .join(&row.current_version);
    let manifest = ui::read_manifest(&install_path)?;
    Ok(manifest
        .contributes
        .mcp_tools
        .iter()
        .map(|tool| PluginMcpToolInfo {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
            output_schema: tool.output_schema.clone(),
            annotations: tool.annotations.clone(),
            enabled: !disabled.iter().any(|name| name == &tool.name),
        })
        .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingField {
    pub id: String,
    #[serde(rename = "type")]
    pub setting_type: crate::plugins::manifest::SettingFieldType,
    pub title: String,
    pub description: Option<String>,
    pub default: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<crate::plugins::manifest::SettingOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingsBundle {
    pub plugin_id: String,
    pub plugin_name: String,
    pub settings: Vec<PluginSettingField>,
    pub values: Value,
    pub mcp_tool_count: usize,
    pub mcp_exposed: bool,
}

fn read_plugin_manifest(
    app: &AppHandle,
    conn: &rusqlite::Connection,
    plugin_id: &str,
) -> Result<(crate::plugins::trust::InstalledPluginRow, PluginManifest), String> {
    let row =
        get_installed_plugin(conn, plugin_id)?.ok_or_else(|| "plugin not found".to_string())?;
    let install_path = packages_dir(app)?
        .join(plugin_id)
        .join(&row.current_version);
    let manifest = ui::read_manifest(&install_path)?;
    Ok((row, manifest))
}

#[tauri::command]
pub fn get_plugin_settings_bundle(
    app: AppHandle,
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<PluginSettingsBundle, String> {
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let (row, manifest) = read_plugin_manifest(&app, &conn, &plugin_id)?;
    let settings = manifest
        .contributes
        .settings
        .iter()
        .map(|setting| PluginSettingField {
            id: setting.id.clone(),
            setting_type: setting.setting_type,
            title: setting.title.clone(),
            description: setting.description.clone(),
            default: setting.default.clone(),
            options: setting.options.clone(),
            placeholder: setting.placeholder.clone(),
        })
        .collect();
    let values =
        crate::plugins::settings::read_values(&conn, &plugin_id, &manifest.contributes.settings)?;
    let fingerprint = manifest.mcp_toolset_fingerprint().unwrap_or_default();
    let mcp_tool_count = manifest.contributes.mcp_tools.len();
    let mcp_exposed = row.mcp_exposed
        && mcp_tool_count > 0
        && crate::plugins::trust::is_plugin_mcp_exposed(&conn, &plugin_id, &fingerprint);
    Ok(PluginSettingsBundle {
        plugin_id,
        plugin_name: manifest.name,
        settings,
        values: Value::Object(values),
        mcp_tool_count,
        mcp_exposed,
    })
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPluginSettingsValuesArgs {
    pub plugin_id: String,
    pub values: Value,
}

#[tauri::command]
pub fn set_plugin_settings_values(
    app: AppHandle,
    state: State<'_, AppState>,
    args: SetPluginSettingsValuesArgs,
) -> Result<Value, String> {
    let patch = args
        .values
        .as_object()
        .ok_or_else(|| "values must be an object".to_string())?;
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let (_row, manifest) = read_plugin_manifest(&app, &conn, &args.plugin_id)?;
    if manifest.contributes.settings.is_empty() {
        return Err("plugin does not declare settings".into());
    }
    let next = crate::plugins::settings::write_values(
        &conn,
        &args.plugin_id,
        &manifest.contributes.settings,
        patch,
    )?;
    drop(conn);
    crate::plugins::settings::notify_settings_changed(&app, &args.plugin_id, &next);
    Ok(Value::Object(next))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPluginMcpToolEnabledArgs {
    pub plugin_id: String,
    pub tool_name: String,
    pub enabled: bool,
}

/// Per-tool enable preference under the plugin-level MCP exposure master switch.
#[tauri::command]
pub fn set_plugin_mcp_tool_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    args: SetPluginMcpToolEnabledArgs,
) -> Result<(), String> {
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let row = get_installed_plugin(&conn, &args.plugin_id)?
        .ok_or_else(|| "plugin not found".to_string())?;
    if !row.trusted {
        return Err("信任插件后才能配置 MCP 工具".into());
    }
    let install_path = packages_dir(&app)?
        .join(&args.plugin_id)
        .join(&row.current_version);
    let manifest = ui::read_manifest(&install_path)?;
    if !manifest
        .contributes
        .mcp_tools
        .iter()
        .any(|tool| tool.name == args.tool_name)
    {
        return Err(format!(
            "plugin does not declare MCP tool {}",
            args.tool_name
        ));
    }
    crate::plugins::trust::set_plugin_mcp_tool_enabled(
        &conn,
        &args.plugin_id,
        &args.tool_name,
        args.enabled,
    )?;
    drop(conn);
    crate::mcp::notify_plugin_tools_changed(&app);
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustPluginArgs {
    pub plugin_id: String,
    pub version: String,
    pub trusted: bool,
}

#[tauri::command]
pub fn trust_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    args: TrustPluginArgs,
) -> Result<(), String> {
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    set_package_trusted(&conn, &args.plugin_id, &args.version, args.trusted)?;
    if !args.trusted
        && get_installed_plugin(&conn, &args.plugin_id)?
            .is_some_and(|row| row.current_version == args.version)
    {
        store_plugin_mcp_exposed(&conn, &args.plugin_id, false, "")?;
    }
    drop(conn);
    crate::mcp::notify_plugin_tools_changed(&app);
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPluginEnabledArgs {
    pub plugin_id: String,
    pub enabled: bool,
}

/// Enable/disable a plugin (design §6.2). Enabling only registers declarative contributes —
/// it never starts the Runtime. Disabling stops the Runtime, tears down every UI view instance,
/// and clears sessions/subscriptions before the contributions disappear from the registry.
#[tauri::command]
pub async fn set_plugin_enabled_command(
    app: AppHandle,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: SetPluginEnabledArgs,
) -> Result<(), String> {
    let plugin_id = args.plugin_id;
    let enabled = args.enabled;
    {
        let conn = state.db.lock();
        ensure_plugin_tables(&conn)?;

        if enabled {
            let rows = list_installed_plugins(&conn)?;
            let row = rows
                .into_iter()
                .find(|r| r.id == plugin_id)
                .ok_or_else(|| "plugin not found".to_string())?;
            if !row.trusted {
                return Err("启用前请先确认信任该插件包".into());
            }
        }

        set_plugin_enabled(&conn, &plugin_id, enabled)?;
    }

    if !enabled {
        host.supervisor.stop(&plugin_id).await;
        crate::plugins::windows::close_plugin_windows(&app, &host, &plugin_id);
        for view_instance_id in host.views_for_plugin(&plugin_id) {
            host.destroy_view(&view_instance_id);
        }
        host.release_all_subscriptions_for_plugin(&plugin_id);
        host.forget_plugin(&plugin_id);
        let conn = state.db.lock();
        let _ = ui::clear_all_sessions_for_plugin(&conn, &plugin_id);
        let _ = storage::delete_all(&conn, &plugin_id);
    }

    refresh_contributions(&app, &state, &host)?;
    crate::mcp::notify_plugin_tools_changed(&app);

    // Design §4.3: a plugin declaring `activationEvents: ["onStartup"]` (and a `main`) gets its
    // Runtime eagerly started right after it's enabled too, not just at the next Tempo boot.
    if enabled {
        if let Ok(packages_root) = packages_dir(&app) {
            let needs_startup = {
                let conn = state.db.lock();
                crate::plugins::loader::plugins_needing_startup(&conn, &packages_root)
                    .unwrap_or_default()
                    .contains(&plugin_id)
            };
            if needs_startup {
                let host = host.inner().clone();
                let started_plugin_id = plugin_id.clone();
                tokio::spawn(async move {
                    if let Err(error) = host.supervisor.ensure_started(&started_plugin_id).await {
                        tracing::warn!(
                            plugin_id = %started_plugin_id,
                            error = %error,
                            "onStartup plugin activation failed after enable"
                        );
                    }
                });
            }
        }
    }

    Ok(())
}

fn refresh_contributions(
    app: &AppHandle,
    app_state: &AppState,
    host: &Arc<PluginHost>,
) -> Result<Vec<PluginContributionBundle>, String> {
    let conn = app_state.db.lock();
    let bundles =
        with_development_contributions(scan_enabled_contributions(app, host, &conn)?, host);
    drop(conn);
    let _ = app.emit(CONTRIBUTIONS_CHANGED_EVENT, ());
    Ok(bundles)
}

/// Read-only scan used by the frontend sync listener. Must not emit
/// `plugin-contributions-changed`, or list→emit→list loops forever.
#[tauri::command]
pub fn list_plugin_contributions(
    app: AppHandle,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
) -> Result<Vec<PluginContributionBundle>, String> {
    let conn = state.db.lock();
    Ok(with_development_contributions(
        scan_enabled_contributions(&app, &host, &conn)?,
        &host,
    ))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCallCommandArgs {
    pub plugin_id: String,
    pub command_id: String,
    #[serde(default)]
    pub params: Value,
}

/// Lazily activates the plugin Runtime (if needed) and invokes one of its registered
/// commands (design §6.2/§6.3). `command_id` is the manifest-local id, not the runtime id.
#[tauri::command]
pub async fn plugin_call_command(
    app: AppHandle,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: PluginCallCommandArgs,
) -> Result<Value, RpcError> {
    let params = enrich_action_command_input(&app, &state, args.params)?;
    host.supervisor
        .call(
            &args.plugin_id,
            &args.command_id,
            params,
            bridge::DEFAULT_TIMEOUT,
        )
        .await
}

fn enrich_action_command_input(
    app: &AppHandle,
    state: &State<'_, AppState>,
    mut params: Value,
) -> Result<Value, RpcError> {
    if params.get("actionId").and_then(Value::as_str).is_none() {
        return Ok(params);
    }
    let Some(input) = params.get_mut("input").and_then(Value::as_object_mut) else {
        return Ok(params);
    };
    if input.get("kind").and_then(Value::as_str) != Some("image") {
        return Ok(params);
    }
    let entry_id = input
        .get("entryId")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            RpcError::new(bridge::codes::INVALID_REQUEST, "image entryId is required")
        })?;
    let entry = {
        let conn = state.db.lock();
        crate::builtin_plugins::clipboard::db::get_clipboard_entry(&conn, entry_id)
    }
    .ok_or_else(|| RpcError::new(bridge::codes::NOT_FOUND, "clipboard image not found"))?;
    if entry.kind != "image" {
        return Err(RpcError::new(
            bridge::codes::INVALID_REQUEST,
            "clipboard entry is not an image",
        ));
    }
    let path = crate::builtin_plugins::clipboard::images::clipboard_image_path(app, &entry.content)
        .map_err(|message| RpcError::new(bridge::codes::NOT_FOUND, message))?;
    input.insert(
        "filePath".into(),
        Value::String(path.to_string_lossy().into_owned()),
    );
    Ok(params)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginBridgeInvokeArgs {
    pub plugin_id: String,
    #[serde(default)]
    pub view_instance_id: Option<String>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Single entry point for the iframe UI bridge (`PluginAppHost` postMessage -> here ->
/// `plugin_bridge_invoke` -> `bridge::dispatch`). The host — not the payload — decides which
/// plugin/view a call belongs to: `viewInstanceId`, if present, must already be registered to
/// `pluginId` or the call is rejected (design §5.3: never trust self-reported identity).
#[tauri::command]
pub async fn plugin_bridge_invoke(
    app: AppHandle,
    host: State<'_, Arc<PluginHost>>,
    args: PluginBridgeInvokeArgs,
) -> Result<Value, RpcError> {
    let host = host.inner().clone();
    let ctx = match &args.view_instance_id {
        Some(view_instance_id) => {
            let view = host.view(view_instance_id).ok_or_else(|| {
                RpcError::new(bridge::codes::NOT_FOUND, "view instance not found")
            })?;
            if view.plugin_id != args.plugin_id {
                return Err(RpcError::new(
                    bridge::codes::FORBIDDEN,
                    "view instance belongs to another plugin",
                ));
            }
            ConnectionContext::ui(args.plugin_id.clone(), view_instance_id.clone())
        }
        None => ConnectionContext::runtime(args.plugin_id.clone()),
    };

    bridge::dispatch(&app, &host, &ctx, &args.method, args.params).await
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUiPrepareArgs {
    pub plugin_id: String,
    pub app_id: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub session_payload: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUiPrepareResult {
    pub view_instance_id: String,
    pub entry_url: String,
    pub theme: String,
    pub api_version: String,
    pub params: Value,
    pub session: Option<Value>,
}

/// Resolves a plugin app contribution to a loadable `tempo-plugin://` URL and mints a view
/// instance the Host Bridge will authenticate subsequent calls against. Re-verifies the full
/// package hash before any UI resource is served (design §8.4 step 9 / §15-1).
#[tauri::command]
pub fn plugin_ui_prepare(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: PluginUiPrepareArgs,
) -> Result<PluginUiPrepareResult, String> {
    if !is_valid_local_id(&args.app_id) {
        return Err(format!("invalid app id: {}", args.app_id));
    }

    if let Some(development) = host.development_plugin(&args.plugin_id) {
        let app_contrib = development
            .manifest
            .contributes
            .apps
            .iter()
            .find(|candidate| candidate.id == args.app_id)
            .ok_or_else(|| format!("plugin does not contribute app {}", args.app_id))?;
        let entry_url = match development.ui_source.as_ref() {
            Some(crate::plugins::host::DevelopmentUiSource::Url(url)) => url.clone(),
            Some(crate::plugins::host::DevelopmentUiSource::Static(_)) => {
                let plugin_hash = ui::plugin_hash_of(&development.manifest.id);
                ui::plugin_entry_url(&plugin_hash, &app_contrib.entry)
            }
            None => return Err("开发插件没有配置 UI 连接".into()),
        };
        let conn = state.db.lock();
        let theme = crate::db::get_setting(&conn, "theme", "system");
        let owner_window_label = host
            .plugin_window(window.label())
            .filter(|owner| {
                owner.plugin_id == development.manifest.id && owner.app_local_id == args.app_id
            })
            .map(|_| window.label());
        let view_instance_id =
            host.create_view(&development.manifest.id, &args.app_id, owner_window_label);
        return Ok(PluginUiPrepareResult {
            view_instance_id,
            entry_url,
            theme,
            api_version: bridge::HOST_API_VERSION.to_string(),
            params: args.params,
            session: args.session_payload,
        });
    }

    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let row = get_installed_plugin(&conn, &args.plugin_id)?
        .ok_or_else(|| "plugin not found".to_string())?;
    if !row.enabled || !row.trusted {
        return Err("plugin is not enabled".into());
    }

    let install_path = packages_dir(&app)?.join(&row.id).join(&row.current_version);
    let manifest = ui::read_manifest(&install_path)?;
    let app_contrib = manifest
        .contributes
        .apps
        .iter()
        .find(|a| a.id == args.app_id)
        .ok_or_else(|| format!("plugin does not contribute app {}", args.app_id))?;

    let package_hash = row.package_hash.clone().unwrap_or_default();
    if package_hash.is_empty() {
        return Err("plugin package hash is unknown; re-import required".into());
    }
    crate::plugins::package::verify_package_hash(&install_path, &package_hash)?;

    let plugin_hash = ui::plugin_hash_of(&row.id);
    let entry_url = ui::plugin_entry_url(&plugin_hash, &app_contrib.entry);
    let theme = crate::db::get_setting(&conn, "theme", "system");

    let session = if let Some(payload) = args.session_payload {
        Some(payload)
    } else {
        ui::load_session(
            &conn,
            &row.id,
            &args.app_id,
            &row.current_version,
            app_contrib.session_version.unwrap_or(1),
        )?
        .map(|envelope| envelope.payload)
    };

    let owner_window_label = host
        .plugin_window(window.label())
        .filter(|owner| owner.plugin_id == row.id && owner.app_local_id == args.app_id)
        .map(|_| window.label());
    let view_instance_id = host.create_view(&row.id, &args.app_id, owner_window_label);

    Ok(PluginUiPrepareResult {
        view_instance_id,
        entry_url,
        theme,
        api_version: bridge::HOST_API_VERSION.to_string(),
        params: args.params,
        session,
    })
}

#[tauri::command]
pub fn plugin_ui_dispose(
    host: State<'_, Arc<PluginHost>>,
    view_instance_id: String,
) -> Result<(), String> {
    host.destroy_view(&view_instance_id);
    Ok(())
}

/// Asks the UI (via cached `session.push` state) for its latest session payload with a 300ms
/// budget, then persists it (design §5.5). Returns quietly if nothing arrives in time.
#[tauri::command]
pub async fn plugin_ui_serialize_session(
    app: AppHandle,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    view_instance_id: String,
) -> Result<(), String> {
    let Some(view) = host.view(&view_instance_id) else {
        return Ok(());
    };

    let mut payload = host.take_cached_session_payload(&view_instance_id);
    if payload.is_none() {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
        while tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(30)).await;
            payload = host.take_cached_session_payload(&view_instance_id);
            if payload.is_some() {
                break;
            }
        }
    }
    let Some(payload) = payload else {
        return Ok(());
    };

    if host.development_plugin(&view.plugin_id).is_some() {
        return Ok(());
    }

    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let Some(row) = get_installed_plugin(&conn, &view.plugin_id)? else {
        return Ok(());
    };

    let session_version = packages_dir(&app)
        .ok()
        .map(|root| root.join(&view.plugin_id).join(&row.current_version))
        .and_then(|install_path| ui::read_manifest(&install_path).ok())
        .and_then(|manifest| {
            manifest
                .contributes
                .apps
                .iter()
                .find(|a| a.id == view.app_local_id)
                .and_then(|a| a.session_version)
        })
        .unwrap_or(1);

    let _ = ui::save_session(
        &conn,
        &view.plugin_id,
        &view.app_local_id,
        &row.current_version,
        session_version,
        &payload,
    );
    Ok(())
}

#[tauri::command]
pub fn plugin_open_data_dir(app: AppHandle, plugin_id: String) -> Result<(), String> {
    let dir = plugin_data_dir(&app, &plugin_id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create plugin data dir: {e}"))?;
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(dir.display().to_string(), None::<String>)
        .map_err(|e| format!("open plugin data dir: {e}"))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUninstallArgs {
    pub plugin_id: String,
    #[serde(default)]
    pub delete_data: bool,
}

#[tauri::command]
pub async fn plugin_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: PluginUninstallArgs,
) -> Result<(), String> {
    let plugin_id = args.plugin_id;
    let delete_data = args.delete_data;
    host.supervisor.stop(&plugin_id).await;
    crate::plugins::windows::close_plugin_windows(&app, &host, &plugin_id);
    for view_instance_id in host.views_for_plugin(&plugin_id) {
        host.destroy_view(&view_instance_id);
    }
    host.release_all_subscriptions_for_plugin(&plugin_id);
    host.forget_plugin(&plugin_id);

    let row = {
        let conn = state.db.lock();
        ensure_plugin_tables(&conn)?;
        let row = get_installed_plugin(&conn, &plugin_id)?;
        let _ = ui::clear_all_sessions_for_plugin(&conn, &plugin_id);
        delete_plugin_records(&conn, &plugin_id)?;
        row
    };

    if let Some(row) = row {
        let install_path = packages_dir(&app)?
            .join(&plugin_id)
            .join(&row.current_version);
        if install_path.exists() {
            let trash = trash_dir(&app)?;
            std::fs::create_dir_all(&trash).map_err(|e| format!("create trash dir: {e}"))?;
            let dest = trash.join(format!(
                "{plugin_id}-{}-{}",
                row.current_version,
                ui::plugin_hash_of(&plugin_id)
            ));
            if std::fs::rename(&install_path, &dest).is_err() {
                // Cross-device or locked file fallback: best-effort recursive delete.
                let _ = std::fs::remove_dir_all(&install_path);
            }
        }
    }

    if delete_data {
        let data_dir = plugin_data_dir(&app, &plugin_id)?;
        if data_dir.exists() {
            let _ = std::fs::remove_dir_all(&data_dir);
        }
        let conn = state.db.lock();
        let _ = storage::delete_all(&conn, &plugin_id);
    }

    refresh_contributions(&app, &state, &host)?;
    crate::mcp::notify_plugin_tools_changed(&app);
    Ok(())
}

/// Runtime id helper exposed for completeness/tests of the loader naming scheme.
#[allow(dead_code)]
pub fn contribution_runtime_id(plugin_id: &str, local_id: &str) -> String {
    runtime_id(plugin_id, local_id)
}
