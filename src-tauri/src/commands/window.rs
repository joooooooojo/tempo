use tauri::AppHandle;

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn debug_log(scope: String, message: String) {
    tracing::debug!(
        target: "tempo::frontend",
        scope = %crate::logging::sanitize_log_value(&scope),
        message_chars = message.chars().count(),
        "frontend debug log"
    );
}

/// Whether the OS appearance preference is dark (for theme = "system").
#[tauri::command]
pub fn system_prefers_dark() -> bool {
    crate::platform::system_prefers_dark()
}
