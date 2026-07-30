//! Platform event broadcasts for plugin Runtime instances and open plugin pages.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::host::PluginHost;
use super::trust::{ensure_plugin_tables, list_installed_plugins};

/// Broadcast a platform event to eligible plugins without activating stopped Runtimes.
///
/// Production recipients must be enabled and trusted. Connected development plugins are
/// included independently of the installed-plugin table. Each plugin id receives at most one
/// Runtime event and one UI event.
pub fn broadcast(app: &AppHandle, host: &Arc<PluginHost>, event: &str, payload: Value) {
    let mut plugin_ids = host
        .development_plugins()
        .into_iter()
        .map(|entry| entry.manifest.id)
        .collect::<HashSet<_>>();

    if let Some(app_state) = app.try_state::<crate::db::AppState>() {
        let conn = app_state.db.lock();
        match ensure_plugin_tables(&conn).and_then(|_| list_installed_plugins(&conn)) {
            Ok(rows) => {
                plugin_ids.extend(
                    rows.into_iter()
                        .filter(|row| row.enabled && row.trusted)
                        .map(|row| row.id),
                );
            }
            Err(error) => {
                tracing::warn!(error = %error, event = %event, "failed to list host event recipients");
            }
        }
    }

    for plugin_id in plugin_ids {
        let _ = host
            .supervisor
            .emit_event(&plugin_id, event, payload.clone());

        if !host.views_for_plugin(&plugin_id).is_empty() {
            let _ = app.emit(
                "plugin-runtime-event",
                json!({
                    "pluginId": plugin_id,
                    "source": "platform",
                    "event": event,
                    "payload": payload.clone(),
                }),
            );
        }
    }
}
