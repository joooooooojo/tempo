//! Tempo plugin host: package install, trust, on-demand Node runtime, supervisor, Host Bridge,
//! and the `tempo-plugin://` UI resource protocol.

pub mod bridge;
pub mod hooks;
pub mod host;
pub mod ids;
pub mod loader;
pub mod manifest;
pub mod mcp_bridge;
pub mod package;
pub mod paths;
pub mod runtime;
pub mod storage;
pub mod supervisor;
pub mod trust;
pub mod ui;
