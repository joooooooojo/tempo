//! `PluginHost`: managed Tauri state bundling the Supervisor, the UI view-instance registry,
//! the `pluginHash <-> pluginId` index used by the `tempo-plugin://` protocol, subscriptions,
//! and per-plugin concurrency bookkeeping for the Host Bridge (design §3.1, §7).

use std::collections::HashMap;
use std::path::PathBuf;

use parking_lot::Mutex;
use serde_json::Value;
use tauri::AppHandle;

use super::manifest::PluginManifest;
use super::supervisor::Supervisor;

#[derive(Debug, Clone)]
pub enum DevelopmentUiSource {
    Url(String),
    Static(PathBuf),
}

#[derive(Debug, Clone)]
pub struct DevelopmentPlugin {
    pub session_id: String,
    pub project_id: String,
    pub root_path: PathBuf,
    pub manifest: PluginManifest,
    pub ui_source: Option<DevelopmentUiSource>,
    pub runtime_entry: Option<PathBuf>,
    pub receive_real_hooks: bool,
    pub use_production_data: bool,
}

#[derive(Debug, Clone)]
pub struct PluginRegistryEntry {
    pub plugin_id: String,
    /// Cached for upcoming host queries / settings surfaces.
    #[allow(dead_code)]
    pub version: String,
    #[allow(dead_code)]
    pub package_hash: String,
    pub install_path: PathBuf,
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub requires_node_runtime: bool,
    #[allow(dead_code)]
    pub main: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ViewInstance {
    pub plugin_id: String,
    pub app_local_id: String,
    pub owner_window_label: Option<String>,
    pub last_session_payload: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct PluginWindowInstance {
    pub plugin_id: String,
    pub app_local_id: String,
    pub params: Value,
}

#[derive(Debug, Clone)]
struct SubscriptionEntry {
    plugin_id: String,
    kind: String,
    view_instance_id: Option<String>,
}

pub struct PluginHost {
    pub supervisor: std::sync::Arc<Supervisor>,
    views: Mutex<HashMap<String, ViewInstance>>,
    registry: Mutex<HashMap<String, PluginRegistryEntry>>,
    hash_index: Mutex<HashMap<String, String>>,
    development: Mutex<HashMap<String, DevelopmentPlugin>>,
    development_hash_index: Mutex<HashMap<String, String>>,
    subscriptions: Mutex<HashMap<String, SubscriptionEntry>>,
    inflight: Mutex<HashMap<String, usize>>,
    plugin_windows: Mutex<HashMap<String, PluginWindowInstance>>,
}

impl PluginHost {
    pub fn new(app: AppHandle) -> Self {
        Self {
            supervisor: std::sync::Arc::new(Supervisor::new(app)),
            views: Mutex::new(HashMap::new()),
            registry: Mutex::new(HashMap::new()),
            hash_index: Mutex::new(HashMap::new()),
            development: Mutex::new(HashMap::new()),
            development_hash_index: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            plugin_windows: Mutex::new(HashMap::new()),
        }
    }

    // -- Plugin registry (protocol + supervisor lookups) --------------------------------

    pub fn replace_registry(
        &self,
        registry: HashMap<String, PluginRegistryEntry>,
        hash_index: HashMap<String, String>,
    ) {
        *self.registry.lock() = registry;
        *self.hash_index.lock() = hash_index;
    }

    pub fn plugin_entry(&self, plugin_id: &str) -> Option<PluginRegistryEntry> {
        if let Some(entry) = self.development_plugin(plugin_id) {
            let install_path = match &entry.ui_source {
                Some(DevelopmentUiSource::Static(path)) => path.clone(),
                _ => entry.root_path.clone(),
            };
            return Some(PluginRegistryEntry {
                plugin_id: entry.manifest.id.clone(),
                version: entry.manifest.version.clone(),
                package_hash: String::new(),
                install_path,
                name: entry.manifest.name.clone(),
                requires_node_runtime: entry.manifest.requires_node_runtime(),
                main: entry.manifest.main.clone(),
            });
        }
        self.registry.lock().get(plugin_id).cloned()
    }

    pub fn resolve_hash(&self, plugin_hash: &str) -> Option<PluginRegistryEntry> {
        let plugin_id = self
            .development_hash_index
            .lock()
            .get(plugin_hash)
            .cloned()
            .or_else(|| self.hash_index.lock().get(plugin_hash).cloned())?;
        self.plugin_entry(&plugin_id)
    }

    pub fn register_development_plugin(&self, entry: DevelopmentPlugin) {
        let plugin_id = entry.manifest.id.clone();
        let plugin_hash = super::ui::plugin_hash_of(&plugin_id);
        self.development.lock().insert(plugin_id.clone(), entry);
        self.development_hash_index
            .lock()
            .insert(plugin_hash, plugin_id);
    }

    pub fn development_plugin(&self, plugin_id: &str) -> Option<DevelopmentPlugin> {
        self.development.lock().get(plugin_id).cloned()
    }

    pub fn development_plugins(&self) -> Vec<DevelopmentPlugin> {
        self.development.lock().values().cloned().collect()
    }

    pub fn remove_development_plugin(&self, plugin_id: &str) -> Option<DevelopmentPlugin> {
        let removed = self.development.lock().remove(plugin_id);
        if removed.is_some() {
            self.development_hash_index
                .lock()
                .retain(|_, id| id != plugin_id);
        }
        removed
    }

    /// Refresh the in-memory development manifest after the project file is saved so
    /// declarative contributions (apps/actions keywords, names, etc.) update without reconnect.
    pub fn update_development_manifest_for_project(
        &self,
        project_id: &str,
        manifest: PluginManifest,
    ) -> bool {
        let mut development = self.development.lock();
        let Some(old_plugin_id) = development
            .iter()
            .find_map(|(plugin_id, entry)| {
                (entry.project_id == project_id).then(|| plugin_id.clone())
            })
        else {
            return false;
        };

        let Some(mut entry) = development.remove(&old_plugin_id) else {
            return false;
        };
        let new_plugin_id = manifest.id.clone();
        entry.manifest = manifest;
        development.insert(new_plugin_id.clone(), entry);

        if old_plugin_id != new_plugin_id {
            let old_hash = super::ui::plugin_hash_of(&old_plugin_id);
            let new_hash = super::ui::plugin_hash_of(&new_plugin_id);
            let mut hashes = self.development_hash_index.lock();
            hashes.remove(&old_hash);
            hashes.insert(new_hash, new_plugin_id);
        }

        true
    }

    pub fn plugin_storage_namespace(&self, plugin_id: &str) -> String {
        self.development_plugin(plugin_id)
            .filter(|entry| !entry.use_production_data)
            .map(|entry| format!("__tempo_dev__:{}:{plugin_id}", entry.project_id))
            .unwrap_or_else(|| plugin_id.to_string())
    }

    pub fn forget_plugin(&self, plugin_id: &str) {
        self.registry.lock().remove(plugin_id);
        let mut hashes = self.hash_index.lock();
        let stale: Vec<String> = hashes
            .iter()
            .filter(|(_, id)| id.as_str() == plugin_id)
            .map(|(hash, _)| hash.clone())
            .collect();
        for hash in stale {
            hashes.remove(&hash);
        }
    }

    // -- UI view instance registry -------------------------------------------------------

    pub fn create_view(
        &self,
        plugin_id: &str,
        app_local_id: &str,
        owner_window_label: Option<&str>,
    ) -> String {
        let id = format!("view-{}", generate_id());
        self.views.lock().insert(
            id.clone(),
            ViewInstance {
                plugin_id: plugin_id.to_string(),
                app_local_id: app_local_id.to_string(),
                owner_window_label: owner_window_label.map(str::to_string),
                last_session_payload: None,
            },
        );
        id
    }

    pub fn destroy_view(&self, view_instance_id: &str) -> Option<ViewInstance> {
        let view = self.views.lock().remove(view_instance_id);
        if let Some(view) = &view {
            self.subscriptions
                .lock()
                .retain(|_, sub| sub.view_instance_id.as_deref() != Some(view_instance_id));
            let _ = &view.plugin_id;
        }
        view
    }

    pub fn view(&self, view_instance_id: &str) -> Option<ViewInstance> {
        self.views.lock().get(view_instance_id).cloned()
    }

    pub fn views_for_plugin(&self, plugin_id: &str) -> Vec<String> {
        self.views
            .lock()
            .iter()
            .filter(|(_, v)| v.plugin_id == plugin_id)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn views_for_window(&self, window_label: &str) -> Vec<String> {
        self.views
            .lock()
            .iter()
            .filter(|(_, view)| view.owner_window_label.as_deref() == Some(window_label))
            .map(|(id, _)| id.clone())
            .collect()
    }

    // -- Standalone plugin windows ------------------------------------------------------

    pub fn register_plugin_window(&self, label: String, window: PluginWindowInstance) {
        self.plugin_windows.lock().insert(label, window);
    }

    pub fn plugin_window(&self, label: &str) -> Option<PluginWindowInstance> {
        self.plugin_windows.lock().get(label).cloned()
    }

    pub fn remove_plugin_window(&self, label: &str) -> Option<PluginWindowInstance> {
        self.plugin_windows.lock().remove(label)
    }

    pub fn plugin_window_labels_for_plugin(&self, plugin_id: &str) -> Vec<String> {
        self.plugin_windows
            .lock()
            .iter()
            .filter(|(_, window)| window.plugin_id == plugin_id)
            .map(|(label, _)| label.clone())
            .collect()
    }

    pub fn has_plugin_windows(&self) -> bool {
        !self.plugin_windows.lock().is_empty()
    }

    pub fn cache_session_payload(&self, view_instance_id: &str, payload: Value) -> bool {
        if let Some(view) = self.views.lock().get_mut(view_instance_id) {
            view.last_session_payload = Some(payload);
            true
        } else {
            false
        }
    }

    pub fn take_cached_session_payload(&self, view_instance_id: &str) -> Option<Value> {
        self.views
            .lock()
            .get(view_instance_id)
            .and_then(|v| v.last_session_payload.clone())
    }

    // -- Subscriptions (theme.onChange, etc.) --------------------------------------------

    pub fn register_subscription(
        &self,
        subscription_id: &str,
        plugin_id: &str,
        kind: &str,
        view_instance_id: Option<&str>,
    ) {
        self.subscriptions.lock().insert(
            subscription_id.to_string(),
            SubscriptionEntry {
                plugin_id: plugin_id.to_string(),
                kind: kind.to_string(),
                view_instance_id: view_instance_id.map(str::to_string),
            },
        );
    }

    pub fn release_subscription(&self, subscription_id: &str, plugin_id: &str) -> bool {
        let mut subs = self.subscriptions.lock();
        if subs
            .get(subscription_id)
            .is_some_and(|entry| entry.plugin_id == plugin_id)
        {
            subs.remove(subscription_id);
            true
        } else {
            false
        }
    }

    /// Release every subscription for a plugin (disable/disconnect cleanup, design §7).
    pub fn release_all_subscriptions_for_plugin(&self, plugin_id: &str) {
        self.subscriptions
            .lock()
            .retain(|_, sub| sub.plugin_id != plugin_id);
    }

    pub fn release_all_subscriptions_for_view(&self, view_instance_id: &str) {
        self.subscriptions
            .lock()
            .retain(|_, sub| sub.view_instance_id.as_deref() != Some(view_instance_id));
    }

    pub fn subscriptions_by_kind(&self, kind: &str) -> Vec<(String, String, Option<String>)> {
        self.subscriptions
            .lock()
            .iter()
            .filter(|(_, sub)| sub.kind == kind)
            .map(|(id, sub)| {
                (
                    id.clone(),
                    sub.plugin_id.clone(),
                    sub.view_instance_id.clone(),
                )
            })
            .collect()
    }

    // -- Per-plugin concurrency (design §7: max 32 in-flight requests) ------------------

    pub fn try_acquire_inflight_slot(&self, plugin_id: &str, max: usize) -> bool {
        let mut inflight = self.inflight.lock();
        let count = inflight.entry(plugin_id.to_string()).or_insert(0);
        if *count >= max {
            return false;
        }
        *count += 1;
        true
    }

    pub fn release_inflight_slot(&self, plugin_id: &str) {
        let mut inflight = self.inflight.lock();
        if let Some(count) = inflight.get_mut(plugin_id) {
            *count = count.saturating_sub(1);
        }
    }
}

pub fn generate_id() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
