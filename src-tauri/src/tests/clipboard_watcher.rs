use crate::clipboard_db::ClipboardEntry;
use crate::clipboard_watcher::{clipboard_entry_content_summary, resolve_clipboard_source};
use crate::platform::ForegroundApp;

fn app(name: &str, process_name: &str) -> ForegroundApp {
    ForegroundApp {
        name: name.to_string(),
        process_name: process_name.to_string(),
    }
}

#[test]
fn resolve_clipboard_source_uses_current_non_tempo_app() {
    let (name, process) = resolve_clipboard_source(
        Some(app("Code", "Code.exe")),
        Some("Fallback"),
        Some("Fallback.exe"),
    );

    assert_eq!(name.as_deref(), Some("Code"));
    assert_eq!(process.as_deref(), Some("Code.exe"));
}

#[test]
fn resolve_clipboard_source_falls_back_for_tempo_and_system_sources() {
    let (name, process) = resolve_clipboard_source(
        Some(app("Tempo", "tempo.exe")),
        Some("Browser"),
        Some("browser.exe"),
    );

    assert_eq!(name.as_deref(), Some("Browser"));
    assert_eq!(process.as_deref(), Some("browser.exe"));

    let (name, process) = resolve_clipboard_source(
        Some(app("SnippingTool", "SnippingTool.exe")),
        Some("Editor"),
        Some("editor.exe"),
    );

    assert_eq!(name.as_deref(), Some("Editor"));
    assert_eq!(process.as_deref(), Some("editor.exe"));
}

#[test]
fn clipboard_entry_summary_does_not_include_text_content() {
    let entry = ClipboardEntry {
        id: 1,
        content: "secret clipboard text".to_string(),
        kind: "text".to_string(),
        source_app: None,
        source_process: None,
        source_icon_data_url: None,
        image_width: None,
        image_height: None,
        pinned: false,
        created_at: "2026-01-01T00:00:00Z".to_string(),
    };

    let summary = clipboard_entry_content_summary(&entry);

    assert!(summary.contains("text_chars="));
    assert!(!summary.contains("secret"));
    assert!(!summary.contains("clipboard text"));
}
