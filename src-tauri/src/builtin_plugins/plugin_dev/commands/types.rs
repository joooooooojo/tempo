use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevProject {
    pub id: String,
    pub root_path: String,
    pub plugin_id: Option<String>,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub last_opened_at: String,
    pub created_at: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestDiagnostic {
    pub severity: String,
    pub code: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevManifestDocument {
    pub raw: String,
    pub hash: String,
    pub parsed: Option<Value>,
    pub valid: bool,
    pub diagnostics: Vec<ManifestDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevPreferences {
    pub ui_source_kind: Option<String>,
    pub ui_service_url: Option<String>,
    pub ui_static_root: Option<String>,
    pub runtime_dev_entry: Option<String>,
    #[serde(default = "default_true")]
    pub auto_reconnect_runtime: bool,
    #[serde(default)]
    pub use_production_data: bool,
}

fn default_true() -> bool {
    true
}

impl Default for PluginDevPreferences {
    fn default() -> Self {
        Self {
            ui_source_kind: None,
            ui_service_url: None,
            ui_static_root: None,
            runtime_dev_entry: None,
            auto_reconnect_runtime: true,
            use_production_data: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevConnectionStatus {
    pub connected: bool,
    pub plugin_id: Option<String>,
    pub state: String,
    pub ui_state: Option<String>,
    pub runtime_state: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDevProjectDetail {
    pub project: PluginDevProject,
    pub manifest: PluginDevManifestDocument,
    pub preferences: PluginDevPreferences,
    pub connection: PluginDevConnectionStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectArgs {
    pub root_path: String,
    pub plugin_id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIdArgs {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectArgs {
    pub root_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteManifestArgs {
    pub project_id: String,
    pub raw: String,
    pub expected_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePreferencesArgs {
    pub project_id: String,
    pub preferences: PluginDevPreferences,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeUiUrlArgs {
    pub url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeUiUrlResult {
    pub reachable: bool,
    pub status: Option<u16>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMcpToolArgs {
    pub project_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub arguments: Value,
}
