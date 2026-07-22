use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::db::AppState;
use crate::plugins::package::{import_directory, InstalledPackage};
use crate::plugins::runtime::{
    get_plugin_runtime_status, install_plugin_runtime, uninstall_plugin_runtime, PluginRuntimeStatus,
};
use crate::plugins::trust::{
    ensure_plugin_tables, list_installed_plugins, record_installed_version, set_package_trusted,
    set_plugin_enabled, InstalledPluginRow,
};
use crate::plugins::manifest::PluginManifest;
use crate::plugins::paths::packages_dir;

#[tauri::command]
pub fn plugin_runtime_status(app: AppHandle) -> Result<PluginRuntimeStatus, String> {
    get_plugin_runtime_status(&app)
}

#[tauri::command]
pub async fn plugin_runtime_install(app: AppHandle) -> Result<PluginRuntimeStatus, String> {
    install_plugin_runtime(&app).await
}

#[tauri::command]
pub fn plugin_runtime_uninstall(app: AppHandle) -> Result<PluginRuntimeStatus, String> {
    uninstall_plugin_runtime(&app)
}

#[tauri::command]
pub fn import_local_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<InstalledPackage, String> {
    let source = PathBuf::from(&path);
    let installed = import_directory(&app, &source)?;
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
pub fn list_plugins(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<InstalledPluginRow>, String> {
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let mut rows = list_installed_plugins(&conn)?;
    let packages = packages_dir(&app).ok();
    for row in &mut rows {
        if let Some(root) = &packages {
            let manifest_path = root
                .join(&row.id)
                .join(&row.current_version)
                .join("manifest.json");
            if let Ok(raw) = std::fs::read_to_string(manifest_path) {
                if let Ok(manifest) = PluginManifest::parse_str(&raw) {
                    row.requires_node_runtime = manifest.requires_node_runtime();
                }
            }
        }
    }
    Ok(rows)
}

#[tauri::command]
pub fn trust_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
    version: String,
    trusted: bool,
) -> Result<(), String> {
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    set_package_trusted(&conn, &plugin_id, &version, trusted)
}

#[tauri::command]
pub fn set_plugin_enabled_command(
    state: State<'_, AppState>,
    plugin_id: String,
    enabled: bool,
) -> Result<(), String> {
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;

    if enabled {
        // Enabling requires trust on current version.
        let rows = list_installed_plugins(&conn)?;
        let row = rows
            .into_iter()
            .find(|r| r.id == plugin_id)
            .ok_or_else(|| "plugin not found".to_string())?;
        if !row.trusted {
            return Err("启用前请先确认信任该插件包".into());
        }
        if row.requires_node_runtime {
            // Soft check — detailed gate happens at activation time.
        }
    }

    set_plugin_enabled(&conn, &plugin_id, enabled)
}
