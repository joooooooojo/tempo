//! Host-managed contributed settings (`contributes.settings`).
//!
//! Values live in plugin private storage under the reserved key `__tempo/settings`.
//! Plugins read them with `storage.plugin.get("__tempo/settings")`.

use rusqlite::Connection;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager};

use super::manifest::ContributedSetting;
use super::storage;

pub const TEMPO_SETTINGS_KEY: &str = "__tempo/settings";
pub const SETTINGS_CHANGED_EVENT: &str = "plugin-settings-changed";

pub fn defaults_object(settings: &[ContributedSetting]) -> Map<String, Value> {
    let mut out = Map::new();
    for setting in settings {
        out.insert(setting.id.clone(), setting.default.clone());
    }
    out
}

pub fn read_values(
    conn: &Connection,
    plugin_id: &str,
    settings: &[ContributedSetting],
) -> Result<Map<String, Value>, String> {
    let mut merged = defaults_object(settings);
    let Some(stored) = storage::get(conn, plugin_id, TEMPO_SETTINGS_KEY)? else {
        return Ok(merged);
    };
    let Some(object) = stored.as_object() else {
        return Ok(merged);
    };
    for setting in settings {
        if let Some(value) = object.get(&setting.id) {
            if let Ok(coerced) = setting.coerce_value(value) {
                merged.insert(setting.id.clone(), coerced);
            }
        }
    }
    Ok(merged)
}

pub fn write_values(
    conn: &Connection,
    plugin_id: &str,
    settings: &[ContributedSetting],
    patch: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let mut next = read_values(conn, plugin_id, settings)?;
    let known: std::collections::HashSet<&str> =
        settings.iter().map(|setting| setting.id.as_str()).collect();
    for (key, value) in patch {
        if !known.contains(key.as_str()) {
            return Err(format!("unknown setting id: {key}"));
        }
        let setting = settings
            .iter()
            .find(|item| item.id == *key)
            .expect("known setting");
        let coerced = setting.coerce_value(value)?;
        next.insert(key.clone(), coerced);
    }
    storage::set(conn, plugin_id, TEMPO_SETTINGS_KEY, &Value::Object(next.clone()))?;
    Ok(next)
}

pub fn notify_settings_changed(app: &AppHandle, plugin_id: &str, values: &Map<String, Value>) {
    let payload = json!({
        "pluginId": plugin_id,
        "values": values,
    });
    let _ = app.emit(SETTINGS_CHANGED_EVENT, &payload);
    // Deliver to open plugin UI iframes (PluginAppHost listens for plugin-runtime-event).
    let _ = app.emit(
        "plugin-runtime-event",
        json!({
            "pluginId": plugin_id,
            "event": "settings.changed",
            "payload": { "values": values },
        }),
    );
    // Deliver to a running Runtime process when present.
    if let Some(host) = app.try_state::<std::sync::Arc<crate::plugins::host::PluginHost>>() {
        let _ = host.supervisor.emit_event(
            plugin_id,
            "settings.changed",
            json!({ "values": values }),
        );
    }
}
