//! App-data paths for plugins and the on-demand Node runtime.
//!
//! All durable plugin files live under the unified Tempo storage root
//! (`%APPDATA%/Tempo` on Windows by default).

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::db::{current_storage_dir, default_storage_dir};

fn storage_root(app: &AppHandle) -> Result<PathBuf, String> {
    current_storage_dir(app).or_else(|_| default_storage_dir(app))
}

pub fn plugins_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(storage_root(app)?.join("plugins"))
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
    Ok(storage_root(app)?.join("plugin-runtime"))
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

/// Directory holding short-lived Unix domain socket / Windows named pipe endpoints used for the
/// per-plugin Runtime IPC handshake. Kept out of app_data to avoid long paths on some platforms.
pub fn plugin_ipc_dir() -> PathBuf {
    std::env::temp_dir().join("tempo-plugin-ipc")
}
