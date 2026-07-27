//! Per-builtin-app MCP exposure preferences.
//!
//! Builtin tools live on the static MCP ToolRouter. Exposure defaults to on (legacy behavior);
//! users can disable an entire builtin or individual tools from plugin settings.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::db::{current_storage_dir, default_storage_dir, AppState};

#[derive(Debug, Clone, Copy)]
pub struct BuiltinMcpToolDef {
    pub name: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinMcpToolInfo {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

const TODO_TOOLS: &[BuiltinMcpToolDef] = &[
    BuiltinMcpToolDef {
        name: "list_todos",
        description: "列出待办（轻量列表）",
    },
    BuiltinMcpToolDef {
        name: "get_todo",
        description: "按 id 获取待办详情",
    },
    BuiltinMcpToolDef {
        name: "create_todo",
        description: "创建待办",
    },
    BuiltinMcpToolDef {
        name: "update_todo",
        description: "更新待办",
    },
    BuiltinMcpToolDef {
        name: "complete_todo",
        description: "完成 / 重新打开待办",
    },
    BuiltinMcpToolDef {
        name: "pin_todo",
        description: "置顶 / 取消置顶待办",
    },
    BuiltinMcpToolDef {
        name: "delete_todo",
        description: "删除待办",
    },
    BuiltinMcpToolDef {
        name: "add_todo_subtask",
        description: "添加子任务",
    },
    BuiltinMcpToolDef {
        name: "add_todo_note",
        description: "添加待办备注",
    },
];

const SNIPPET_TOOLS: &[BuiltinMcpToolDef] = &[
    BuiltinMcpToolDef {
        name: "list_snippets",
        description: "搜索 / 列出快捷短语",
    },
    BuiltinMcpToolDef {
        name: "list_snippet_groups",
        description: "列出快捷短语分组",
    },
    BuiltinMcpToolDef {
        name: "create_snippet",
        description: "创建快捷短语",
    },
    BuiltinMcpToolDef {
        name: "update_snippet",
        description: "更新快捷短语",
    },
    BuiltinMcpToolDef {
        name: "delete_snippet",
        description: "删除快捷短语",
    },
    BuiltinMcpToolDef {
        name: "create_snippet_group",
        description: "创建快捷短语分组",
    },
    BuiltinMcpToolDef {
        name: "copy_snippet_to_clipboard",
        description: "复制快捷短语到剪贴板",
    },
];

const CLIPBOARD_TOOLS: &[BuiltinMcpToolDef] = &[BuiltinMcpToolDef {
    name: "list_clipboard",
    description: "搜索剪贴板历史",
}];

const REPORTS_TOOLS: &[BuiltinMcpToolDef] = &[BuiltinMcpToolDef {
    name: "get_daily_report",
    description: "读取今日屏幕使用报告",
}];

pub fn builtin_mcp_tools_for_app(app_id: &str) -> &'static [BuiltinMcpToolDef] {
    match app_id {
        "todo" => TODO_TOOLS,
        "snippets" => SNIPPET_TOOLS,
        "clipboard" => CLIPBOARD_TOOLS,
        "reports" => REPORTS_TOOLS,
        _ => &[],
    }
}

pub fn builtin_app_id_for_tool(tool_name: &str) -> Option<&'static str> {
    for (app_id, tools) in [
        ("todo", TODO_TOOLS),
        ("snippets", SNIPPET_TOOLS),
        ("clipboard", CLIPBOARD_TOOLS),
        ("reports", REPORTS_TOOLS),
    ] {
        if tools.iter().any(|tool| tool.name == tool_name) {
            return Some(app_id);
        }
    }
    None
}

fn storage_root(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    current_storage_dir(app).or_else(|_| default_storage_dir(app))
}

pub fn builtin_data_dir(app: &AppHandle, app_id: &str) -> Result<std::path::PathBuf, String> {
    Ok(storage_root(app)?.join("builtins").join(app_id))
}

pub fn ensure_builtin_mcp_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS builtin_mcp_exposure (
           app_id TEXT PRIMARY KEY,
           exposed INTEGER NOT NULL DEFAULT 1,
           disabled_tools TEXT NOT NULL DEFAULT '[]',
           updated_at TEXT NOT NULL
         );",
    )
    .map_err(|error| format!("create builtin_mcp_exposure: {error}"))?;
    Ok(())
}

fn parse_tool_names(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn serialize_tool_names(tools: &[String]) -> Result<String, String> {
    serde_json::to_string(tools).map_err(|error| format!("serialize tool names: {error}"))
}

pub fn get_builtin_mcp_disabled_tools(
    conn: &Connection,
    app_id: &str,
) -> Result<Vec<String>, String> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT disabled_tools FROM builtin_mcp_exposure WHERE app_id = ?1",
            params![app_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("read builtin disabled_tools: {e}"))?;
    Ok(raw.map(|value| parse_tool_names(&value)).unwrap_or_default())
}

/// Missing row means exposed (legacy: all builtin tools were always available).
pub fn is_builtin_mcp_exposed(conn: &Connection, app_id: &str) -> bool {
    conn.query_row(
        "SELECT exposed FROM builtin_mcp_exposure WHERE app_id = ?1",
        params![app_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|exposed| exposed != 0)
    .unwrap_or(true)
}

pub fn is_builtin_mcp_tool_enabled(conn: &Connection, app_id: &str, tool_name: &str) -> bool {
    get_builtin_mcp_disabled_tools(conn, app_id)
        .map(|disabled| !disabled.iter().any(|name| name == tool_name))
        .unwrap_or(true)
}

pub fn set_builtin_mcp_exposed(
    conn: &Connection,
    app_id: &str,
    exposed: bool,
) -> Result<(), String> {
    ensure_builtin_mcp_tables(conn)?;
    let now = chrono::Local::now().to_rfc3339();
    let disabled = get_builtin_mcp_disabled_tools(conn, app_id)?;
    let disabled_json = serialize_tool_names(&disabled)?;
    conn.execute(
        "INSERT INTO builtin_mcp_exposure
           (app_id, exposed, disabled_tools, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(app_id) DO UPDATE SET
           exposed = excluded.exposed,
           updated_at = excluded.updated_at",
        params![app_id, exposed as i64, disabled_json, now],
    )
    .map_err(|e| format!("upsert builtin_mcp_exposure: {e}"))?;
    Ok(())
}

pub fn set_builtin_mcp_tool_enabled(
    conn: &Connection,
    app_id: &str,
    tool_name: &str,
    enabled: bool,
) -> Result<(), String> {
    ensure_builtin_mcp_tables(conn)?;
    let now = chrono::Local::now().to_rfc3339();
    let exposed = is_builtin_mcp_exposed(conn, app_id);
    let mut disabled = get_builtin_mcp_disabled_tools(conn, app_id)?;
    if enabled {
        disabled.retain(|name| name != tool_name);
    } else if !disabled.iter().any(|name| name == tool_name) {
        disabled.push(tool_name.to_string());
    }
    let disabled_json = serialize_tool_names(&disabled)?;
    conn.execute(
        "INSERT INTO builtin_mcp_exposure
           (app_id, exposed, disabled_tools, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(app_id) DO UPDATE SET
           disabled_tools = excluded.disabled_tools,
           updated_at = excluded.updated_at",
        params![app_id, exposed as i64, disabled_json, now],
    )
    .map_err(|e| format!("upsert builtin_mcp_tool_enabled: {e}"))?;
    Ok(())
}

pub fn list_builtin_mcp_tool_infos(
    conn: &Connection,
    app_id: &str,
) -> Result<Vec<BuiltinMcpToolInfo>, String> {
    ensure_builtin_mcp_tables(conn)?;
    let disabled = get_builtin_mcp_disabled_tools(conn, app_id)?;
    Ok(builtin_mcp_tools_for_app(app_id)
        .iter()
        .map(|tool| BuiltinMcpToolInfo {
            name: tool.name.to_string(),
            description: tool.description.to_string(),
            enabled: !disabled.iter().any(|name| name == tool.name),
        })
        .collect())
}

/// Whether a static ToolRouter tool may be listed / called under current preferences.
pub fn is_static_mcp_tool_allowed(app: &AppHandle, tool_name: &str) -> bool {
    let Some(app_id) = builtin_app_id_for_tool(tool_name) else {
        // Platform meta-tools (tempo_*) stay available whenever MCP is on.
        return true;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let conn = state.db.lock();
    let _ = ensure_builtin_mcp_tables(&conn);
    if !is_builtin_mcp_exposed(&conn, app_id) {
        return false;
    }
    is_builtin_mcp_tool_enabled(&conn, app_id, tool_name)
}
