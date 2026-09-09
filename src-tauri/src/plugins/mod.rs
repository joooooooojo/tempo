//! Tempo plugin host: package install, trust, on-demand Deno runtime, supervisor, Host Bridge,
//! and the `tempo-plugin://` UI resource protocol.

pub mod bridge;
pub mod files;
pub mod host;
pub mod host_events;
pub mod ids;
pub mod loader;
pub mod manifest;
pub mod mcp_bridge;
pub mod package;
pub mod paths;
pub mod permissions;
pub mod runtime;
pub mod repository;
pub mod repository_template;
pub mod settings;
pub mod storage;
pub mod supervisor;
pub mod trust;
pub mod ui;
pub mod window_icons;
pub mod windows;
