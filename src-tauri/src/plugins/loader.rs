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

use super::host::{DevelopmentPlugin, DevelopmentUiSource, PluginHost, PluginRegistryEntry};
use super::ids::runtime_id;
use super::manifest::{ActionInputKind, AppRect, PluginManifest, PluginWindowMode};
use super::paths::packages_dir;
use super::trust::list_installed_plugins;
use super::ui::{plugin_entry_url, plugin_hash_of, plugin_icon_url};

/// Prefer the contribution's own icon; otherwise reuse the first app/action icon in the package
/// (same fallback as the settings plugin list) so launcher tiles stay visible after connect.
fn contribution_icon_url(
    plugin_hash: &str,
    icon: Option<&String>,
    manifest: &PluginManifest,
) -> Option<String> {
    icon.or_else(|| {
        manifest
            .contributes
            .apps
            .iter()
            .find_map(|app| app.icon.as_ref())
    })
    .or_else(|| {
        manifest
            .contributes
            .actions
            .iter()
            .find_map(|action| action.icon.as_ref())
    })
    .map(|path| plugin_icon_url(plugin_hash, path))
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
    pub window_mode: PluginWindowMode,
    pub rect: AppRect,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActionContribution {
    pub id: String,
    pub local_id: String,
    pub name: String,
    pub keywords: Vec<String>,
    pub icon_url: Option<String>,
    /// Runtime id of the app this action opens: `{pluginId}/{appLocalId}`.
    pub app_id: Option<String>,
    /// Runtime id of the command this action invokes: `{pluginId}/{commandLocalId}`.
    pub command_id: Option<String>,
    pub accepts: Vec<ActionInputKind>,
    pub title_template: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributionBundle {
    pub plugin_id: String,
    pub version: String,
    pub package_hash: String,
    pub development: bool,
    pub development_ui_source: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub requires_node_runtime: bool,
    pub apps: Vec<PluginAppContribution>,
    pub actions: Vec<PluginActionContribution>,
}

pub fn development_contribution_bundle(
    development: &DevelopmentPlugin,
) -> PluginContributionBundle {
    let manifest = &development.manifest;
    let plugin_id = &manifest.id;
    let plugin_hash = plugin_hash_of(plugin_id);
    let apps = manifest
        .contributes
        .apps
        .iter()
        .map(|contrib| PluginAppContribution {
            id: runtime_id(plugin_id, &contrib.id),
            local_id: contrib.id.clone(),
            name: contrib.name.clone(),
            keywords: contrib.keywords.clone(),
            icon_url: contribution_icon_url(
                &plugin_hash,
                contrib.icon.as_ref(),
                manifest,
            ),
            entry_path: match &development.ui_source {
                Some(DevelopmentUiSource::Url(url)) => url.clone(),
                _ => plugin_entry_url(&plugin_hash, &contrib.entry),
            },
            window_mode: contrib.window_mode.clone(),
            rect: contrib.rect.clone(),
        })
        .collect();
    let actions = manifest
        .contributes
        .actions
        .iter()
        .map(|contrib| PluginActionContribution {
            id: runtime_id(plugin_id, &contrib.id),
            local_id: contrib.id.clone(),
            name: contrib.name.clone(),
            keywords: contrib.keywords.clone(),
            icon_url: contribution_icon_url(
                &plugin_hash,
                contrib.icon.as_ref(),
                manifest,
            ),
            app_id: contrib
                .app
                .as_ref()
                .map(|app_id| runtime_id(plugin_id, app_id)),
            command_id: contrib
                .command
                .as_ref()
                .map(|command_id| runtime_id(plugin_id, command_id)),
            accepts: contrib.accepted_inputs(),
            title_template: contrib.title_template.clone(),
        })
        .collect();

    PluginContributionBundle {
        plugin_id: plugin_id.clone(),
        version: manifest.version.clone(),
        package_hash: "development".into(),
        development: true,
        development_ui_source: development.ui_source.as_ref().map(|source| match source {
            DevelopmentUiSource::Url(_) => "url".into(),
            DevelopmentUiSource::Static(_) => "static".into(),
        }),
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        requires_node_runtime: development.runtime_entry.is_some(),
        apps,
        actions,
    }
}

pub fn with_development_contributions(
    mut installed: Vec<PluginContributionBundle>,
    host: &PluginHost,
) -> Vec<PluginContributionBundle> {
    for development in host.development_plugins() {
        installed.retain(|bundle| bundle.plugin_id != development.manifest.id);
        installed.push(development_contribution_bundle(&development));
    }
    installed
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
                icon_url: contribution_icon_url(
                    &plugin_hash,
                    contrib.icon.as_ref(),
                    &manifest,
                ),
                entry_path: plugin_entry_url(&plugin_hash, &contrib.entry),
                window_mode: contrib.window_mode.clone(),
                rect: contrib.rect.clone(),
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
                icon_url: contribution_icon_url(
                    &plugin_hash,
                    contrib.icon.as_ref(),
                    &manifest,
                ),
                app_id: contrib
                    .app
                    .as_ref()
                    .map(|app_id| runtime_id(&row.id, app_id)),
                command_id: contrib
                    .command
                    .as_ref()
                    .map(|command_id| runtime_id(&row.id, command_id)),
                accepts: contrib.accepted_inputs(),
                title_template: contrib.title_template.clone(),
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
            development: false,
            development_ui_source: None,
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
pub fn plugins_needing_startup(
    conn: &Connection,
    packages_root: &Path,
) -> Result<Vec<String>, String> {
    let rows = list_installed_plugins(conn)?;
    let mut out = Vec::new();
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
        if manifest.main.is_some()
            && manifest
                .activation_events
                .iter()
                .any(|event| event == "onStartup")
        {
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
        assert_eq!(
            runtime_id("com.example.hello", "main"),
            "com.example.hello/main"
        );
        assert_ne!(
            runtime_id("com.example.a", "main"),
            runtime_id("com.example.b", "main")
        );
    }
}
