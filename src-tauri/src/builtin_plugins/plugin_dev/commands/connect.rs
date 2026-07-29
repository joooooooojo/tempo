use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use crate::db::AppState;
use crate::plugins::host::{generate_id, DevelopmentPlugin, DevelopmentUiSource, PluginHost};
use crate::plugins::manifest::PluginManifest;

use super::paths::*;
use super::probe::validate_loopback_url;
use super::projects::{
    connection_status, ensure_plugin_dev_tables, preferences_for,
    project_root,
};
use super::types::*;

pub(super) const UI_RELOAD_EVENT: &str = "plugin-dev://ui-reload";
pub(super) const CONTRIBUTIONS_CHANGED_EVENT: &str = "plugin-contributions-changed";

pub(super) fn resolve_static_source(root: &Path, configured: Option<&str>) -> Result<PathBuf, String> {
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

pub(super) fn resolve_runtime_entry(
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

pub(super) fn runtime_file_signature(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some((metadata.len(), metadata.modified().ok()?))
}

pub(super) fn spawn_runtime_entry_watcher(
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

