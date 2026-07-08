use tauri::window::Color;
#[cfg(not(target_os = "macos"))]
use tauri::LogicalPosition;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const QUICK_TODO_PANEL_WIDTH: f64 = 420.0;
pub const QUICK_TODO_PANEL_HEIGHT: f64 = 80.0;
pub const QUICK_TODO_SHADOW_PADDING: f64 = 32.0;

pub fn quick_todo_window_size() -> (f64, f64) {
    (
        QUICK_TODO_PANEL_WIDTH + QUICK_TODO_SHADOW_PADDING * 2.0,
        QUICK_TODO_PANEL_HEIGHT + QUICK_TODO_SHADOW_PADDING * 2.0,
    )
}

pub fn precache_auxiliary_windows(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("quick-todo").is_none() {
        let (width, height) = quick_todo_window_size();
        let window = build_quick_todo_window(app, width, height)?;
        polish_quick_todo_window(&window);
        let _ = window.hide();
    }

    Ok(())
}

pub fn show_quick_todo(app: &AppHandle) -> tauri::Result<()> {
    let (width, height) = quick_todo_window_size();
    let window = if let Some(window) = app.get_webview_window("quick-todo") {
        window
    } else {
        let window = build_quick_todo_window(app, width, height)?;
        polish_quick_todo_window(&window);
        window
    };

    let _ = window.set_size(LogicalSize::new(width, height));
    let _ = window.center();
    let _ = window.set_always_on_top(true);
    polish_quick_todo_window(&window);
    window.show()?;
    window.set_focus()?;
    let _ = app.emit_to("quick-todo", "quick-todo:focus-title", ());
    Ok(())
}

#[tauri::command]
pub fn show_eye_care_overlay(app: AppHandle) -> Result<(), String> {
    show_eye_care_overlay_window(&app).map_err(|error| error.to_string())
}

pub fn show_eye_care_overlay_window(app: &AppHandle) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window("eye-care-reminder") {
        window
    } else {
        build_eye_care_overlay_window(app)?
    };

    #[cfg(target_os = "macos")]
    {
        let _ = window.set_simple_fullscreen(true);
        polish_macos_eye_care_overlay(&window);
    }

    #[cfg(target_os = "windows")]
    {
        let (x, y, width, height) = primary_monitor_bounds(app)?;
        let _ = window.set_position(LogicalPosition::new(x, y));
        let _ = window.set_size(LogicalSize::new(width, height));
        let _ = window.set_fullscreen(true);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let (x, y, width, height) = primary_monitor_bounds(app)?;
        let _ = window.set_position(LogicalPosition::new(x, y));
        let _ = window.set_size(LogicalSize::new(width, height));
    }

    let _ = window.set_always_on_top(true);
    let _ = window.set_shadow(false);
    let _ = window.set_background_color(Some(Color(239, 251, 244, 255)));
    window.show()?;
    window.set_focus()?;
    let _ = app.emit_to("eye-care-reminder", "eye-care:reveal", ());
    Ok(())
}

#[tauri::command]
pub fn hide_eye_care_overlay(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("eye-care-reminder") else {
        return Ok(());
    };

    #[cfg(target_os = "macos")]
    let _ = window.set_simple_fullscreen(false);

    #[cfg(target_os = "windows")]
    let _ = window.set_fullscreen(false);

    window.hide().map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
fn primary_monitor_bounds(app: &AppHandle) -> tauri::Result<(f64, f64, f64, f64)> {
    let monitor = app
        .primary_monitor()?
        .ok_or_else(|| tauri::Error::WindowNotFound)?;

    let size = monitor.size();
    let position = monitor.position();
    let scale = monitor.scale_factor();

    Ok((
        position.x as f64 / scale,
        position.y as f64 / scale,
        size.width as f64 / scale,
        size.height as f64 / scale,
    ))
}

pub fn polish_quick_todo_window(window: &WebviewWindow) {
    let _ = window.set_shadow(false);
    let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));

    #[cfg(target_os = "macos")]
    polish_macos_quick_todo_window(window);
}

pub fn build_quick_todo_window(
    app: &AppHandle,
    width: f64,
    height: f64,
) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        "quick-todo",
        WebviewUrl::App("/?view=quick-todo".into()),
    )
    .title("快速待办")
    .inner_size(width, height)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .background_color(Color(0, 0, 0, 0))
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(false)
    .center()
    .build()
}

fn build_eye_care_overlay_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        "eye-care-reminder",
        WebviewUrl::App("/?view=eye-care".into()),
    )
    .title("")
    .inner_size(1280.0, 800.0)
    .decorations(false)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible_on_all_workspaces(true)
    .visible(false)
    .focused(false)
    .shadow(false)
    .background_color(Color(239, 251, 244, 255))
    .build()
}

#[cfg(target_os = "macos")]
fn polish_macos_eye_care_overlay(window: &WebviewWindow) {
    let _ = window.with_webview(|webview| unsafe {
        apply_macos_overlay_level(webview.ns_window());
    });
}

#[cfg(target_os = "macos")]
unsafe fn apply_macos_overlay_level(ns_window: *mut std::ffi::c_void) {
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};

    let ns_window = ns_window.cast::<Object>();
    if ns_window.is_null() {
        return;
    }

    // NSScreenSaverWindowLevel: cover menu bar and dock.
    let _: () = msg_send![ns_window, setLevel: 1000_i64];
    let _: () = msg_send![ns_window, setHasShadow: false];
}

#[cfg(target_os = "macos")]
fn polish_macos_quick_todo_window(window: &WebviewWindow) {
    let _ = window.with_webview(|webview| unsafe {
        apply_macos_transparent_window(webview.ns_window());
    });
}

#[cfg(target_os = "macos")]
unsafe fn apply_macos_transparent_window(ns_window: *mut std::ffi::c_void) {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    let ns_window = ns_window.cast::<Object>();
    if ns_window.is_null() {
        return;
    }

    let clear_color: *mut Object = msg_send![Class::get("NSColor").unwrap(), clearColor];
    let _: () = msg_send![ns_window, setBackgroundColor: clear_color];
    let _: () = msg_send![ns_window, setOpaque: false];
    let _: () = msg_send![ns_window, setHasShadow: false];
}
