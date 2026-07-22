//! App-data paths for plugins and the on-demand Node runtime.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

pub fn plugins_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    Ok(dir.join("plugins"))
}

pub fn packages_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(plugins_root(app)?.join("packages"))
}

pub fn plugin_data_dir(app: &AppHandle, plugin_id: &str) -> Result<PathBuf, String> {
    Ok(plugins_root(app)?.join("data").join(plugin_id))
}

pub fn staging_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(plugins_root(app)?.join("_staging"))
}

pub fn trash_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(plugins_root(app)?.join("_trash"))
}

pub fn plugin_runtime_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    Ok(dir.join("plugin-runtime"))
}

pub fn node_runtime_dir(app: &AppHandle, version: &str) -> Result<PathBuf, String> {
    Ok(plugin_runtime_root(app)?.join("node").join(version))
}

pub fn runtime_manifest_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(plugin_runtime_root(app)?.join("manifest.json"))
}

pub fn ensure_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("create {}: {e}", path.display()))
}
