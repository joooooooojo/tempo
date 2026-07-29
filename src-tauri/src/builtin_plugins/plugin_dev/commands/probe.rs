use std::sync::Arc;
use std::time::Duration;
use serde_json::{json, Value};
use tauri::State;

use crate::plugins::host::PluginHost;

use super::types::*;

pub(super) fn validate_loopback_url(raw: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|error| format!("无效服务 URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("服务 URL 只支持 http 或 https".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "服务 URL 缺少 host".to_string())?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
        || host == "[::1]";
    if !loopback {
        return Err("开发服务 URL 必须使用 localhost、127.0.0.1 或 [::1]".into());
    }
    Ok(url)
}


#[tauri::command]
pub async fn plugin_dev_probe_ui_url(args: ProbeUiUrlArgs) -> Result<ProbeUiUrlResult, String> {
    let url = validate_loopback_url(&args.url)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| format!("创建连接检测失败: {error}"))?;
    match client.get(url).send().await {
        Ok(response) => Ok(ProbeUiUrlResult {
            reachable: true,
            status: Some(response.status().as_u16()),
            message: format!("服务已响应 HTTP {}", response.status().as_u16()),
        }),
        Err(error) => Ok(ProbeUiUrlResult {
            reachable: false,
            status: None,
            message: format!("服务暂不可达: {error}"),
        }),
    }
}


#[tauri::command]
pub async fn plugin_dev_simulate_hook(
    host: State<'_, Arc<PluginHost>>,
    args: RunHookArgs,
) -> Result<Value, String> {
    let entry = host
        .development_plugins()
        .into_iter()
        .find(|candidate| candidate.project_id == args.project_id)
        .ok_or_else(|| "项目尚未连接到 Tempo".to_string())?;
    let hooks = entry
        .manifest
        .contributes
        .hooks
        .iter()
        .filter(|hook| hook.event == args.event)
        .collect::<Vec<_>>();
    if hooks.is_empty() {
        return Err(format!("Manifest 未声明 Hook 事件 {}", args.event));
    }
    let mut results = Vec::with_capacity(hooks.len());
    for hook in hooks {
        let result = host
            .supervisor
            .call(
                &entry.manifest.id,
                &hook.command,
                args.payload.clone(),
                crate::plugins::bridge::DEFAULT_TIMEOUT,
            )
            .await
            .map_err(|error| error.message)?;
        results.push(json!({ "command": hook.command, "result": result }));
    }
    Ok(Value::Array(results))
}

#[tauri::command]
pub async fn plugin_dev_run_mcp_tool(
    host: State<'_, Arc<PluginHost>>,
    args: RunMcpToolArgs,
) -> Result<Value, String> {
    let entry = host
        .development_plugins()
        .into_iter()
        .find(|candidate| candidate.project_id == args.project_id)
        .ok_or_else(|| "项目尚未连接到 Tempo".to_string())?;
    let tool = entry
        .manifest
        .contributes
        .mcp_tools
        .iter()
        .find(|tool| tool.name == args.tool_name)
        .ok_or_else(|| format!("Manifest 未声明 MCP Tool {}", args.tool_name))?;
    let input_validator = jsonschema::validator_for(&tool.input_schema)
        .map_err(|error| format!("MCP inputSchema 无效: {error}"))?;
    input_validator
        .validate(&args.arguments)
        .map_err(|error| format!("MCP 输入不符合 Schema: {error}"))?;
    let result = host
        .supervisor
        .call(
            &entry.manifest.id,
            &tool.command,
            args.arguments,
            crate::plugins::bridge::DEFAULT_TIMEOUT,
        )
        .await
        .map_err(|error| error.message)?;
    if let Some(schema) = &tool.output_schema {
        let output_validator = jsonschema::validator_for(schema)
            .map_err(|error| format!("MCP outputSchema 无效: {error}"))?;
        output_validator
            .validate(&result)
            .map_err(|error| format!("MCP 输出不符合 Schema: {error}"))?;
    }
    Ok(result)
}

