//! Declarative plugin contribution loader (design §6.2, §9).
//!
//! Scanning/enabling only reads `manifest.json` and registers declarative contributes; it
//! never executes plugin code. Runtime activation is always lazy (first `runtime.*`/command
//! call, or an explicit `onStartup` activation event) and is handled by `supervisor.rs`.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;
use tauri::AppHandle;

use super::host::{PluginHost, PluginRegistryEntry};
use super::ids::runtime_id;
use super::manifest::PluginManifest;
use super::paths::packages_dir;
use super::trust::list_installed_plugins;
use super::ui::{plugin_entry_url, plugin_hash_of, plugin_icon_url};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultSizeDto {
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAppContribution {
    /// Runtime id: `{pluginId}/{localId}`.
    pub id: String,
    pub local_id: String,
    pub name: String,
    pub keywords: Vec<String>,
    pub icon_url: Option<String>,
    /// Resolved `tempo-plugin://` URL for the app's UI entry document.
    pub entry_path: String,
    pub default_size: Option<DefaultSizeDto>,
    pub persist_session: bool,
    pub session_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActionContribution {
    pub id: String,
    pub local_id: String,
    pub name: String,
    pub keywords: Vec<String>,
    pub icon_url: Option<String>,
    /// Runtime id of the command this action invokes: `{pluginId}/{commandLocalId}`.
    pub command_id: String,
    pub title_template: Option<String>,
    pub requires_query: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributionBundle {
    pub plugin_id: String,
    pub version: String,
    pub package_hash: String,
    pub name: String,
    pub description: Option<String>,
    pub requires_node_runtime: bool,
    pub apps: Vec<PluginAppContribution>,
    pub actions: Vec<PluginActionContribution>,
}

/// Scan every enabled + trusted plugin on disk, refresh the host's protocol/registry maps, and
/// return declarative contribution bundles for the frontend app/action registries. A plugin
/// that fails to parse is skipped (and logged) rather than failing the whole scan — one broken
/// plugin must not take down every other plugin's contributions (design §15 acceptance #8).
pub fn scan_enabled_contributions(
    app: &AppHandle,
    host: &PluginHost,
    conn: &Connection,
) -> Result<Vec<PluginContributionBundle>, String> {
    let rows = list_installed_plugins(conn)?;
    let packages_root = packages_dir(app)?;

    let mut bundles = Vec::new();
    let mut registry: HashMap<String, PluginRegistryEntry> = HashMap::new();
    let mut hash_index: HashMap<String, String> = HashMap::new();

    for row in rows {
        if !row.enabled || !row.trusted {
            continue;
        }
        let install_path = packages_root.join(&row.id).join(&row.current_version);
        let manifest_path = install_path.join("manifest.json");
        let raw = match std::fs::read_to_string(&manifest_path) {
            Ok(raw) => raw,
            Err(error) => {
                tracing::warn!(plugin_id = %row.id, error = %error, "skip plugin: manifest unreadable");
                continue;
            }
        };
        let manifest = match PluginManifest::parse_str(&raw) {
            Ok(manifest) => manifest,
            Err(error) => {
                tracing::warn!(plugin_id = %row.id, error = %error, "skip plugin: invalid manifest");
                continue;
            }
        };

        let package_hash = row.package_hash.clone().unwrap_or_default();
        let plugin_hash = plugin_hash_of(&row.id);

        let apps: Vec<PluginAppContribution> = manifest
            .contributes
            .apps
            .iter()
            .map(|contrib| PluginAppContribution {
                id: runtime_id(&row.id, &contrib.id),
                local_id: contrib.id.clone(),
                name: contrib.name.clone(),
                keywords: contrib.keywords.clone(),
                icon_url: contrib
                    .icon
                    .as_ref()
                    .map(|icon| plugin_icon_url(&plugin_hash, icon)),
                entry_path: plugin_entry_url(&plugin_hash, &contrib.entry),
                default_size: contrib.default_size.as_ref().map(|size| DefaultSizeDto {
                    width: size.width,
                    height: size.height,
                }),
                persist_session: contrib.persist_session,
                session_version: contrib.session_version,
            })
            .collect();

        let actions: Vec<PluginActionContribution> = manifest
            .contributes
            .actions
            .iter()
            .map(|contrib| PluginActionContribution {
                id: runtime_id(&row.id, &contrib.id),
                local_id: contrib.id.clone(),
                name: contrib.name.clone(),
                keywords: contrib.keywords.clone(),
                icon_url: contrib
                    .icon
                    .as_ref()
                    .map(|icon| plugin_icon_url(&plugin_hash, icon)),
                command_id: runtime_id(&row.id, &contrib.command),
                title_template: contrib.title_template.clone(),
                requires_query: contrib.requires_query,
            })
            .collect();

        registry.insert(
            row.id.clone(),
            PluginRegistryEntry {
                plugin_id: row.id.clone(),
                version: row.current_version.clone(),
                package_hash: package_hash.clone(),
                install_path: install_path.clone(),
                name: manifest.name.clone(),
                requires_node_runtime: manifest.requires_node_runtime(),
                main: manifest.main.clone(),
            },
        );
        hash_index.insert(plugin_hash, row.id.clone());

        bundles.push(PluginContributionBundle {
            plugin_id: row.id.clone(),
            version: row.current_version.clone(),
            package_hash,
            name: manifest.name.clone(),
            description: manifest.description.clone(),
            requires_node_runtime: manifest.requires_node_runtime(),
            apps,
            actions,
        });
    }

    host.replace_registry(registry, hash_index);
    Ok(bundles)
}

/// Plugin ids that should have their Runtime eagerly started (design §4.3: `activationEvents`
/// only accepts `onStartup`, and only for packages that declare a `main`). Callers are
/// responsible for actually calling `supervisor.ensure_started` for each id — this function
/// only reads `manifest.json` files and never touches the Supervisor or executes plugin code.
/// Used both on boot (after [`scan_enabled_contributions`]) and right after a plugin is enabled.
pub fn plugins_needing_startup(conn: &Connection, packages_root: &Path) -> Result<Vec<String>, String> {
    let rows = list_installed_plugins(conn)?;
    let mut out = Vec::new();
    for row in rows {
        if !row.enabled || !row.trusted {
            continue;
        }
        let manifest_path = packages_root.join(&row.id).join(&row.current_version).join("manifest.json");
        let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = PluginManifest::parse_str(&raw) else {
            continue;
        };
        if manifest.main.is_some() && manifest.activation_events.iter().any(|event| event == "onStartup") {
            out.push(row.id);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::ids::runtime_id;

    #[test]
    fn runtime_ids_are_namespaced_by_plugin() {
        assert_eq!(runtime_id("com.example.hello", "main"), "com.example.hello/main");
        assert_ne!(
            runtime_id("com.example.a", "main"),
            runtime_id("com.example.b", "main")
        );
    }
}
