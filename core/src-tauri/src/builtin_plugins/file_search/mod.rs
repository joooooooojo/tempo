pub mod commands;
mod preview_protocol;
mod tools;

#[cfg(target_os = "windows")]
pub(crate) mod backend_windows;

#[cfg(target_os = "macos")]
mod backend_macos;

pub use commands::*;
pub use preview_protocol::{
    file_preview_protocol_response, preview_url_for_path, FILE_PREVIEW_PROTOCOL,
};
