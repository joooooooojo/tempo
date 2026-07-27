//! Plugin package manifest (manifest.json v1).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::ids::{is_valid_local_id, is_valid_plugin_id};

pub const MANIFEST_VERSION: u32 = 1;

/// Fixed UI document at the package root (must sit beside `manifest.json`).
pub const UI_ENTRY_FILE: &str = "index.html";

/// Allowed Runtime entry filenames at the package root (beside `manifest.json`).
/// Named `main.*` so they never collide with UI assets like `index.js` next to `index.html`.
pub const MAIN_ENTRY_FILES: &[&str] = &["main.mjs", "main.js"];

pub const APP_WINDOW_MIN_WIDTH: f64 = 320.0;
pub const APP_WINDOW_MAX_WIDTH: f64 = 4096.0;
pub const APP_WINDOW_MIN_HEIGHT: f64 = 240.0;
pub const APP_WINDOW_MAX_HEIGHT: f64 = 2160.0;
const MAX_MCP_SCHEMA_BYTES: usize = 64 * 1024;
const MAX_MCP_TOOLS_PER_PLUGIN: usize = 64;
const MAX_MCP_DESCRIPTION_CHARS: usize = 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub manifest_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub engines: PluginEngines,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    /// Root-level Runtime entry: must be `main.mjs` or `main.js` when present.
    /// Required for headless (no `apps[]`) plugins; optional for pure UI packages.
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub executables: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub activation_events: Vec<String>,
    #[serde(default)]
    pub contributes: PluginContributes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEngines {
    pub tempo: String,
    pub plugin_api: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributes {
    #[serde(default)]
    pub apps: Vec<ContributedApp>,
    #[serde(default)]
    pub actions: Vec<ContributedAction>,
    #[serde(default)]
    pub commands: Vec<ContributedCommand>,
    #[serde(default)]
    pub hooks: Vec<ContributedHook>,
    #[serde(default)]
    pub mcp_tools: Vec<ContributedMcpTool>,
    #[serde(default)]
    pub settings: Vec<ContributedSetting>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributedApp {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// Must be the package-root `index.html` (same directory as `manifest.json`).
    pub entry: String,
    /// `normal` renders inside the main panel; `standalone` uses a normal OS window.
    #[serde(default)]
    pub window_mode: PluginWindowMode,
    #[serde(default)]
    pub rect: AppRect,
    #[serde(default)]
    pub session_version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginWindowMode {
    Normal,
    Standalone,
}

impl Default for PluginWindowMode {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRect {
    #[serde(default)]
    pub width: Option<RectValue>,
    #[serde(default)]
    pub height: Option<RectValue>,
    #[serde(default)]
    pub x: Option<RectValue>,
    #[serde(default)]
    pub y: Option<RectValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RectValue {
    Pixels(f64),
    Expression(String),
}

impl AppRect {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(value) = &self.width {
            validate_rect_dimension(
                value,
                "rect.width",
                APP_WINDOW_MIN_WIDTH,
                APP_WINDOW_MAX_WIDTH,
            )?;
        }
        if let Some(value) = &self.height {
            validate_rect_dimension(
                value,
                "rect.height",
                APP_WINDOW_MIN_HEIGHT,
                APP_WINDOW_MAX_HEIGHT,
            )?;
        }
        if let Some(value) = &self.x {
            validate_rect_position(value, "rect.x")?;
        }
        if let Some(value) = &self.y {
            validate_rect_position(value, "rect.y")?;
        }
        Ok(())
    }
}

fn parse_percentage(value: &str) -> Option<f64> {
    value
        .trim()
        .strip_suffix('%')
        .and_then(|number| number.trim().parse::<f64>().ok())
        .filter(|number| number.is_finite())
}

fn validate_rect_dimension(
    value: &RectValue,
    field: &str,
    min_pixels: f64,
    max_pixels: f64,
) -> Result<(), String> {
    match value {
        RectValue::Pixels(pixels) if (min_pixels..=max_pixels).contains(pixels) => Ok(()),
        RectValue::Pixels(_) => Err(format!(
            "{field} must be between {min_pixels:.0} and {max_pixels:.0} pixels"
        )),
        RectValue::Expression(expression) => match parse_percentage(expression) {
            Some(percent) if (1.0..=100.0).contains(&percent) => Ok(()),
            _ => Err(format!(
                "{field} must be pixels or a percentage from 1% to 100%"
            )),
        },
    }
}

fn validate_rect_position(value: &RectValue, field: &str) -> Result<(), String> {
    match value {
        RectValue::Pixels(pixels) if pixels.is_finite() => Ok(()),
        RectValue::Pixels(_) => Err(format!("{field} must be a finite number")),
        RectValue::Expression(expression) if expression.trim().eq_ignore_ascii_case("center") => {
            Ok(())
        }
        RectValue::Expression(expression) => match parse_percentage(expression) {
            Some(percent) if (0.0..=100.0).contains(&percent) => Ok(()),
            _ => Err(format!(
                "{field} must be pixels, center, or a percentage from 0% to 100%"
            )),
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributedAction {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub accepts: Option<Vec<ActionInputKind>>,
    /// Legacy compatibility: true/false both map to text-only (none input is no longer supported).
    #[serde(default)]
    pub requires_query: Option<bool>,
    #[serde(default)]
    pub title_template: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionInputKind {
    Text,
    Image,
}

impl ContributedAction {
    pub fn accepted_inputs(&self) -> Vec<ActionInputKind> {
        self.accepts
            .clone()
            .unwrap_or_else(|| vec![ActionInputKind::Text])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributedCommand {
    pub id: String,
    pub title: String,
    #[serde(default = "default_private")]
    pub visibility: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributedHook {
    pub event: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributedMcpTool {
    pub name: String,
    pub description: String,
    pub command: String,
    #[serde(default = "default_mcp_input_schema")]
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub annotations: Option<ContributedMcpToolAnnotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributedMcpToolAnnotations {
    #[serde(default)]
    pub read_only_hint: Option<bool>,
    #[serde(default)]
    pub destructive_hint: Option<bool>,
    #[serde(default)]
    pub idempotent_hint: Option<bool>,
    #[serde(default)]
    pub open_world_hint: Option<bool>,
}

const MAX_SETTINGS_PER_PLUGIN: usize = 64;
const MAX_SETTING_TITLE_CHARS: usize = 128;
const MAX_SETTING_DESCRIPTION_CHARS: usize = 512;
const MAX_SETTING_OPTIONS: usize = 64;

/// Host-rendered control kinds for `contributes.settings` (v1).
/// Inspired by VS Code configuration property schemas, but named for UI controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingFieldType {
    Switch,
    Select,
    Multiselect,
    Input,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingOption {
    pub value: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributedSetting {
    pub id: String,
    #[serde(rename = "type")]
    pub setting_type: SettingFieldType,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: Value,
    /// Required for `select` / `multiselect`.
    #[serde(default)]
    pub options: Option<Vec<SettingOption>>,
    /// Optional placeholder for `input`.
    #[serde(default)]
    pub placeholder: Option<String>,
}

impl ContributedSetting {
    fn validate_options(&self) -> Result<&[SettingOption], String> {
        let Some(options) = &self.options else {
            return Err(format!(
                "setting {} of type {:?} requires a non-empty options array",
                self.id, self.setting_type
            ));
        };
        if options.is_empty() {
            return Err(format!(
                "setting {} of type {:?} requires a non-empty options array",
                self.id, self.setting_type
            ));
        }
        if options.len() > MAX_SETTING_OPTIONS {
            return Err(format!(
                "setting {} may declare at most {MAX_SETTING_OPTIONS} options",
                self.id
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for option in options {
            if option.value.is_empty() {
                return Err(format!(
                    "setting {} option values must be non-empty strings",
                    self.id
                ));
            }
            if !seen.insert(option.value.as_str()) {
                return Err(format!(
                    "setting {} options contains duplicate value {}",
                    self.id, option.value
                ));
            }
            if let Some(label) = &option.label {
                if label.trim().is_empty() {
                    return Err(format!(
                        "setting {} option labels must be non-empty when set",
                        self.id
                    ));
                }
            }
        }
        Ok(options)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !is_valid_local_id(&self.id) {
            return Err(format!("invalid setting id: {}", self.id));
        }
        if self.title.trim().is_empty() {
            return Err(format!("setting {} title is required", self.id));
        }
        if self.title.chars().count() > MAX_SETTING_TITLE_CHARS {
            return Err(format!(
                "setting {} title exceeds {MAX_SETTING_TITLE_CHARS} characters",
                self.id
            ));
        }
        if let Some(description) = &self.description {
            if description.chars().count() > MAX_SETTING_DESCRIPTION_CHARS {
                return Err(format!(
                    "setting {} description exceeds {MAX_SETTING_DESCRIPTION_CHARS} characters",
                    self.id
                ));
            }
        }
        match self.setting_type {
            SettingFieldType::Switch => {
                if !self.default.is_boolean() {
                    return Err(format!("setting {} default must be a boolean", self.id));
                }
                if self.options.is_some() {
                    return Err(format!(
                        "setting {} of type switch must not declare options",
                        self.id
                    ));
                }
            }
            SettingFieldType::Input => {
                if !self.default.is_string() {
                    return Err(format!("setting {} default must be a string", self.id));
                }
                if self.options.is_some() {
                    return Err(format!(
                        "setting {} of type input must not declare options",
                        self.id
                    ));
                }
            }
            SettingFieldType::Select => {
                let options = self.validate_options()?;
                let Some(default) = self.default.as_str() else {
                    return Err(format!(
                        "setting {} default must be a string matching an option value",
                        self.id
                    ));
                };
                if !options.iter().any(|option| option.value == default) {
                    return Err(format!(
                        "setting {} default must be one of the option values",
                        self.id
                    ));
                }
            }
            SettingFieldType::Multiselect => {
                let options = self.validate_options()?;
                let Some(defaults) = self.default.as_array() else {
                    return Err(format!(
                        "setting {} default must be an array of option values",
                        self.id
                    ));
                };
                let mut seen = std::collections::HashSet::new();
                for item in defaults {
                    let Some(raw) = item.as_str() else {
                        return Err(format!(
                            "setting {} default array items must be strings",
                            self.id
                        ));
                    };
                    if !options.iter().any(|option| option.value == raw) {
                        return Err(format!(
                            "setting {} default contains unknown option value {raw}",
                            self.id
                        ));
                    }
                    if !seen.insert(raw) {
                        return Err(format!(
                            "setting {} default must not contain duplicate values",
                            self.id
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn coerce_value(&self, value: &Value) -> Result<Value, String> {
        match self.setting_type {
            SettingFieldType::Switch => {
                if value.is_boolean() {
                    Ok(value.clone())
                } else {
                    Err(format!("setting {} expects a boolean", self.id))
                }
            }
            SettingFieldType::Input => {
                if value.is_string() {
                    Ok(value.clone())
                } else {
                    Err(format!("setting {} expects a string", self.id))
                }
            }
            SettingFieldType::Select => {
                let Some(raw) = value.as_str() else {
                    return Err(format!("setting {} expects a string", self.id));
                };
                let allowed = self.options.as_ref().map(Vec::as_slice).unwrap_or(&[]);
                if allowed.iter().any(|option| option.value == raw) {
                    Ok(Value::String(raw.to_string()))
                } else {
                    Err(format!("setting {} value is not in options", self.id))
                }
            }
            SettingFieldType::Multiselect => {
                let Some(items) = value.as_array() else {
                    return Err(format!("setting {} expects a string array", self.id));
                };
                let allowed = self.options.as_ref().map(Vec::as_slice).unwrap_or(&[]);
                let mut out = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for item in items {
                    let Some(raw) = item.as_str() else {
                        return Err(format!(
                            "setting {} array items must be strings",
                            self.id
                        ));
                    };
                    if !allowed.iter().any(|option| option.value == raw) {
                        return Err(format!("setting {} value is not in options", self.id));
                    }
                    if seen.insert(raw.to_string()) {
                        out.push(Value::String(raw.to_string()));
                    }
                }
                Ok(Value::Array(out))
            }
        }
    }
}

fn default_mcp_input_schema() -> Value {
    json!({ "type": "object", "properties": {} })
}

fn default_private() -> String {
    "private".into()
}

impl PluginManifest {
    pub fn parse_str(raw: &str) -> Result<Self, String> {
        let manifest: Self =
            serde_json::from_str(raw).map_err(|e| format!("invalid manifest.json: {e}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.manifest_version != MANIFEST_VERSION {
            return Err(format!(
                "unsupported manifestVersion {}; expected {MANIFEST_VERSION}",
                self.manifest_version
            ));
        }
        if !is_valid_plugin_id(&self.id) {
            return Err(format!("invalid plugin id: {}", self.id));
        }
        if self.name.trim().is_empty() {
            return Err("plugin name is required".into());
        }
        if self.version.trim().is_empty() {
            return Err("plugin version is required".into());
        }
        if self.engines.plugin_api.trim().is_empty() {
            return Err("engines.pluginApi is required".into());
        }

        let has_ui = !self.contributes.apps.is_empty();
        let command_ids: std::collections::HashSet<&str> = self
            .contributes
            .commands
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        let app_ids: std::collections::HashSet<&str> = self
            .contributes
            .apps
            .iter()
            .map(|app| app.id.as_str())
            .collect();

        for command in &self.contributes.commands {
            if !is_valid_local_id(&command.id) {
                return Err(format!("invalid command id: {}", command.id));
            }
        }

        for app in &self.contributes.apps {
            if !is_valid_local_id(&app.id) {
                return Err(format!("invalid app id: {}", app.id));
            }
            validate_relative_path(&app.entry, "apps.entry")?;
            if app.entry != UI_ENTRY_FILE {
                return Err(format!(
                    "apps.entry must be `{UI_ENTRY_FILE}` at the package root (got {})",
                    app.entry
                ));
            }
            if let Some(icon) = &app.icon {
                validate_relative_path(icon, "apps.icon")?;
            }
            app.rect.validate()?;
            if app.session_version == Some(0) {
                return Err("sessionVersion must be a positive integer".into());
            }
        }

        for action in &self.contributes.actions {
            if !is_valid_local_id(&action.id) {
                return Err(format!("invalid action id: {}", action.id));
            }
            match (&action.app, &action.command) {
                (Some(app), None) => {
                    if !app_ids.contains(app.as_str()) {
                        return Err(format!(
                            "action {} references missing app {}",
                            action.id, app
                        ));
                    }
                }
                (None, Some(command)) => {
                    if !command_ids.contains(command.as_str()) {
                        return Err(format!(
                            "action {} references missing command {}",
                            action.id, command
                        ));
                    }
                    if self.main.is_none() {
                        return Err(format!(
                            "action {} targets command {} but the plugin has no main entry",
                            action.id, command
                        ));
                    }
                }
                (Some(_), Some(_)) => {
                    return Err(format!(
                        "action {} must target exactly one of app or command",
                        action.id
                    ));
                }
                (None, None) => {
                    return Err(format!(
                        "action {} must target an app or command",
                        action.id
                    ));
                }
            }
            if action.accepts.is_some() && action.requires_query.is_some() {
                return Err(format!(
                    "action {} cannot use accepts and requiresQuery together",
                    action.id
                ));
            }
            let accepted_inputs = action.accepted_inputs();
            if accepted_inputs.is_empty() {
                return Err(format!("action {} accepts must not be empty", action.id));
            }
            let unique_inputs: std::collections::HashSet<_> =
                accepted_inputs.iter().copied().collect();
            if unique_inputs.len() != accepted_inputs.len() {
                return Err(format!(
                    "action {} accepts contains duplicate input kinds",
                    action.id
                ));
            }
            if let Some(icon) = &action.icon {
                validate_relative_path(icon, "actions.icon")?;
            }
        }

        for hook in &self.contributes.hooks {
            if !command_ids.contains(hook.command.as_str()) {
                return Err(format!(
                    "hook {} references missing command {}",
                    hook.event, hook.command
                ));
            }
        }

        if self.contributes.mcp_tools.len() > MAX_MCP_TOOLS_PER_PLUGIN {
            return Err(format!(
                "plugins may declare at most {MAX_MCP_TOOLS_PER_PLUGIN} MCP tools"
            ));
        }
        let mut mcp_tool_names = std::collections::HashSet::new();
        for tool in &self.contributes.mcp_tools {
            if !is_valid_local_id(&tool.name) {
                return Err(format!("invalid mcpTool name: {}", tool.name));
            }
            if !mcp_tool_names.insert(tool.name.as_str()) {
                return Err(format!("duplicate mcpTool name: {}", tool.name));
            }
            if tool.description.trim().is_empty() {
                return Err(format!("mcpTool {} description is required", tool.name));
            }
            if tool.description.chars().count() > MAX_MCP_DESCRIPTION_CHARS {
                return Err(format!(
                    "mcpTool {} description exceeds {MAX_MCP_DESCRIPTION_CHARS} characters",
                    tool.name
                ));
            }
            if !command_ids.contains(tool.command.as_str()) {
                return Err(format!(
                    "mcpTool {} references missing command {}",
                    tool.name, tool.command
                ));
            }
            if self.main.is_none() {
                return Err(format!(
                    "mcpTool {} targets command {} but the plugin has no main entry",
                    tool.name, tool.command
                ));
            }
            validate_mcp_schema(&tool.input_schema, &tool.name, "inputSchema")?;
            if let Some(schema) = &tool.output_schema {
                validate_mcp_schema(schema, &tool.name, "outputSchema")?;
            }
        }

        if self.contributes.settings.len() > MAX_SETTINGS_PER_PLUGIN {
            return Err(format!(
                "plugins may declare at most {MAX_SETTINGS_PER_PLUGIN} settings"
            ));
        }
        let mut setting_ids = std::collections::HashSet::new();
        for setting in &self.contributes.settings {
            if !setting_ids.insert(setting.id.as_str()) {
                return Err(format!("duplicate setting id: {}", setting.id));
            }
            setting.validate()?;
        }

        match &self.main {
            Some(main) => {
                validate_relative_path(main, "main")?;
                if !is_allowed_main_entry(main) {
                    return Err(format!(
                        "main must be `{}` or `{}` at the package root (got {main})",
                        MAIN_ENTRY_FILES[0], MAIN_ENTRY_FILES[1]
                    ));
                }
            }
            None => {
                if !has_ui {
                    return Err(format!(
                        "headless plugins require main (`{}` or `{}`) at the package root",
                        MAIN_ENTRY_FILES[0], MAIN_ENTRY_FILES[1]
                    ));
                }
                if !self.activation_events.is_empty() {
                    return Err("activationEvents require a main entry".into());
                }
            }
        }

        for exe in &self.executables {
            validate_relative_path(exe, "executables")?;
        }

        Ok(())
    }

    pub fn mcp_toolset_fingerprint(&self) -> Result<String, String> {
        let mut tools = self.contributes.mcp_tools.clone();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let value = serde_json::to_value(tools)
            .map_err(|error| format!("serialize MCP toolset: {error}"))?;
        let canonical = canonicalize_json(value);
        let bytes = serde_json::to_vec(&canonical)
            .map_err(|error| format!("serialize canonical MCP toolset: {error}"))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }

    pub fn requires_node_runtime(&self) -> bool {
        self.main.is_some()
    }

    pub fn has_ui(&self) -> bool {
        !self.contributes.apps.is_empty()
    }

    /// Behavior-derived kind for UI (`ui` / `hybrid` / `headless`).
    /// Manifest `kind` is classification-only; display uses contributes + main.
    pub fn resolved_kind(&self) -> &'static str {
        match (self.has_ui(), self.requires_node_runtime()) {
            (true, true) => "hybrid",
            (true, false) => "ui",
            (false, _) => "headless",
        }
    }
}

fn validate_mcp_schema(schema: &Value, tool_name: &str, field: &str) -> Result<(), String> {
    let object = schema
        .as_object()
        .ok_or_else(|| format!("mcpTool {tool_name} {field} must be an object"))?;
    if object.get("type").and_then(Value::as_str) != Some("object") {
        return Err(format!(
            "mcpTool {tool_name} {field} must declare type object"
        ));
    }
    let size = serde_json::to_vec(schema)
        .map_err(|error| format!("serialize mcpTool {tool_name} {field}: {error}"))?
        .len();
    if size > MAX_MCP_SCHEMA_BYTES {
        return Err(format!(
            "mcpTool {tool_name} {field} exceeds {MAX_MCP_SCHEMA_BYTES} bytes"
        ));
    }
    jsonschema::validator_for(schema)
        .map(|_| ())
        .map_err(|error| format!("mcpTool {tool_name} has invalid {field}: {error}"))
}

fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_json).collect()),
        Value::Object(values) => {
            let mut entries: Vec<_> = values.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize_json(value)))
                    .collect(),
            )
        }
        primitive => primitive,
    }
}

pub fn is_allowed_main_entry(path: &str) -> bool {
    MAIN_ENTRY_FILES.contains(&path)
}

pub fn validate_relative_path(path: &str, field: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} path is empty"));
    }
    if trimmed.contains('\\') {
        return Err(format!("{field} must use / separators: {path}"));
    }
    if trimmed.starts_with('/')
        || trimmed.contains(':')
        || trimmed.contains("..")
        || trimmed.contains("//")
    {
        return Err(format!("{field} is not a safe relative path: {path}"));
    }
    if trimmed.starts_with("http:") || trimmed.starts_with("https:") || trimmed.starts_with("file:")
    {
        return Err(format!("{field} must not be a URL: {path}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_hybrid_at_package_root() {
        let raw = r#"{
          "manifestVersion": 1,
          "id": "com.example.hello",
          "name": "Hello",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.0.0" },
          "main": "main.mjs",
          "contributes": {
            "apps": [{
              "id": "main",
              "name": "Hello",
              "entry": "index.html"
            }],
            "commands": [{ "id": "hello", "title": "Hello" }],
            "actions": [{ "id": "run", "name": "Run", "command": "hello" }]
          }
        }"#;
        let m = PluginManifest::parse_str(raw).unwrap();
        assert!(m.requires_node_runtime());
        assert!(m.has_ui());
        assert_eq!(m.contributes.apps[0].entry, UI_ENTRY_FILE);
        assert_eq!(m.main.as_deref(), Some("main.mjs"));
    }

    #[test]
    fn parses_repository_example_manifest() {
        let raw = include_str!("../../../examples/plugins/com.example.hello/manifest.json");
        let manifest = PluginManifest::parse_str(raw).unwrap();
        assert_eq!(manifest.id, "com.example.hello");
        assert_eq!(manifest.version, "1.0.5");
    }

    #[test]
    fn rejects_nested_ui_or_main_entry() {
        let nested_ui = r#"{
          "manifestVersion": 1,
          "id": "com.example.hello",
          "name": "Hello",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.0.0" },
          "contributes": {
            "apps": [{ "id": "main", "name": "Hello", "entry": "dist/ui/index.html" }]
          }
        }"#;
        assert!(PluginManifest::parse_str(nested_ui).is_err());

        let nested_main = r#"{
          "manifestVersion": 1,
          "id": "com.example.hello",
          "name": "Hello",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.0.0" },
          "main": "dist/main/index.mjs",
          "contributes": {
            "apps": [{ "id": "main", "name": "Hello", "entry": "index.html" }]
          }
        }"#;
        assert!(PluginManifest::parse_str(nested_main).is_err());
    }

    #[test]
    fn headless_requires_root_main() {
        let missing = r#"{
          "manifestVersion": 1,
          "id": "com.example.hello",
          "name": "Hello",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.0.0" },
          "contributes": {
            "commands": [{ "id": "hello", "title": "Hello" }],
            "actions": [{ "id": "run", "name": "Run", "command": "hello" }]
          }
        }"#;
        assert!(PluginManifest::parse_str(missing).is_err());

        let ok = r#"{
          "manifestVersion": 1,
          "id": "com.example.hello",
          "name": "Hello",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.0.0" },
          "main": "main.js",
          "contributes": {
            "commands": [{ "id": "hello", "title": "Hello" }],
            "actions": [{ "id": "run", "name": "Run", "command": "hello" }]
          }
        }"#;
        assert!(PluginManifest::parse_str(ok).is_ok());
    }

    #[test]
    fn rejects_missing_command_ref() {
        let raw = r#"{
          "manifestVersion": 1,
          "id": "com.example.hello",
          "name": "Hello",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.0.0" },
          "main": "main.mjs",
          "contributes": {
            "actions": [{ "id": "run", "name": "Run", "command": "missing" }]
          }
        }"#;
        assert!(PluginManifest::parse_str(raw).is_err());
    }

    #[test]
    fn action_can_open_an_app_for_image_input() {
        let raw = r#"{
          "manifestVersion": 1,
          "id": "com.example.crop",
          "name": "Crop",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.2.0" },
          "contributes": {
            "apps": [{ "id": "crop", "name": "Crop", "entry": "index.html" }],
            "actions": [{
              "id": "crop-image",
              "name": "Crop image",
              "accepts": ["image"],
              "app": "crop"
            }]
          }
        }"#;
        let manifest = PluginManifest::parse_str(raw).unwrap();
        let action = &manifest.contributes.actions[0];
        assert_eq!(action.app.as_deref(), Some("crop"));
        assert_eq!(action.command, None);
        assert_eq!(action.accepted_inputs(), vec![ActionInputKind::Image]);
        assert!(!manifest.requires_node_runtime());
    }

    #[test]
    fn action_requires_exactly_one_target() {
        let base = r#"{
          "manifestVersion": 1,
          "id": "com.example.action",
          "name": "Action",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.2.0" },
          "main": "main.mjs",
          "contributes": {
            "apps": [{ "id": "main", "name": "Main", "entry": "index.html" }],
            "commands": [{ "id": "run", "title": "Run" }],
            "actions": [{ "id": "action", "name": "Action" }]
          }
        }"#;
        assert!(PluginManifest::parse_str(base).is_err());
        let both = base.replace(
            "\"name\": \"Action\" }],",
            "\"name\": \"Action\", \"app\": \"main\", \"command\": \"run\" }],",
        );
        assert!(PluginManifest::parse_str(&both).is_err());
    }

    #[test]
    fn legacy_requires_query_maps_to_accepts() {
        let raw = r#"{
          "manifestVersion": 1,
          "id": "com.example.legacy",
          "name": "Legacy",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.0.0" },
          "main": "main.mjs",
          "contributes": {
            "commands": [{ "id": "run", "title": "Run" }],
            "actions": [{
              "id": "action",
              "name": "Action",
              "requiresQuery": false,
              "command": "run"
            }]
          }
        }"#;
        let manifest = PluginManifest::parse_str(raw).unwrap();
        assert_eq!(
            manifest.contributes.actions[0].accepted_inputs(),
            vec![ActionInputKind::Text]
        );
    }

    #[test]
    fn validates_window_mode_and_rect() {
        let valid = r#"{
          "manifestVersion": 1,
          "id": "com.example.window",
          "name": "Window",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.0.0" },
          "contributes": {
            "apps": [{
              "id": "main",
              "name": "Window",
              "entry": "index.html",
              "windowMode": "standalone",
              "rect": { "width": "75%", "height": 640, "x": "center", "y": "10%" }
            }]
          }
        }"#;
        let manifest = PluginManifest::parse_str(valid).unwrap();
        let app = &manifest.contributes.apps[0];
        assert_eq!(app.window_mode, PluginWindowMode::Standalone);
        assert!(matches!(app.rect.width, Some(RectValue::Expression(ref value)) if value == "75%"));
        assert!(matches!(app.rect.x, Some(RectValue::Expression(ref value)) if value == "center"));

        let too_small = valid.replace("\"height\": 640", "\"height\": 200");
        assert!(PluginManifest::parse_str(&too_small).is_err());

        let invalid_position = valid.replace("\"y\": \"10%\"", "\"y\": \"outside\"");
        assert!(PluginManifest::parse_str(&invalid_position).is_err());

        let invalid_session_version = valid.replace(
            "\"rect\": { \"width\": \"75%\", \"height\": 640, \"x\": \"center\", \"y\": \"10%\" }",
            "\"rect\": { \"width\": \"75%\", \"height\": 640, \"x\": \"center\", \"y\": \"10%\" }, \"sessionVersion\": 0",
        );
        assert!(PluginManifest::parse_str(&invalid_session_version).is_err());
    }

    #[test]
    fn validates_mcp_tool_contract_and_stable_fingerprint() {
        let raw = r#"{
          "manifestVersion": 1,
          "id": "com.example.mcp",
          "name": "MCP",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.2.0" },
          "main": "main.mjs",
          "contributes": {
            "commands": [{ "id": "summarize", "title": "Summarize" }],
            "mcpTools": [{
              "name": "summarize",
              "description": "Summarize a note",
              "command": "summarize",
              "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string" }, "limit": { "type": "integer" } },
                "required": ["id"]
              },
              "outputSchema": {
                "type": "object",
                "properties": { "summary": { "type": "string" } },
                "required": ["summary"]
              },
              "annotations": { "readOnlyHint": true, "openWorldHint": false }
            }]
          }
        }"#;
        let reordered = raw.replace(
            r#""id": { "type": "string" }, "limit": { "type": "integer" }"#,
            r#""limit": { "type": "integer" }, "id": { "type": "string" }"#,
        );
        let first = PluginManifest::parse_str(raw).unwrap();
        let second = PluginManifest::parse_str(&reordered).unwrap();
        assert_eq!(
            first.mcp_toolset_fingerprint().unwrap(),
            second.mcp_toolset_fingerprint().unwrap()
        );
        assert_eq!(
            first.contributes.mcp_tools[0]
                .annotations
                .as_ref()
                .and_then(|annotations| annotations.read_only_hint),
            Some(true)
        );
    }

    #[test]
    fn rejects_invalid_mcp_tool_contracts() {
        let valid = r#"{
          "manifestVersion": 1,
          "id": "com.example.mcp",
          "name": "MCP",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.2.0" },
          "main": "main.mjs",
          "contributes": {
            "commands": [{ "id": "run", "title": "Run" }],
            "mcpTools": [{
              "name": "run",
              "description": "Run",
              "command": "run",
              "inputSchema": { "type": "object" }
            }]
          }
        }"#;
        assert!(PluginManifest::parse_str(valid).is_ok());
        assert!(PluginManifest::parse_str(&valid.replace(
            r#""inputSchema": { "type": "object" }"#,
            r#""inputSchema": { "type": "string" }"#
        ))
        .is_err());
        assert!(PluginManifest::parse_str(
            &valid.replace(r#""description": "Run""#, r#""description": "  ""#)
        )
        .is_err());
        let duplicate = valid.replace(
            "]\n          }",
            r#", {
              "name": "run",
              "description": "Run again",
              "command": "run",
              "inputSchema": { "type": "object" }
            }]
          }"#,
        );
        assert!(PluginManifest::parse_str(&duplicate).is_err());
        assert!(PluginManifest::parse_str(&valid.replace(r#""main": "main.mjs","#, "")).is_err());
    }

    #[test]
    fn validates_contributed_settings() {
        let valid = r#"{
          "manifestVersion": 1,
          "id": "com.example.settings",
          "name": "Settings",
          "version": "1.0.0",
          "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.2.0" },
          "contributes": {
            "apps": [{ "id": "main", "name": "Main", "entry": "index.html" }],
            "settings": [
              {
                "id": "loud",
                "type": "switch",
                "title": "大声打招呼",
                "default": false
              },
              {
                "id": "theme",
                "type": "select",
                "title": "主题",
                "default": "auto",
                "options": [
                  { "value": "auto", "label": "跟随系统" },
                  { "value": "light", "label": "浅色" },
                  { "value": "dark", "label": "深色" }
                ]
              },
              {
                "id": "langs",
                "type": "multiselect",
                "title": "语言",
                "default": ["zh"],
                "options": [
                  { "value": "zh", "label": "中文" },
                  { "value": "en", "label": "English" }
                ]
              },
              {
                "id": "name",
                "type": "input",
                "title": "默认称呼",
                "default": "world",
                "placeholder": "world"
              }
            ]
          }
        }"#;
        let manifest = PluginManifest::parse_str(valid).unwrap();
        assert_eq!(manifest.contributes.settings.len(), 4);
        assert_eq!(
            manifest.contributes.settings[0].setting_type,
            SettingFieldType::Switch
        );
        assert_eq!(
            manifest.contributes.settings[2].setting_type,
            SettingFieldType::Multiselect
        );

        assert!(PluginManifest::parse_str(&valid.replace(
            r#""default": "auto""#,
            r#""default": "neon""#
        ))
        .is_err());
        assert!(PluginManifest::parse_str(&valid.replace(
            r#""type": "switch""#,
            r#""type": "switch", "options": [{ "value": "x" }]"#
        ))
        .is_err());
    }
}
