use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::db::AppState;

fn json_result<T: serde::Serialize>(value: T) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(&value)
        .map_err(|e| McpError::internal_error(format!("serialize tool result: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn tool_err(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
}

fn plugin_tool_model(
    tool: crate::plugins::mcp_bridge::ExposedPluginTool,
) -> Result<Tool, McpError> {
    let input_schema = tool.input_schema.as_object().cloned().ok_or_else(|| {
        McpError::internal_error("plugin MCP input schema is not an object", None)
    })?;
    let title = format!("{} · {}", tool.plugin_name, tool.tool_name);
    let description = format!("[{}] {}", tool.plugin_name, tool.description);
    let mut model = Tool::new(tool.external_name, description, Arc::new(input_schema))
        .with_title(title.clone());
    if let Some(output_schema) = tool.output_schema {
        let output_schema = output_schema.as_object().cloned().ok_or_else(|| {
            McpError::internal_error("plugin MCP output schema is not an object", None)
        })?;
        model = model.with_raw_output_schema(Arc::new(output_schema));
    }
    if let Some(annotations) = tool.annotations {
        model = model.with_annotations(ToolAnnotations::from_raw(
            Some(title),
            annotations.read_only_hint,
            annotations.destructive_hint,
            annotations.idempotent_hint,
            annotations.open_world_hint,
        ));
    }
    Ok(model)
}

fn list_plugin_tool_models(app: &AppHandle) -> Result<Vec<Tool>, McpError> {
    crate::plugins::mcp_bridge::list_exposed_tools(app)
        .map_err(|error| {
            tracing::warn!(error = %error, "build plugin MCP registry snapshot failed");
            McpError::internal_error("plugin MCP registry unavailable", None)
        })?
        .into_iter()
        .map(plugin_tool_model)
        .collect()
}

fn app_state(app: &AppHandle) -> Result<tauri::State<'_, AppState>, CallToolResult> {
    app.try_state::<AppState>()
        .ok_or_else(|| tool_err("AppState unavailable; is Tempo fully started?"))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IdArgs {
    #[schemars(description = "Resource id (todo or snippet, depending on the tool)")]
    pub id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTodoArgs {
    #[schemars(description = "Todo title / 待办标题")]
    pub title: String,
    #[serde(default)]
    #[schemars(description = "Optional markdown body / 详情内容")]
    pub content: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional due datetime in RFC3339, e.g. 2026-07-15T18:00:00+08:00")]
    pub due_at: Option<String>,
    #[serde(default)]
    #[schemars(description = "Recurrence: none | daily | weekly | monthly")]
    pub recurrence: Option<String>,
    #[serde(default)]
    #[schemars(description = "Remind 1 day before due")]
    pub remind_1d: Option<bool>,
    #[serde(default)]
    #[schemars(description = "Remind 1 hour before due")]
    pub remind_1h: Option<bool>,
    #[serde(default)]
    #[schemars(description = "Custom reminder hours before due (e.g. 3 = 3 hours before)")]
    pub remind_custom_hours: Option<i64>,
    #[serde(default)]
    #[schemars(description = "Optional initial subtask titles / 子任务标题列表")]
    pub subtasks: Option<Vec<String>>,
    #[serde(default)]
    #[schemars(description = "Optional tags / 标签")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTodoArgs {
    #[schemars(description = "Todo id to update")]
    pub id: i64,
    #[schemars(description = "New title / 待办标题")]
    pub title: String,
    #[serde(default)]
    #[schemars(description = "New markdown body (empty string clears content)")]
    pub content: String,
    #[serde(default)]
    #[schemars(description = "Optional due datetime in RFC3339, e.g. 2026-07-15T18:00:00+08:00")]
    pub due_at: Option<String>,
    #[serde(default)]
    #[schemars(description = "Recurrence: none | daily | weekly | monthly")]
    pub recurrence: Option<String>,
    #[serde(default)]
    #[schemars(description = "Remind 1 day before due")]
    pub remind_1d: Option<bool>,
    #[serde(default)]
    #[schemars(description = "Remind 1 hour before due")]
    pub remind_1h: Option<bool>,
    #[serde(default)]
    #[schemars(description = "Custom reminder hours before due")]
    pub remind_custom_hours: Option<i64>,
    #[serde(default)]
    #[schemars(description = "Replacement tags list / 标签")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CompleteTodoArgs {
    #[schemars(description = "Todo id")]
    pub id: i64,
    #[schemars(description = "true = mark completed / 完成; false = reopen")]
    pub completed: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PinTodoArgs {
    #[schemars(description = "Todo id")]
    pub id: i64,
    #[schemars(description = "true = pin / 置顶; false = unpin")]
    pub pinned: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddSubtaskArgs {
    #[schemars(description = "Parent todo id")]
    pub todo_id: i64,
    #[schemars(description = "Subtask title / 子任务标题")]
    pub title: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddNoteArgs {
    #[schemars(description = "Todo id to attach the note to")]
    pub todo_id: i64,
    #[schemars(description = "Note text / 备注内容")]
    pub body: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListSnippetsArgs {
    #[serde(default)]
    #[schemars(description = "Optional search query over title/content / 搜索关键词")]
    pub query: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional snippet group id to filter by")]
    pub group_id: Option<i64>,
    #[serde(default)]
    #[schemars(description = "Optional sort order (app-defined string, e.g. recent or title)")]
    pub sort: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateSnippetArgs {
    #[schemars(description = "Snippet title / 快捷短语标题")]
    pub title: String,
    #[schemars(description = "Snippet body text to insert or copy")]
    pub content: String,
    #[serde(default)]
    #[schemars(description = "Optional tags / 标签")]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    #[schemars(description = "Optional group id to place the snippet in")]
    pub group_id: Option<i64>,
    #[serde(default)]
    #[schemars(description = "Optional keyboard shortcut / 快捷键")]
    pub shortcut: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional language hint for highlighting (e.g. markdown, typescript)"
    )]
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateSnippetArgs {
    #[schemars(description = "Snippet id to update")]
    pub id: i64,
    #[schemars(description = "New title / 快捷短语标题")]
    pub title: String,
    #[schemars(description = "New body text")]
    pub content: String,
    #[serde(default)]
    #[schemars(description = "Replacement tags list / 标签")]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    #[schemars(description = "Optional group id")]
    pub group_id: Option<i64>,
    #[serde(default)]
    #[schemars(description = "Optional keyboard shortcut / 快捷键")]
    pub shortcut: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional language hint for highlighting")]
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateSnippetGroupArgs {
    #[schemars(description = "Group name / 分组名称")]
    pub name: String,
    #[serde(default)]
    #[schemars(description = "Optional color (hex or app color token)")]
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListClipboardArgs {
    #[serde(default)]
    #[schemars(description = "Optional search query over clipboard text / 剪贴板搜索")]
    pub query: Option<String>,
    #[serde(default)]
    #[schemars(description = "Max number of entries to return (default is app-defined)")]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DailyReportArgs {
    #[serde(default)]
    #[schemars(description = "Optional date as YYYY-MM-DD; defaults to today")]
    pub date: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CallPluginToolArgs {
    #[schemars(
        description = "Plugin package id, e.g. com.example.hello (see tempo_list_exposed_plugin_tools)"
    )]
    pub plugin_id: String,
    #[schemars(
        description = "The tool's local name from contributes.mcpTools[].name (see tempo_list_exposed_plugin_tools)"
    )]
    pub tool_name: String,
    #[serde(default)]
    #[schemars(description = "Arguments matching the tool's inputSchema; defaults to {}")]
    pub arguments: serde_json::Value,
}

#[derive(Clone)]
pub struct TempoMcpServer {
    app: AppHandle,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl TempoMcpServer {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "List Tempo todos (lightweight, no full note/image payloads). Use when the user asks about todos, tasks, 待办, 任务列表, or what's on their list"
    )]
    fn list_todos(&self) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::todo::commands::get_todos(self.app.clone(), state) {
            Ok(todos) => json_result(todos),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Get one Tempo todo by id with full details (notes, subtasks). Use when the user asks about a specific todo/待办详情"
    )]
    fn get_todo(&self, Parameters(args): Parameters<IdArgs>) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::todo::commands::get_todo(state, args.id) {
            Ok(todo) => json_result(todo),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Create a Tempo todo/待办. Use when the user wants to add a task. Optional: content (markdown), due_at (RFC3339), recurrence (none|daily|weekly|monthly), reminders, subtasks, tags"
    )]
    fn create_todo(
        &self,
        Parameters(args): Parameters<CreateTodoArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::todo::commands::add_todo(
            self.app.clone(),
            state,
            args.title,
            args.content,
            args.due_at,
            None,
            args.recurrence,
            args.remind_1d,
            args.remind_1h,
            args.remind_custom_hours,
            args.subtasks,
            args.tags,
        ) {
            Ok(todo) => json_result(todo),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Update a Tempo todo (title, content, due date, recurrence, reminders, tags). Use when editing/修改待办"
    )]
    fn update_todo(
        &self,
        Parameters(args): Parameters<UpdateTodoArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::todo::commands::update_todo_details(
            self.app.clone(),
            state,
            args.id,
            args.title,
            args.content,
            args.due_at,
            args.recurrence,
            args.remind_1d,
            args.remind_1h,
            args.remind_custom_hours,
            args.tags,
        ) {
            Ok(todo) => json_result(todo),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Mark a Tempo todo completed or incomplete. Use when the user finishes/完成 or reopens a task"
    )]
    fn complete_todo(
        &self,
        Parameters(args): Parameters<CompleteTodoArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::todo::commands::set_todo_completed(
            self.app.clone(),
            state,
            args.id,
            args.completed,
        ) {
            Ok(todo) => json_result(todo),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Pin or unpin a Tempo todo. Use when the user wants to pin/置顶 or unpin a task"
    )]
    fn pin_todo(
        &self,
        Parameters(args): Parameters<PinTodoArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::todo::commands::set_todo_pinned(self.app.clone(), state, args.id, args.pinned)
        {
            Ok(todo) => json_result(todo),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Delete a Tempo todo by id. Use when the user wants to remove/删除 a task"
    )]
    fn delete_todo(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::todo::commands::delete_todo(self.app.clone(), state, args.id) {
            Ok(()) => json_result(json!({ "deleted": true, "id": args.id })),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Add a subtask/子任务 to a Tempo todo. Use when breaking a task into smaller steps"
    )]
    fn add_todo_subtask(
        &self,
        Parameters(args): Parameters<AddSubtaskArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::todo::commands::add_todo_subtask(
            self.app.clone(),
            state,
            args.todo_id,
            args.title,
        ) {
            Ok(todo) => json_result(todo),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Add a text note/备注 to a Tempo todo. Use when appending notes or comments to a task"
    )]
    fn add_todo_note(
        &self,
        Parameters(args): Parameters<AddNoteArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::todo::commands::add_todo_note(
            self.app.clone(),
            state,
            args.todo_id,
            args.body,
            None,
        ) {
            Ok(todo) => json_result(todo),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "List Tempo quick phrases/snippets/快捷短语. Optional query, group_id, sort. Use when searching phrases, templates, or canned text"
    )]
    fn list_snippets(
        &self,
        Parameters(args): Parameters<ListSnippetsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        let snippets =
            crate::builtin_plugins::snippets::commands::get_snippets(state, args.query, args.group_id, args.sort);
        json_result(snippets)
    }

    #[tool(
        description = "List Tempo snippet groups/快捷短语分组. Use when organizing or browsing phrase categories"
    )]
    fn list_snippet_groups(&self) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        json_result(crate::builtin_plugins::snippets::commands::get_snippet_groups(state))
    }

    #[tool(
        description = "Create a Tempo quick phrase/snippet/快捷短语. Use when saving reusable text, templates, or canned replies"
    )]
    fn create_snippet(
        &self,
        Parameters(args): Parameters<CreateSnippetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::snippets::commands::create_snippet(
            self.app.clone(),
            state,
            args.title,
            args.content,
            args.tags.unwrap_or_default(),
            args.group_id,
            args.shortcut,
            args.language,
        ) {
            Ok(snippet) => json_result(snippet),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Update a Tempo quick phrase/snippet. Use when editing/修改快捷短语 content, tags, group, or shortcut"
    )]
    fn update_snippet(
        &self,
        Parameters(args): Parameters<UpdateSnippetArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::snippets::commands::update_snippet_command(
            self.app.clone(),
            state,
            args.id,
            args.title,
            args.content,
            args.tags.unwrap_or_default(),
            args.group_id,
            args.shortcut,
            args.language,
        ) {
            Ok(snippet) => json_result(snippet),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Delete a Tempo quick phrase/snippet by id. Use when removing/删除快捷短语"
    )]
    fn delete_snippet(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::snippets::commands::delete_snippet_command(self.app.clone(), state, args.id) {
            Ok(()) => json_result(json!({ "deleted": true, "id": args.id })),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Create a Tempo snippet group/快捷短语分组. Use when adding a new phrase category"
    )]
    fn create_snippet_group(
        &self,
        Parameters(args): Parameters<CreateSnippetGroupArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::snippets::commands::create_snippet_group(
            self.app.clone(),
            state,
            args.name,
            args.color,
        ) {
            Ok(group) => json_result(group),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Copy a Tempo snippet to the system clipboard. Use when the user wants to paste/复制 a saved quick phrase"
    )]
    fn copy_snippet_to_clipboard(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::builtin_plugins::snippets::commands::copy_snippet_to_clipboard(self.app.clone(), state, args.id)
        {
            Ok(snippet) => json_result(snippet),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Search Tempo clipboard history/剪贴板历史 (text entries; images summarized). Optional query and limit. Use when finding recently copied text"
    )]
    fn list_clipboard(
        &self,
        Parameters(args): Parameters<ListClipboardArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        let page = crate::builtin_plugins::clipboard::commands::get_clipboard_history(
            self.app.clone(),
            state,
            args.query,
            args.limit,
            Some(0),
        );
        // Shrink payload for AI: drop image data URLs
        let entries: Vec<_> = page
            .entries
            .into_iter()
            .map(|mut entry| {
                if entry.kind == "image" {
                    entry.content = "[image]".into();
                    entry.source_icon_data_url = None;
                }
                entry
            })
            .collect();
        json_result(json!({
            "total": page.total,
            "has_more": page.has_more,
            "entries": entries,
        }))
    }

    #[tool(
        description = "Get Tempo daily usage report/今日报告/屏幕时间. Optional date YYYY-MM-DD (defaults today). Use when asking how much time was spent on apps"
    )]
    fn get_daily_report(
        &self,
        Parameters(args): Parameters<DailyReportArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        let report = crate::builtin_plugins::reports::commands::get_daily_report(state, args.date);
        json_result(report)
    }

    #[tool(
        description = "List Tempo plugin tools the user has explicitly exposed to MCP/AI (design: plugins never auto-expose tools — this reflects only what each plugin's settings toggle currently allows). Call this before tempo_call_plugin_tool to discover valid plugin_id/tool_name pairs and their input schemas. Returns an empty list if no plugin has opted in."
    )]
    fn tempo_list_exposed_plugin_tools(&self) -> Result<CallToolResult, McpError> {
        match crate::plugins::mcp_bridge::list_exposed_tools(&self.app) {
            Ok(tools) => json_result(tools),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Call one tool contributed by a Tempo plugin, if the user has exposed it to MCP. Use tempo_list_exposed_plugin_tools first to find a valid plugin_id/tool_name and its inputSchema. Fails with an error if the plugin is not installed, enabled, trusted, or MCP-exposed, or if tool_name is unknown."
    )]
    async fn tempo_call_plugin_tool(
        &self,
        Parameters(args): Parameters<CallPluginToolArgs>,
    ) -> Result<CallToolResult, McpError> {
        match crate::plugins::mcp_bridge::call_exposed_tool(
            &self.app,
            &args.plugin_id,
            &args.tool_name,
            args.arguments,
        )
        .await
        {
            Ok(value) => json_result(value),
            Err(e) => Ok(tool_err(e)),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TempoMcpServer {
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let name = request.name.to_string();
        if self.tool_router.get(&name).is_some() {
            if !crate::mcp::is_static_mcp_tool_allowed(&self.app, &name) {
                return Err(McpError::invalid_params(
                    "builtin MCP tool is disabled",
                    None,
                ));
            }
            let context = ToolCallContext::new(self, request, context);
            return self.tool_router.call(context).await;
        }

        let plugin_tool =
            crate::plugins::mcp_bridge::get_exposed_tool(&self.app, &name).map_err(|error| {
                tracing::warn!(error = %error, tool_name = %name, "resolve plugin MCP tool failed");
                McpError::internal_error("plugin MCP registry unavailable", None)
            })?;
        if plugin_tool.is_none() {
            return Err(McpError::invalid_params("tool not found", None));
        }
        let arguments = serde_json::Value::Object(request.arguments.unwrap_or_default());
        match crate::plugins::mcp_bridge::call_exposed_tool_by_external_name(
            &self.app, &name, arguments,
        )
        .await
        {
            Ok(value) => Ok(CallToolResult::structured(value)),
            Err(error) => Ok(tool_err(error)),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.retain(|tool| crate::mcp::is_static_mcp_tool_allowed(&self.app, tool.name.as_ref()));
        tools.extend(list_plugin_tool_models(&self.app)?);
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        if let Some(tool) = self.tool_router.get(name) {
            if !crate::mcp::is_static_mcp_tool_allowed(&self.app, name) {
                return None;
            }
            return Some(tool.clone());
        }
        match crate::plugins::mcp_bridge::get_exposed_tool(&self.app, name) {
            Ok(Some(tool)) => plugin_tool_model(tool).ok(),
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(error = %error, tool_name = %name, "get plugin MCP tool failed");
                None
            }
        }
    }

    async fn on_initialized(&self, context: rmcp::service::NotificationContext<rmcp::RoleServer>) {
        let Some(controller) = self.app.try_state::<crate::mcp::McpController>() else {
            return;
        };
        let mut changes = controller.subscribe_tools_changed();
        let peer = context.peer;
        tokio::spawn(async move {
            while let Ok(()) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) =
                changes.recv().await
            {
                if peer.notify_tool_list_changed().await.is_err() {
                    break;
                }
            }
        });
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
            .with_server_info(Implementation::new("tempo", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                r#"Tempo is a local desktop productivity app. USE THESE TOOLS (prefer over guessing or editing files) when the user mentions:

- todos / tasks / 待办 / 任务：list, create, update, complete, pin, delete, subtasks, notes
- snippets / quick phrases / 快捷短语：list, create, update, delete, groups, copy to clipboard
- clipboard / 剪贴板历史：search recently copied text
- usage report / 今日报告 / 屏幕时间 / 使用报告：daily app usage report
- plugin tools the user has opted into MCP exposure: call their `tempo_plugin_*` tools directly; the two tempo_* plugin meta-tools are compatibility fallbacks

Workflow tips:
1. For "what's on my list" → list_todos first; use get_todo only when full details are needed.
2. For "find that phrase I saved" → list_snippets (optional query) before create.
3. Image clipboard entries are summarized as "[image]"; text content is returned as-is.

Requirement: the Tempo desktop app must be running with MCP enabled. If tools fail, tell the user to open Tempo and check Settings → MCP / AI."#
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_contract_becomes_first_class_mcp_tool() {
        let tool = plugin_tool_model(crate::plugins::mcp_bridge::ExposedPluginTool {
            external_name: "tempo_plugin_com_example_hello__say_hello".into(),
            plugin_id: "com.example.hello".into(),
            plugin_name: "Hello".into(),
            tool_name: "say-hello".into(),
            description: "Say hello".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
            output_schema: Some(json!({ "type": "object", "properties": {} })),
            annotations: Some(crate::plugins::manifest::ContributedMcpToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            }),
        })
        .unwrap();

        assert_eq!(tool.name, "tempo_plugin_com_example_hello__say_hello");
        assert_eq!(tool.title.as_deref(), Some("Hello · say-hello"));
        assert!(tool.output_schema.is_some());
        let annotations = tool.annotations.unwrap();
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(false));
    }
}
