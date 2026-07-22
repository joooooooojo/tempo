use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::db::AppState;

fn json_result<T: serde::Serialize>(value: T) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(&value).map_err(|e| {
        McpError::internal_error(format!("serialize tool result: {e}"), None)
    })?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn tool_err(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message.into())])
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
    #[schemars(description = "Optional language hint for highlighting (e.g. markdown, typescript)")]
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
pub struct StartPomodoroArgs {
    #[serde(default)]
    #[schemars(description = "Optional todo id to bind this focus session to")]
    pub todo_id: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DailyReportArgs {
    #[serde(default)]
    #[schemars(description = "Optional date as YYYY-MM-DD; defaults to today")]
    pub date: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CallPluginToolArgs {
    #[schemars(description = "Plugin package id, e.g. com.example.hello (see tempo_list_exposed_plugin_tools)")]
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
        match crate::commands::todos::get_todos(self.app.clone(), state) {
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
        match crate::commands::todos::get_todo(state, args.id) {
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
        match crate::commands::todos::add_todo(
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
        match crate::commands::todos::update_todo_details(
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
        match crate::commands::todos::set_todo_completed(
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
        match crate::commands::todos::set_todo_pinned(
            self.app.clone(),
            state,
            args.id,
            args.pinned,
        ) {
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
        match crate::commands::todos::delete_todo(self.app.clone(), state, args.id) {
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
        match crate::commands::todos::add_todo_subtask(
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
        match crate::commands::todos::add_todo_note(
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
            crate::commands::snippets::get_snippets(state, args.query, args.group_id, args.sort);
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
        json_result(crate::commands::snippets::get_snippet_groups(state))
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
        match crate::commands::snippets::create_snippet(
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
        match crate::commands::snippets::update_snippet_command(
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
        match crate::commands::snippets::delete_snippet_command(self.app.clone(), state, args.id) {
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
        match crate::commands::snippets::create_snippet_group(
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
        match crate::commands::snippets::copy_snippet_to_clipboard(
            self.app.clone(),
            state,
            args.id,
        ) {
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
        let page = crate::commands::clipboard::get_clipboard_history(
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
        description = "Get current Tempo pomodoro/番茄钟 timer state. Use when asking if a focus session is running or remaining time"
    )]
    fn get_pomodoro_state(&self) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        json_result(crate::commands::pomodoro_cmds::get_pomodoro_state(state))
    }

    #[tool(
        description = "Start the Tempo pomodoro/番茄钟 timer. Optional todo_id to bind focus. Use when starting focus/专注/开始番茄"
    )]
    fn start_pomodoro(
        &self,
        Parameters(args): Parameters<StartPomodoroArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        match crate::commands::pomodoro_cmds::start_pomodoro(
            self.app.clone(),
            state,
            args.todo_id,
        ) {
            Ok(snapshot) => json_result(snapshot),
            Err(e) => Ok(tool_err(e)),
        }
    }

    #[tool(
        description = "Pause the Tempo pomodoro/番茄钟 timer. Use when pausing focus/暂停番茄"
    )]
    fn pause_pomodoro(&self) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        json_result(crate::commands::pomodoro_cmds::pause_pomodoro(
            self.app.clone(),
            state,
        ))
    }

    #[tool(
        description = "Stop the Tempo pomodoro/番茄钟 timer. Use when ending focus/停止番茄"
    )]
    fn stop_pomodoro(&self) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        json_result(crate::commands::pomodoro_cmds::stop_pomodoro(
            self.app.clone(),
            state,
        ))
    }

    #[tool(
        description = "Skip the current Tempo pomodoro phase (focus or break). Use when skipping/跳过当前阶段"
    )]
    fn skip_pomodoro_phase(&self) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        json_result(crate::commands::pomodoro_cmds::skip_pomodoro_phase(
            self.app.clone(),
            state,
        ))
    }

    #[tool(
        description = "Get Tempo daily screen-time report/今日报告/屏幕时间. Optional date YYYY-MM-DD (defaults today). Use when asking how much time was spent on apps"
    )]
    fn get_daily_report(
        &self,
        Parameters(args): Parameters<DailyReportArgs>,
    ) -> Result<CallToolResult, McpError> {
        let state = match app_state(&self.app) {
            Ok(s) => s,
            Err(e) => return Ok(e),
        };
        let report = crate::commands::reports::get_daily_report(state, args.date);
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
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("tempo", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                r#"Tempo is a local desktop productivity app. USE THESE TOOLS (prefer over guessing or editing files) when the user mentions:

- todos / tasks / 待办 / 任务：list, create, update, complete, pin, delete, subtasks, notes
- snippets / quick phrases / 快捷短语：list, create, update, delete, groups, copy to clipboard
- clipboard / 剪贴板历史：search recently copied text
- pomodoro / 番茄钟 / 专注：get state, start, pause, stop, skip phase
- screen time / 今日报告 / 屏幕时间 / 使用报告：daily app usage report
- plugin tools the user has opted into MCP exposure：tempo_list_exposed_plugin_tools then tempo_call_plugin_tool

Workflow tips:
1. For "what's on my list" → list_todos first; use get_todo only when full details are needed.
2. For focus tied to a task → list_todos or get_todo, then start_pomodoro with todo_id.
3. For "find that phrase I saved" → list_snippets (optional query) before create.
4. Image clipboard entries are summarized as "[image]"; text content is returned as-is.

Requirement: the Tempo desktop app must be running with MCP enabled. If tools fail, tell the user to open Tempo and check Settings → MCP / AI."#
                    .to_string(),
            )
    }
}
