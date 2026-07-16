use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

pub fn hide_to_tray(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        crate::macos_dock::hide_presence(app);
    }

    #[cfg(not(target_os = "macos"))]
    if let Some(window) = app.get_webview_window("main") {
        crate::logging::debug_if_err(window.hide(), "hide main window to tray");
    }
}

#[tauri::command]
pub fn hide_to_tray_command(app: AppHandle) {
    hide_to_tray(&app);
}

#[tauri::command]
pub fn show_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        #[cfg(target_os = "macos")]
        {
            crate::macos_dock::show_presence(&app)?;
        }

        window.show().map_err(|e| e.to_string())?;
        window.unminimize().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }

    Ok(())
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
