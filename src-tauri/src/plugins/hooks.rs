//! Phase 2 host event hooks (design §6.1 "Hooks", §13 Phase 2).
//!
//! `dispatch_event` scans every enabled+trusted plugin's manifest for `contributes.hooks`
//! entries matching `event`, and fires each matching command at its plugin's Runtime (lazily
//! activating it if needed) without waiting for the result — hooks are host-initiated
//! notifications, not RPC calls, so a slow or failing plugin hook must never block the caller
//! (e.g. the clipboard watcher thread) or delay another plugin's hook.

use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Manager};

use super::bridge::DEFAULT_TIMEOUT;
use super::host::PluginHost;
use super::manifest::PluginManifest;
use super::paths::packages_dir;
use super::trust::{ensure_plugin_tables, list_installed_plugins};

/// Fire `event` (with `payload`) at every enabled+trusted plugin declaring a matching
/// `contributes.hooks` entry. `payload` should already carry a `schemaVersion` (design §6.1).
/// Fire-and-forget: each matching command runs on its own spawned task and errors are only
/// logged, never returned to the caller.
pub fn dispatch_event(app: &AppHandle, host: &Arc<PluginHost>, event: &str, payload: Value) {
    let Some(app_state) = app.try_state::<crate::db::AppState>() else {
        return;
    };
    let packages_root = match packages_dir(app) {
        Ok(root) => root,
        Err(error) => {
            tracing::warn!(error = %error, event = %event, "hooks: failed to resolve packages dir");
            return;
        }
    };

    let rows = {
        let conn = app_state.db.lock();
        if let Err(error) = ensure_plugin_tables(&conn) {
            tracing::warn!(error = %error, "hooks: failed to prepare plugin tables");
            return;
        }
        match list_installed_plugins(&conn) {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(error = %error, "hooks: failed to list installed plugins");
                return;
            }
        }
    };

    for row in rows {
        if !row.enabled || !row.trusted {
            continue;
        }
        let manifest_path = packages_root
            .join(&row.id)
            .join(&row.current_version)
            .join("manifest.json");
        let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = PluginManifest::parse_str(&raw) else {
            continue;
        };

        for hook in &manifest.contributes.hooks {
            if hook.event != event {
                continue;
            }
            let plugin_id = row.id.clone();
            let command_id = hook.command.clone();
            let host = host.clone();
            let payload = payload.clone();
            let event = event.to_string();
            // `dispatch_event` may be called from a plain OS thread (e.g. the clipboard
            // watcher), not necessarily inside a Tokio task — use the app-wide async runtime
            // handle rather than `tokio::spawn`, which requires an ambient runtime context.
            tauri::async_runtime::spawn(async move {
                if let Err(error) = host
                    .supervisor
                    .call(&plugin_id, &command_id, payload, DEFAULT_TIMEOUT)
                    .await
                {
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        command = %command_id,
                        event = %event,
                        error = %error,
                        "plugin hook invocation failed"
                    );
                }
            });
        }
    }
}
