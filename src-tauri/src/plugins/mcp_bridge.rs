//! Phase 2 MCP dynamic exposure (design §11, §13 Phase 2 "MCP 动态挂载 + 暴露开关").
//!
//! Plugin `contributes.mcpTools` are never auto-registered as MCP tools. Instead, the user
//! opts a plugin in per-package (`plugin_mcp_exposure`, `commands::plugins::set_plugin_mcp_exposed`)
//! and the two `tempo_list_exposed_plugin_tools` / `tempo_call_plugin_tool` meta-tools in
//! `mcp/server.rs` call into this module to discover and invoke whatever the user exposed.
//! This is deliberately a thin wrapper — MCP calls go through the exact same Supervisor RPC
//! path (timeout, concurrency limit, cancellation) as any other Runtime call (design §11).

use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::db::AppState;

use super::bridge::DEFAULT_TIMEOUT;
use super::host::PluginHost;
use super::manifest::PluginManifest;
use super::paths::packages_dir;
use super::trust::{ensure_plugin_tables, is_plugin_mcp_exposed, list_installed_plugins, InstalledPluginRow};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposedPluginTool {
    pub plugin_id: String,
    pub plugin_name: String,
    pub tool_name: String,
    pub description: String,
    pub input_schema: Value,
}

fn app_state(app: &AppHandle) -> Result<tauri::State<'_, AppState>, String> {
    app.try_state::<AppState>().ok_or_else(|| "app state unavailable".to_string())
}

fn enabled_trusted_exposed_row(
    app_state: &AppState,
    plugin_id: &str,
) -> Result<InstalledPluginRow, String> {
    let conn = app_state.db.lock();
    ensure_plugin_tables(&conn)?;
    let row = list_installed_plugins(&conn)?
        .into_iter()
        .find(|r| r.id == plugin_id)
        .ok_or_else(|| "plugin not found".to_string())?;
    if !row.enabled || !row.trusted {
        return Err("plugin is not enabled/trusted".into());
    }
    if !is_plugin_mcp_exposed(&conn, plugin_id) {
        return Err("plugin has not opted into MCP exposure".into());
    }
    Ok(row)
}

/// Every tool contributed by every enabled+trusted+MCP-exposed plugin (design §11: "用户查看
/// 工具清单后按插件开启" — this *is* that清单, for whichever MCP caller asks first).
pub fn list_exposed_tools(app: &AppHandle) -> Result<Vec<ExposedPluginTool>, String> {
    let state = app_state(app)?;
    let packages_root = packages_dir(app)?;

    let rows = {
        let conn = state.db.lock();
        ensure_plugin_tables(&conn)?;
        list_installed_plugins(&conn)?
    };

    let mut out = Vec::new();
    for row in rows {
        if !row.enabled || !row.trusted {
            continue;
        }
        let exposed = {
            let conn = state.db.lock();
            is_plugin_mcp_exposed(&conn, &row.id)
        };
        if !exposed {
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
        for tool in &manifest.contributes.mcp_tools {
            out.push(ExposedPluginTool {
                plugin_id: row.id.clone(),
                plugin_name: manifest.name.clone(),
                tool_name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
            });
        }
    }
    Ok(out)
}

/// Resolve `tool_name` (a manifest-local `contributes.mcpTools[].name`) to its backing command
/// and invoke it through the Supervisor. Audits every call (tool, plugin_id, ok/err, duration)
/// via `tracing` — never the arguments themselves, which may contain user data.
pub async fn call_exposed_tool(
    app: &AppHandle,
    plugin_id: &str,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, String> {
    let started = Instant::now();
    let result = call_exposed_tool_inner(app, plugin_id, tool_name, arguments).await;
    let elapsed_ms = started.elapsed().as_millis();
    match &result {
        Ok(_) => tracing::info!(
            tool = "tempo_call_plugin_tool",
            plugin_id = %plugin_id,
            tool_name = %tool_name,
            ok = true,
            elapsed_ms,
            "plugin mcp tool call"
        ),
        Err(error) => tracing::warn!(
            tool = "tempo_call_plugin_tool",
            plugin_id = %plugin_id,
            tool_name = %tool_name,
            ok = false,
            elapsed_ms,
            error = %error,
            "plugin mcp tool call"
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

    let row = enabled_trusted_exposed_row(&state, plugin_id)?;

    let install_path = packages_dir(app)?.join(plugin_id).join(&row.current_version);
    let manifest_path = install_path.join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest: {e}"))?;
    let manifest = PluginManifest::parse_str(&raw)?;
    let tool = manifest
        .contributes
        .mcp_tools
        .iter()
        .find(|t| t.name == tool_name)
        .ok_or_else(|| format!("plugin does not expose mcp tool {tool_name}"))?;

    host.supervisor
        .call(plugin_id, &tool.command, arguments, DEFAULT_TIMEOUT)
        .await
        .map_err(|error| error.to_string())
}
