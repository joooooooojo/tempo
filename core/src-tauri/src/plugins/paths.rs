//! App-data paths for plugins and the on-demand Deno runtime.
//!
//! All durable plugin files live under the unified Tempo storage root
//! (`%APPDATA%/Tempo` on Windows by default).

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::db::{current_storage_dir, default_storage_dir};
use crate::plugins::ids::is_valid_plugin_id;

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
    if !is_valid_plugin_id(plugin_id) {
        return Err("invalid plugin id".into());
    }
    Ok(plugins_root(app)?.join("data").join(plugin_id))
}

pub fn plugin_dev_data_dir(app: &AppHandle, project_id: &str) -> Result<PathBuf, String> {
    Ok(plugins_root(app)?.join("dev-data").join(project_id))
}

pub fn active_plugin_data_dir(
    app: &AppHandle,
    host: &super::host::PluginHost,
    plugin_id: &str,
) -> Result<PathBuf, String> {
    if let Some(development) = host
        .development_plugin(plugin_id)
        .filter(|entry| !entry.use_production_data)
    {
        plugin_dev_data_dir(app, &development.project_id)
    } else {
        plugin_data_dir(app, plugin_id)
    }
}

pub fn staging_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(plugins_root(app)?.join("_staging"))
}

pub fn trash_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(plugins_root(app)?.join("_trash"))
}

pub fn repositories_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(plugins_root(app)?.join("_repositories"))
}

pub fn repository_cache_dir(app: &AppHandle, repository_id: &str) -> Result<PathBuf, String> {
    Ok(repositories_dir(app)?.join(repository_id))
}

pub fn plugin_runtime_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(storage_root(app)?.join("plugin-runtime"))
}

pub fn deno_runtime_dir(app: &AppHandle, version: &str) -> Result<PathBuf, String> {
    Ok(plugin_runtime_root(app)?.join("deno").join(version))
}

pub fn runtime_manifest_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(plugin_runtime_root(app)?.join("deno-manifest.json"))
}

pub fn ensure_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("create {}: {e}", path.display()))
}
