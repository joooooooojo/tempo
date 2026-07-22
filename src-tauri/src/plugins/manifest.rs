//! Plugin package manifest (manifest.json v1).

use serde::{Deserialize, Serialize};

use super::ids::{is_valid_local_id, is_valid_plugin_id};

pub const MANIFEST_VERSION: u32 = 1;

/// Fixed UI document at the package root (must sit beside `manifest.json`).
pub const UI_ENTRY_FILE: &str = "index.html";

/// Allowed Runtime entry filenames at the package root (beside `manifest.json`).
/// Named `main.*` so they never collide with UI assets like `index.js` next to `index.html`.
pub const MAIN_ENTRY_FILES: &[&str] = &["main.mjs", "main.js"];

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
    #[serde(default)]
    pub default_size: Option<DefaultSize>,
    #[serde(default)]
    pub persist_session: bool,
    #[serde(default)]
    pub session_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultSize {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
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
    #[serde(default = "default_true")]
    pub requires_query: bool,
    #[serde(default)]
    pub title_template: Option<String>,
    pub command: String,
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

fn default_true() -> bool {
    true
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
            if app.persist_session {
                match app.session_version {
                    Some(v) if v >= 1 => {}
                    _ => {
                        return Err(
                            "persistSession=true requires a positive sessionVersion".into()
                        )
                    }
                }
            }
        }

        for action in &self.contributes.actions {
            if !is_valid_local_id(&action.id) {
                return Err(format!("invalid action id: {}", action.id));
            }
            if !command_ids.contains(action.command.as_str()) {
                return Err(format!(
                    "action {} references missing command {}",
                    action.id, action.command
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
}
