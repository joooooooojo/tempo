use tauri::window::Color;
#[cfg(not(target_os = "macos"))]
use tauri::{Monitor, PhysicalPosition, PhysicalSize};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

const EYE_CARE_PRIMARY_LABEL: &str = "eye-care-reminder";

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

    // Pre-create eye-care overlays during startup so the first reminder does not
    // build a WebView inside the invoke handler (Windows WebView2 can deadlock IPC).
    #[cfg(target_os = "macos")]
    {
        if app.get_webview_window(EYE_CARE_PRIMARY_LABEL).is_none() {
            let window = build_eye_care_overlay_window(app, EYE_CARE_PRIMARY_LABEL)?;
            let _ = window.hide();
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let monitors = ordered_monitors(app).unwrap_or_default();
        let count = monitors.len().max(1);
        for index in 0..count {
            let label = eye_care_label(index);
            let window = if let Some(window) = app.get_webview_window(&label) {
                window
            } else {
                build_eye_care_overlay_window(app, &label)?
            };

            if let Some(monitor) = monitors.get(index) {
                place_eye_care_window_on_monitor(&window, monitor);
            }
            let _ = window.hide();
        }
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
    #[cfg(target_os = "macos")]
    {
        // macOS: simple fullscreen on the primary display only.
        // Multi-monitor window overlays look poor and are intentionally skipped.
        let window = get_or_create_eye_care_window(app, EYE_CARE_PRIMARY_LABEL)?;
        let _ = window.set_simple_fullscreen(true);
        polish_macos_eye_care_overlay(&window);
        present_eye_care_window(&window)?;
        window.set_focus()?;
        let _ = app.emit("eye-care:reveal", ());
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let monitors = ordered_monitors(app)?;
        if monitors.is_empty() {
            return Err(tauri::Error::WindowNotFound);
        }

        for (index, monitor) in monitors.iter().enumerate() {
            let label = eye_care_label(index);
            let window = get_or_create_eye_care_window(app, &label)?;
            // Place before and after show: first show on a secondary DPI display
            // can ignore the initial move until the HWND is fully realized.
            place_eye_care_window_on_monitor(&window, monitor);
            present_eye_care_window(&window)?;
            place_eye_care_window_on_monitor(&window, monitor);
        }

        // Drop overlays left over from a previous session with more displays.
        close_extra_eye_care_windows(app, monitors.len());

        if let Some(primary) = app.get_webview_window(EYE_CARE_PRIMARY_LABEL) {
            let _ = primary.set_focus();
        }

        let _ = app.emit("eye-care:reveal", ());
        Ok(())
    }
}

#[tauri::command]
pub fn hide_eye_care_overlay(app: AppHandle) -> Result<(), String> {
    let windows = app.webview_windows();
    for (label, window) in windows {
        if !is_eye_care_label(&label) {
            continue;
        }

        #[cfg(target_os = "macos")]
        let _ = window.set_simple_fullscreen(false);

        if label == EYE_CARE_PRIMARY_LABEL {
            window.hide().map_err(|error| error.to_string())?;
        } else {
            // Keep secondary overlays around so the next reminder does not recreate
            // WebViews inside the invoke handler (Windows WebView2 can deadlock IPC).
            let _ = window.hide();
        }
    }

    Ok(())
}

fn eye_care_label(index: usize) -> String {
    if index == 0 {
        EYE_CARE_PRIMARY_LABEL.to_string()
    } else {
        format!("{EYE_CARE_PRIMARY_LABEL}-{index}")
    }
}

fn is_eye_care_label(label: &str) -> bool {
    label == EYE_CARE_PRIMARY_LABEL || label.starts_with(&format!("{EYE_CARE_PRIMARY_LABEL}-"))
}

fn get_or_create_eye_care_window(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(label) {
        Ok(window)
    } else {
        build_eye_care_overlay_window(app, label)
    }
}

fn present_eye_care_window(window: &WebviewWindow) -> tauri::Result<()> {
    let _ = window.set_always_on_top(true);
    let _ = window.set_shadow(false);
    let _ = window.set_background_color(Some(Color(239, 251, 244, 255)));
    window.show()?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn ordered_monitors(app: &AppHandle) -> tauri::Result<Vec<Monitor>> {
    let mut monitors = app.available_monitors()?;
    if let Some(primary) = app.primary_monitor()? {
        if let Some(index) = monitors.iter().position(|monitor| same_monitor(monitor, &primary)) {
            let primary_monitor = monitors.remove(index);
            monitors.insert(0, primary_monitor);
        } else {
            monitors.insert(0, primary);
        }
    }
    Ok(monitors)
}

#[cfg(not(target_os = "macos"))]
fn same_monitor(left: &Monitor, right: &Monitor) -> bool {
    left.position() == right.position() && left.size() == right.size()
}

#[cfg(not(target_os = "macos"))]
fn place_eye_care_window_on_monitor(window: &WebviewWindow, monitor: &Monitor) {
    let position = monitor.position();
    let size = monitor.size();
    // Use physical pixels so mixed-DPI secondary monitors land correctly.
    let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
    let _ = window.set_size(PhysicalSize::new(size.width, size.height));
}

#[cfg(not(target_os = "macos"))]
fn close_extra_eye_care_windows(app: &AppHandle, active_count: usize) {
    for (label, window) in app.webview_windows() {
        if !is_eye_care_label(&label) || label == EYE_CARE_PRIMARY_LABEL {
            continue;
        }

        let Some(suffix) = label.strip_prefix(&format!("{EYE_CARE_PRIMARY_LABEL}-")) else {
            continue;
        };
        let Ok(index) = suffix.parse::<usize>() else {
            let _ = window.close();
            continue;
        };

        if index >= active_count {
            let _ = window.close();
        }
    }
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

fn build_eye_care_overlay_window(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        label,
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
