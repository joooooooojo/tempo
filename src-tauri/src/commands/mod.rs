pub mod clipboard;
pub mod markdown;
pub mod snippets;
pub mod pomodoro_cmds;
pub mod reports;
pub mod settings;
pub mod todos;
mod tracker;
pub mod window;

use serde::{Deserialize, Serialize};

pub use markdown::markdown_image_protocol_response;
pub use settings::do_reset_today;
pub use todos::check_pending_recurrences;
pub use tracker::start_tracker;
pub use window::{hide_to_tray, quit_app, show_window};

pub const MARKDOWN_IMAGE_PROTOCOL: &str = "tempo-image";

pub(crate) const MAX_TODO_IMAGES: usize = 4;
pub(crate) const MAX_TODO_NOTE_IMAGES: usize = 4;
pub(crate) const MAX_TODO_IMAGE_BYTES: usize = 5 * 1024 * 1024;
pub(crate) const MAX_TODO_NOTE_CHARS: usize = 1_000;

#[derive(Debug, Clone, Deserialize)]
pub struct TodoImageInput {
    pub(crate) data_url: String,
    pub(crate) mime_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct TodoBackupFile {
    format: String,
    exported_at: String,
    todos: Vec<crate::db::TodoItem>,
}
