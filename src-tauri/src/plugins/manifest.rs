//! Plugin package manifest (manifest.json v1).

use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub input_schema: serde_json::Value,
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

        for tool in &self.contributes.mcp_tools {
            if !command_ids.contains(tool.command.as_str()) {
                return Err(format!(
                    "mcpTool {} references missing command {}",
                    tool.name, tool.command
                ));
            }
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

    pub fn requires_node_runtime(&self) -> bool {
        self.main.is_some()
    }

    pub fn has_ui(&self) -> bool {
        !self.contributes.apps.is_empty()
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
        assert_eq!(manifest.version, "1.0.4");
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
}
