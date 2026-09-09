//! Dynamic MCP exposure for plugin Runtime MCP Tool handlers.
//!
//! The platform owns the MCP endpoint and external names. Plugins only declare public tool
//! contracts registered with `tempo.mcpTools.register`; every list and call rechecks trust and approval.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::db::AppState;

use super::bridge::{invoke_runtime_mcp_tool, DEFAULT_TIMEOUT};
use super::host::PluginHost;
use super::manifest::{ContributedMcpToolAnnotations, PluginManifest};
use super::paths::packages_dir;
use super::trust::{
    ensure_plugin_tables, is_plugin_mcp_exposed, is_plugin_mcp_tool_enabled,
    list_installed_plugins, InstalledPluginRow,
};

const EXTERNAL_TOOL_PREFIX: &str = "tempo_plugin_";
const MAX_EXTERNAL_TOOL_NAME_BYTES: usize = 120;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposedPluginTool {
    pub external_name: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub tool_name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ContributedMcpToolAnnotations>,
}

fn app_state(app: &AppHandle) -> Result<tauri::State<'_, AppState>, String> {
    app.try_state::<AppState>()
        .ok_or_else(|| "app state unavailable".to_string())
}

fn enabled_trusted_row(
    app_state: &AppState,
    plugin_id: &str,
) -> Result<InstalledPluginRow, String> {
    let conn = app_state.db.lock();
    ensure_plugin_tables(&conn)?;
    let row = list_installed_plugins(&conn)?
        .into_iter()
        .find(|row| row.id == plugin_id)
        .ok_or_else(|| "plugin not found".to_string())?;
    if !row.enabled || !row.trusted {
        return Err("plugin is not enabled/trusted".into());
    }
    Ok(row)
}

fn read_current_manifest(
    app: &AppHandle,
    row: &InstalledPluginRow,
) -> Result<PluginManifest, String> {
    let manifest_path = packages_dir(app)?
        .join(&row.id)
        .join(&row.current_version)
        .join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("read manifest: {error}"))?;
    PluginManifest::parse_str(&raw)
}

pub fn external_tool_name(plugin_id: &str, tool_name: &str) -> String {
    let readable = format!(
        "{EXTERNAL_TOOL_PREFIX}{}__{}",
        normalize_name_component(plugin_id),
        normalize_name_component(tool_name)
    );
    if readable.len() <= MAX_EXTERNAL_TOOL_NAME_BYTES {
        return readable;
    }

    let digest = hex::encode(Sha256::digest(
        format!("{plugin_id}\0{tool_name}").as_bytes(),
    ));
    let suffix = format!("__{}", &digest[..16]);
    let keep = MAX_EXTERNAL_TOOL_NAME_BYTES.saturating_sub(suffix.len());
    format!("{}{}", &readable[..keep], suffix)
}

fn normalize_name_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Build a fail-closed snapshot of every currently callable plugin tool.
pub fn list_exposed_tools(app: &AppHandle) -> Result<Vec<ExposedPluginTool>, String> {
    let state = app_state(app)?;
    let rows = {
        let conn = state.db.lock();
        ensure_plugin_tables(&conn)?;
        list_installed_plugins(&conn)?
    };

    let mut external_names = HashSet::new();
    let mut output = Vec::new();
    for row in rows {
        if !row.enabled || !row.trusted {
            continue;
        }
        let manifest = match read_current_manifest(app, &row) {
            Ok(manifest) => manifest,
            Err(error) => {
                tracing::warn!(plugin_id = %row.id, error = %error, "skip invalid plugin MCP manifest");
                continue;
            }
        };
        let fingerprint = match manifest.mcp_toolset_fingerprint() {
            Ok(fingerprint) => fingerprint,
            Err(error) => {
                tracing::warn!(plugin_id = %row.id, error = %error, "skip plugin MCP fingerprint failure");
                continue;
            }
        };
        let exposed = {
            let conn = state.db.lock();
            is_plugin_mcp_exposed(&conn, &row.id, &fingerprint)
        };
        if !exposed {
            continue;
        }

        let disabled = {
            let conn = state.db.lock();
            super::trust::get_plugin_mcp_disabled_tools(&conn, &row.id).unwrap_or_default()
        };

        for tool in &manifest.contributes.mcp_tools {
            if disabled.iter().any(|name| name == &tool.name) {
                continue;
            }
            let external_name = external_tool_name(&row.id, &tool.name);
            if !external_names.insert(external_name.clone()) {
                tracing::warn!(
                    plugin_id = %row.id,
                    tool_name = %tool.name,
                    external_name = %external_name,
                    "skip colliding plugin MCP tool name"
                );
                continue;
            }
            output.push(ExposedPluginTool {
                external_name,
                plugin_id: row.id.clone(),
                plugin_name: manifest.name.clone(),
                tool_name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
                output_schema: tool.output_schema.clone(),
                annotations: tool.annotations.clone(),
            });
        }
    }
    output.sort_by(|left, right| left.external_name.cmp(&right.external_name));
    Ok(output)
}

pub fn get_exposed_tool(
    app: &AppHandle,
    external_name: &str,
) -> Result<Option<ExposedPluginTool>, String> {
    Ok(list_exposed_tools(app)?
        .into_iter()
        .find(|tool| tool.external_name == external_name))
}

pub async fn call_exposed_tool_by_external_name(
    app: &AppHandle,
    external_name: &str,
    arguments: Value,
) -> Result<Value, String> {
    let tool = get_exposed_tool(app, external_name)?
        .ok_or_else(|| format!("plugin MCP tool not found: {external_name}"))?;
    call_exposed_tool(app, &tool.plugin_id, &tool.tool_name, arguments).await
}

/// Backward-compatible local-name entry used by the legacy MCP meta-tool.
pub async fn call_exposed_tool(
    app: &AppHandle,
    plugin_id: &str,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, String> {
    let started = Instant::now();
    let external_name = external_tool_name(plugin_id, tool_name);
    let result = call_exposed_tool_inner(app, plugin_id, tool_name, arguments).await;
    let elapsed_ms = started.elapsed().as_millis();
    match &result {
        Ok(_) => tracing::info!(
            plugin_id = %plugin_id,
            tool_name = %tool_name,
            external_name = %external_name,
            ok = true,
            elapsed_ms,
            "plugin MCP tool call"
        ),
        Err(error) => tracing::warn!(
            plugin_id = %plugin_id,
            tool_name = %tool_name,
            external_name = %external_name,
            ok = false,
            elapsed_ms,
            error = %error,
            "plugin MCP tool call"
        ),
    }
    result
}

async fn call_exposed_tool_inner(
    app: &AppHandle,
    plugin_id: &str,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, String> {
    let state = app_state(app)?;
    let host = app
        .try_state::<Arc<PluginHost>>()
        .ok_or_else(|| "plugin host unavailable".to_string())?
        .inner()
        .clone();
    let row = enabled_trusted_row(&state, plugin_id)?;
    let manifest = read_current_manifest(app, &row)?;
    let fingerprint = manifest.mcp_toolset_fingerprint()?;
    {
        let conn = state.db.lock();
        if !is_plugin_mcp_exposed(&conn, plugin_id, &fingerprint) {
            return Err("plugin MCP approval is missing or stale".into());
        }
        if !is_plugin_mcp_tool_enabled(&conn, plugin_id, tool_name) {
            return Err(format!("plugin MCP tool is disabled: {tool_name}"));
        }
    }
    let tool = manifest
        .contributes
        .mcp_tools
        .iter()
        .find(|tool| tool.name == tool_name)
        .ok_or_else(|| format!("plugin does not expose MCP tool {tool_name}"))?;

    let arguments = if arguments.is_null() {
        json!({})
    } else {
        arguments
    };
    validate_instance(&tool.input_schema, &arguments, "input")?;
    let result = invoke_runtime_mcp_tool(&host, plugin_id, &tool.name, arguments, DEFAULT_TIMEOUT)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(output_schema) = &tool.output_schema {
        validate_instance(output_schema, &result, "output")?;
    }
    Ok(result)
}

fn validate_instance(schema: &Value, instance: &Value, direction: &str) -> Result<(), String> {
    let validator = jsonschema::validator_for(schema)
        .map_err(|error| format!("plugin MCP {direction} schema is invalid: {error}"))?;
    let messages: Vec<_> = validator
        .iter_errors(instance)
        .take(3)
        .map(|error| {
            let path = error.instance_path().to_string();
            let path = if path.is_empty() { "/" } else { path.as_str() };
            format!("{path}: {}", error.masked())
        })
        .collect();
    if messages.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "plugin MCP {direction} does not match schema: {}",
            messages.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_names_are_stable_and_client_safe() {
        assert_eq!(
            external_tool_name("com.example.hello", "say-hello"),
            "tempo_plugin_com_example_hello__say_hello"
        );
        let long = external_tool_name(&format!("com.example.{}", "a".repeat(110)), &"b".repeat(64));
        assert!(long.len() <= MAX_EXTERNAL_TOOL_NAME_BYTES);
        assert!(long.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        }));
    }

    #[test]
    fn schema_errors_do_not_echo_argument_values() {
        let schema = json!({
            "type": "object",
            "properties": { "token": { "type": "integer" } },
            "required": ["token"]
        });
        let error =
            validate_instance(&schema, &json!({ "token": "secret-value" }), "input").unwrap_err();
        assert!(error.contains("/token"));
        assert!(!error.contains("secret-value"));
    }
}
