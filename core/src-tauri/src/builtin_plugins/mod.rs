pub mod clipboard;
pub mod file_search;
pub mod hosts;
pub mod mcp_exposure;
pub mod plugin_dev;
pub mod port_manager;
pub mod reports;
pub mod settings;
pub mod snippets;
pub mod todo;
pub mod translate;

pub use mcp_exposure::{
    builtin_data_dir, builtin_mcp_tools_for_app, ensure_builtin_mcp_tables, is_builtin_mcp_exposed,
    is_static_mcp_tool_allowed, list_builtin_mcp_tool_infos, set_builtin_mcp_exposed,
    set_builtin_mcp_tool_enabled, BuiltinMcpToolInfo,
};
