pub mod launcher;
pub mod markdown;
pub mod plugins;
pub(crate) mod tracker;
pub mod url_browsers;
pub mod window;

use serde::{Deserialize, Serialize};

pub use markdown::markdown_image_protocol_response;
pub use tracker::start_tracker;
pub use window::quit_app;

// Re-export builtin plugin commands through historical paths during migration consumers.
pub use crate::builtin_plugins::settings::do_reset_today;
pub use crate::builtin_plugins::todo::check_pending_recurrences;

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
    pub(crate) format: String,
    pub(crate) exported_at: String,
    pub(crate) todos: Vec<crate::db::TodoItem>,
}
