//! Builtin app settings helpers (MCP exposure + private data directory).

use tauri::{AppHandle, State};

use crate::db::AppState;
use crate::mcp::{
    builtin_data_dir, builtin_mcp_tools_for_app, ensure_builtin_mcp_tables,
    is_builtin_mcp_exposed, list_builtin_mcp_tool_infos,
    set_builtin_mcp_exposed as store_builtin_mcp_exposed,
    set_builtin_mcp_tool_enabled as store_builtin_mcp_tool_enabled, BuiltinMcpToolInfo,
};

fn validate_builtin_app_id(app_id: &str) -> Result<(), String> {
    if app_id.trim().is_empty() || app_id == "settings" {
        return Err("invalid builtin app id".into());
    }
    Ok(())
}

#[tauri::command]
pub fn list_builtin_mcp_tools(
    state: State<'_, AppState>,
    app_id: String,
) -> Result<Vec<BuiltinMcpToolInfo>, String> {
    validate_builtin_app_id(&app_id)?;
    let conn = state.db.lock();
    ensure_builtin_mcp_tables(&conn)?;
    list_builtin_mcp_tool_infos(&conn, &app_id)
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinMcpStatus {
    pub exposed: bool,
    pub tool_count: usize,
    pub enabled_tool_count: usize,
}

#[tauri::command]
pub fn get_builtin_mcp_status(
    state: State<'_, AppState>,
    app_id: String,
) -> Result<BuiltinMcpStatus, String> {
    validate_builtin_app_id(&app_id)?;
    let conn = state.db.lock();
    ensure_builtin_mcp_tables(&conn)?;
    let tools = list_builtin_mcp_tool_infos(&conn, &app_id)?;
    let tool_count = tools.len();
    let enabled_tool_count = tools.iter().filter(|tool| tool.enabled).count();
    let exposed = is_builtin_mcp_exposed(&conn, &app_id);
    Ok(BuiltinMcpStatus {
        exposed,
        tool_count,
        enabled_tool_count,
    })
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBuiltinMcpExposedArgs {
    pub app_id: String,
    pub exposed: bool,
}

#[tauri::command]
pub fn set_builtin_mcp_exposed(
    app: AppHandle,
    state: State<'_, AppState>,
    args: SetBuiltinMcpExposedArgs,
) -> Result<(), String> {
    validate_builtin_app_id(&args.app_id)?;
    if builtin_mcp_tools_for_app(&args.app_id).is_empty() {
        return Err("builtin app does not declare MCP tools".into());
    }
    let conn = state.db.lock();
    ensure_builtin_mcp_tables(&conn)?;
    store_builtin_mcp_exposed(&conn, &args.app_id, args.exposed)?;
    drop(conn);
    crate::mcp::notify_plugin_tools_changed(&app);
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBuiltinMcpToolEnabledArgs {
    pub app_id: String,
    pub tool_name: String,
    pub enabled: bool,
}

#[tauri::command]
pub fn set_builtin_mcp_tool_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    args: SetBuiltinMcpToolEnabledArgs,
) -> Result<(), String> {
    validate_builtin_app_id(&args.app_id)?;
    if !builtin_mcp_tools_for_app(&args.app_id)
        .iter()
        .any(|tool| tool.name == args.tool_name)
    {
        return Err(format!(
            "builtin app does not declare MCP tool {}",
            args.tool_name
        ));
    }
    let conn = state.db.lock();
    ensure_builtin_mcp_tables(&conn)?;
    store_builtin_mcp_tool_enabled(&conn, &args.app_id, &args.tool_name, args.enabled)?;
    drop(conn);
    crate::mcp::notify_plugin_tools_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn builtin_open_data_dir(app: AppHandle, app_id: String) -> Result<(), String> {
    validate_builtin_app_id(&app_id)?;
    let dir = builtin_data_dir(&app, &app_id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create builtin data dir: {e}"))?;
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(dir.display().to_string(), None::<String>)
        .map_err(|e| format!("open builtin data dir: {e}"))
}
