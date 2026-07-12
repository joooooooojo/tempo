use tauri::window::Color;
#[cfg(not(target_os = "macos"))]
use tauri::{Monitor, PhysicalPosition, PhysicalSize};
#[cfg(target_os = "macos")]
use tauri::PhysicalPosition;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri::menu::{Menu, MenuItem};

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const EYE_CARE_PRIMARY_LABEL: &str = "eye-care-reminder";
pub const POMODORO_FLOAT_LABEL: &str = "pomodoro-float";

pub const QUICK_TODO_PANEL_WIDTH: f64 = 380.0;
pub const QUICK_TODO_PANEL_HEIGHT: f64 = 44.0;

pub const POMODORO_FLOAT_PANEL_WIDTH: f64 = 300.0;
pub const POMODORO_FLOAT_PANEL_HEIGHT: f64 = 56.0;

pub const CLIPBOARD_PICKER_LABEL: &str = "clipboard-picker";
pub const SNIPPET_PICKER_LABEL: &str = "snippet-picker";
pub const SHELF_HEIGHT: f64 = 228.0;
pub const SHELF_WIDTH_RATIO: f64 = 0.88;
pub const CLIPBOARD_SHELF_WIDTH_RATIO: f64 = 1.0;

const SHELF_SHORTCUT_DEBOUNCE_MS: u64 = 280;

static LAST_SHELF_SHORTCUT_MS: AtomicU64 = AtomicU64::new(0);

pub fn quick_todo_window_size() -> (f64, f64) {
    (QUICK_TODO_PANEL_WIDTH, QUICK_TODO_PANEL_HEIGHT)
}

pub fn pomodoro_float_window_size() -> (f64, f64) {
    (POMODORO_FLOAT_PANEL_WIDTH, POMODORO_FLOAT_PANEL_HEIGHT)
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

    if app.get_webview_window(POMODORO_FLOAT_LABEL).is_none() {
        let (width, height) = pomodoro_float_window_size();
        let window = build_pomodoro_float_window(app, width, height)?;
        attach_pomodoro_float_menu_handler(app, &window);
        polish_pomodoro_float_window(&window);
        let _ = window.hide();
    }

    for (label, view) in [
        (CLIPBOARD_PICKER_LABEL, "clipboard-picker"),
        (SNIPPET_PICKER_LABEL, "snippet-picker"),
    ] {
        if app.get_webview_window(label).is_none() {
            let window = build_shelf_picker_window(app, label, view)?;
            // macOS: defer native NSWindow tweaks to F4/F5 show time. Calling with_webview
            // or setFrame during did_finish_launching aborts the process.
            #[cfg(not(target_os = "macos"))]
            polish_shelf_picker_window(&window, label == CLIPBOARD_PICKER_LABEL);
            #[cfg(target_os = "macos")]
            {
                let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));
                let _ = window.set_shadow(true);
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
pub fn show_clipboard_picker(app: AppHandle) -> Result<(), String> {
    show_clipboard_picker_window(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn show_snippet_picker(app: AppHandle) -> Result<(), String> {
    show_snippet_picker_window(&app).map_err(|error| error.to_string())
}

pub fn show_clipboard_picker_window(app: &AppHandle) -> tauri::Result<()> {
    show_shelf_picker(
        app,
        CLIPBOARD_PICKER_LABEL,
        "clipboard-picker",
        "clipboard-picker:open",
        CLIPBOARD_SHELF_WIDTH_RATIO,
        true,
    )
}

pub fn show_snippet_picker_window(app: &AppHandle) -> tauri::Result<()> {
    show_shelf_picker(
        app,
        SNIPPET_PICKER_LABEL,
        "snippet-picker",
        "snippet-picker:open",
        SHELF_WIDTH_RATIO,
        false,
    )
}

fn show_shelf_picker(
    app: &AppHandle,
    label: &str,
    view: &str,
    open_event: &str,
    width_ratio: f64,
    topmost: bool,
) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window(label) {
        window
    } else {
        let window = build_shelf_picker_window(app, label, view)?;
        polish_shelf_picker_window(&window, topmost);
        window
    };

    if window.is_visible().unwrap_or(false) {
        if !consume_shelf_shortcut_action() {
            return Ok(());
        }
        hide_shelf_picker_window(&window)?;
        return Ok(());
    }

    if !consume_shelf_shortcut_action() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        // Never call Tauri set_always_on_top/show on macOS: tao resets level to
        // NSFloatingWindowLevel (3) asynchronously, which sits below the Dock (~20)
        // and causes Dock flicker when the level changes.
        place_bottom_shelf_window(app, &window, width_ratio)?;
        show_macos_shelf_picker(&window, topmost)?;
        crate::macos_shelf_dismiss::note_shelf_shown();
    }
    #[cfg(not(target_os = "macos"))]
    {
        place_bottom_shelf_window(app, &window, width_ratio)?;
        let _ = window.set_always_on_top(true);
        window.show()?;
        window.set_focus()?;
    }
    let _ = app.emit_to(label, open_event, ());
    Ok(())
}

pub(crate) fn hide_shelf_picker_window(window: &WebviewWindow) -> tauri::Result<()> {
    window.hide()
}

fn consume_shelf_shortcut_action() -> bool {
    let now = shelf_shortcut_now_ms();
    let last = LAST_SHELF_SHORTCUT_MS.load(Ordering::Relaxed);
    if last != 0 && now.saturating_sub(last) < SHELF_SHORTCUT_DEBOUNCE_MS {
        return false;
    }
    LAST_SHELF_SHORTCUT_MS.store(now, Ordering::Relaxed);
    true
}

fn shelf_shortcut_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn shelf_monitor(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<tauri::Monitor> {
    if let Some(monitor) = window.current_monitor()? {
        return Ok(monitor);
    }

    app.primary_monitor()?
        .ok_or(tauri::Error::WindowNotFound)
}

fn place_bottom_shelf_window(
    app: &AppHandle,
    window: &WebviewWindow,
    width_ratio: f64,
) -> tauri::Result<()> {
    let monitor = shelf_monitor(app, window)?;
    let scale = monitor.scale_factor();

    #[cfg(target_os = "macos")]
    let (area_pos, area_w, area_h) = {
        let position = monitor.position();
        let size = monitor.size();
        (
            position,
            size.width as f64,
            size.height as f64,
        )
    };

    #[cfg(not(target_os = "macos"))]
    let (area_pos, area_w, area_h) = {
        let work = monitor.work_area();
        (
            work.position,
            work.size.width as f64,
            work.size.height as f64,
        )
    };

    let width = (area_w / scale) * width_ratio;
    let height = SHELF_HEIGHT;
    let _ = window.set_size(LogicalSize::new(width, height));
    let x = if (width_ratio - 1.0).abs() < f64::EPSILON {
        area_pos.x
    } else {
        area_pos.x + ((area_w - width * scale) / 2.0).round() as i32
    };
    let y = area_pos.y + (area_h - height * scale).round() as i32;
    let _ = window.set_position(PhysicalPosition::new(x, y));
    Ok(())
}

fn build_shelf_picker_window(
    app: &AppHandle,
    label: &str,
    view: &str,
) -> tauri::Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App(format!("/?view={view}").into()),
    )
    .title("")
    .inner_size(960.0, SHELF_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .background_color(Color(0, 0, 0, 0))
    .shadow(cfg!(target_os = "macos"))
    .skip_taskbar(true)
    .visible_on_all_workspaces(true)
    .visible(false)
    .focused(false);

    #[cfg(not(target_os = "macos"))]
    let builder = builder.always_on_top(true);

    let window = builder.build()?;

    attach_shelf_dismiss_on_blur(&window);
    Ok(window)
}

fn attach_shelf_dismiss_on_blur(_window: &WebviewWindow) {
    #[cfg(not(target_os = "macos"))]
    {
        let window = window.clone();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::Focused(false)) {
                let _ = window.hide();
            }
        });
    }
}

pub fn polish_shelf_picker_window(window: &WebviewWindow, topmost: bool) {
    let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));

    #[cfg(target_os = "macos")]
    {
        let _ = window.set_shadow(true);
        polish_macos_shelf_picker_window(window, topmost);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.set_shadow(false);
        let _ = topmost;
    }
}

#[cfg(target_os = "macos")]
fn show_macos_shelf_picker(window: &WebviewWindow, topmost: bool) -> tauri::Result<()> {
    window
        .with_webview(move |webview| unsafe {
            apply_macos_shelf_appearance(webview.ns_window(), topmost);
            present_macos_shelf_without_activation(webview.ns_window());
        })
        .map_err(Into::into)
}

#[cfg(target_os = "macos")]
unsafe fn present_macos_shelf_without_activation(ns_window: *mut std::ffi::c_void) {
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};

    let ns_window = ns_window.cast::<Object>();
    if ns_window.is_null() {
        return;
    }

    // orderFrontRegardless shows the window without activating the app.
    // Avoid makeKeyAndOrderFront / set_focus, which steal focus and make the Dock flicker.
    let _: () = msg_send![ns_window, orderFrontRegardless];
    let is_key: bool = msg_send![ns_window, isKeyWindow];
    if is_key {
        let _: () = msg_send![ns_window, resignKey];
    }
}

#[cfg(target_os = "macos")]
const MACOS_SHELF_WINDOW_LEVEL: i64 = 25;

#[cfg(target_os = "macos")]
fn polish_macos_shelf_picker_window(window: &WebviewWindow, topmost: bool) {
    let _ = window.with_webview(move |webview| unsafe {
        apply_macos_shelf_appearance(webview.ns_window(), topmost);
    });
}

#[cfg(target_os = "macos")]
unsafe fn apply_macos_shelf_appearance(ns_window: *mut std::ffi::c_void, topmost: bool) {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    let ns_window = ns_window.cast::<Object>();
    if ns_window.is_null() {
        return;
    }

    if topmost {
        let _: () = msg_send![ns_window, setLevel: MACOS_SHELF_WINDOW_LEVEL];
        const NS_WINDOW_COLLECTION_CAN_JOIN_ALL_SPACES: usize = 1 << 0;
        const NS_WINDOW_COLLECTION_STATIONARY: usize = 1 << 4;
        const NS_WINDOW_COLLECTION_FULL_SCREEN_AUXILIARY: usize = 1 << 8;
        const NS_WINDOW_COLLECTION_FULL_SCREEN_NONE: usize = 1 << 9;
        let behavior = NS_WINDOW_COLLECTION_CAN_JOIN_ALL_SPACES
            | NS_WINDOW_COLLECTION_STATIONARY
            | NS_WINDOW_COLLECTION_FULL_SCREEN_AUXILIARY
            | NS_WINDOW_COLLECTION_FULL_SCREEN_NONE;
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
    }

    let _: () = msg_send![ns_window, setHidesOnDeactivate: false];

    let Some(color_class) = Class::get("NSColor") else {
        return;
    };
    let clear_color: *mut Object = msg_send![color_class, clearColor];
    let _: () = msg_send![ns_window, setBackgroundColor: clear_color];
    let _: () = msg_send![ns_window, setOpaque: false];
    let _: () = msg_send![ns_window, setHasShadow: true];

    let content_view: *mut Object = msg_send![ns_window, contentView];
    if content_view.is_null() {
        return;
    }

    let _: () = msg_send![content_view, setWantsLayer: true];
    let layer: *mut Object = msg_send![content_view, layer];
    if layer.is_null() {
        return;
    }

    let _: () = msg_send![layer, setCornerRadius: 16.0_f64];
    let _: () = msg_send![layer, setMasksToBounds: true];
}

pub fn is_pomodoro_float_visible(app: &AppHandle) -> bool {
    app.get_webview_window(POMODORO_FLOAT_LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

#[tauri::command]
pub fn show_pomodoro_float(app: AppHandle) -> Result<(), String> {
    show_pomodoro_float_window(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn hide_pomodoro_float(app: AppHandle) -> Result<(), String> {
    hide_pomodoro_float_window(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn toggle_pomodoro_float(app: AppHandle) -> Result<bool, String> {
    toggle_pomodoro_float_window(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn is_pomodoro_float_visible_command(app: AppHandle) -> bool {
    is_pomodoro_float_visible(&app)
}

#[tauri::command]
pub fn set_pomodoro_float_expanded(_app: AppHandle, _expanded: bool) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn save_pomodoro_float_position(
    state: tauri::State<crate::db::AppState>,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let conn = state.db.lock();
    crate::db::set_setting(&conn, "pomodoro_float_x", &x.to_string());
    crate::db::set_setting(&conn, "pomodoro_float_y", &y.to_string());
    Ok(())
}

pub fn show_pomodoro_float_window(app: &AppHandle) -> tauri::Result<()> {
    let (width, height) = pomodoro_float_window_size();
    let window = get_or_create_pomodoro_float_window(app, width, height)?;

    let _ = window.set_size(LogicalSize::new(width, height));
    place_pomodoro_float_window(app, &window, width, height)?;
    let _ = window.set_always_on_top(true);
    polish_pomodoro_float_window(&window);
    window.show()?;
    emit_pomodoro_float_visible(app, true);
    Ok(())
}

pub fn hide_pomodoro_float_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(POMODORO_FLOAT_LABEL) {
        window.hide()?;
    }
    emit_pomodoro_float_visible(app, false);
    Ok(())
}

fn emit_pomodoro_float_visible(app: &AppHandle, visible: bool) {
    let _ = app.emit("pomodoro-float-visible", visible);
    crate::tray_menu::sync_pomodoro_float_checked(app, visible);
}

pub fn toggle_pomodoro_float_window(app: &AppHandle) -> tauri::Result<bool> {
    if is_pomodoro_float_visible(app) {
        hide_pomodoro_float_window(app)?;
        Ok(false)
    } else {
        show_pomodoro_float_window(app)?;
        Ok(true)
    }
}

fn get_or_create_pomodoro_float_window(
    app: &AppHandle,
    width: f64,
    height: f64,
) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(POMODORO_FLOAT_LABEL) {
        Ok(window)
    } else {
        let window = build_pomodoro_float_window(app, width, height)?;
        attach_pomodoro_float_menu_handler(app, &window);
        polish_pomodoro_float_window(&window);
        Ok(window)
    }
}

fn attach_pomodoro_float_menu_handler(app: &AppHandle, window: &WebviewWindow) {
    let app_handle = app.clone();
    window.on_menu_event(move |_window, event| {
        if event.id.as_ref() == "pomodoro-float-hide" {
            let _ = hide_pomodoro_float_window(&app_handle);
        }
    });
}

#[tauri::command]
pub fn popup_pomodoro_float_menu(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(POMODORO_FLOAT_LABEL)
        .ok_or_else(|| "未找到番茄钟悬浮窗".to_string())?;
    let hide = MenuItem::with_id(&app, "pomodoro-float-hide", "关闭悬浮窗", true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let menu = Menu::with_items(&app, &[&hide]).map_err(|error| error.to_string())?;
    window.popup_menu(&menu).map_err(|error| error.to_string())
}

fn place_pomodoro_float_window(
    app: &AppHandle,
    window: &WebviewWindow,
    width: f64,
    height: f64,
) -> tauri::Result<()> {
    if let Some(state) = app.try_state::<crate::db::AppState>() {
        let settings = {
            let conn = state.db.lock();
            crate::db::load_settings(&conn)
        };

        if let (Some(x), Some(y)) = (settings.pomodoro_float_x, settings.pomodoro_float_y) {
            let _ = window.set_position(PhysicalPosition::new(x, y));
            return Ok(());
        }
    }

    if let Some(position) = default_pomodoro_float_position(app, width, height) {
        let _ = window.set_position(position);
    }

    Ok(())
}

fn default_pomodoro_float_position(
    app: &AppHandle,
    width: f64,
    height: f64,
) -> Option<PhysicalPosition<i32>> {
    let monitor = app.primary_monitor().ok()??;
    let position = monitor.position();
    let size = monitor.size();
    let scale = monitor.scale_factor();
    let window_width = (width * scale).round() as i32;
    let _window_height = (height * scale).round() as i32;
    let margin = (16.0 * scale).round() as i32;

    Some(PhysicalPosition::new(
        position.x + size.width as i32 - window_width - margin,
        position.y + margin,
    ))
}

pub fn polish_pomodoro_float_window(window: &WebviewWindow) {
    let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));

    #[cfg(target_os = "macos")]
    {
        let _ = window.set_shadow(true);
        polish_macos_pomodoro_float_window(window);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.set_shadow(false);
    }
}

pub fn build_pomodoro_float_window(
    app: &AppHandle,
    width: f64,
    height: f64,
) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        POMODORO_FLOAT_LABEL,
        WebviewUrl::App("/?view=pomodoro-float".into()),
    )
    .title("番茄钟")
    .inner_size(width, height)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .background_color(Color(0, 0, 0, 0))
    .shadow(cfg!(target_os = "macos"))
    .always_on_top(true)
    .skip_taskbar(true)
    .visible_on_all_workspaces(true)
    .visible(false)
    .focused(false)
    .build()
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

fn eye_care_background_color(dark: bool) -> Color {
    if dark {
        Color(20, 36, 30, 255)
    } else {
        Color(239, 251, 244, 255)
    }
}

#[tauri::command]
pub fn sync_eye_care_window_background(app: AppHandle, dark: bool) -> Result<(), String> {
    let color = eye_care_background_color(dark);
    for (label, window) in app.webview_windows() {
        if !is_eye_care_label(&label) {
            continue;
        }
        window
            .set_background_color(Some(color))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn present_eye_care_window(window: &WebviewWindow) -> tauri::Result<()> {
    let _ = window.set_always_on_top(true);
    let _ = window.set_shadow(false);
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
    let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));

    #[cfg(target_os = "macos")]
    {
        let _ = window.set_shadow(true);
        polish_macos_quick_todo_window(window);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.set_shadow(false);
    }
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
    .shadow(cfg!(target_os = "macos"))
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
const MACOS_OVERLAY_CORNER_RADIUS: f64 = 10.0;

#[cfg(target_os = "macos")]
fn polish_macos_pomodoro_float_window(window: &WebviewWindow) {
    let _ = window.with_webview(|webview| unsafe {
        apply_macos_native_overlay_window(webview.ns_window());
    });
}

#[cfg(target_os = "macos")]
unsafe fn apply_macos_native_overlay_window(ns_window: *mut std::ffi::c_void) {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    let ns_window = ns_window.cast::<Object>();
    if ns_window.is_null() {
        return;
    }

    let clear_color: *mut Object = msg_send![Class::get("NSColor").unwrap(), clearColor];
    let _: () = msg_send![ns_window, setBackgroundColor: clear_color];
    let _: () = msg_send![ns_window, setOpaque: false];
    let _: () = msg_send![ns_window, setHasShadow: true];

    let content_view: *mut Object = msg_send![ns_window, contentView];
    if content_view.is_null() {
        return;
    }

    let _: () = msg_send![content_view, setWantsLayer: true];
    let layer: *mut Object = msg_send![content_view, layer];
    if layer.is_null() {
        return;
    }

    let _: () = msg_send![layer, setCornerRadius: MACOS_OVERLAY_CORNER_RADIUS];
    let _: () = msg_send![layer, setMasksToBounds: true];
}

#[cfg(target_os = "macos")]
fn polish_macos_quick_todo_window(window: &WebviewWindow) {
    let _ = window.with_webview(|webview| unsafe {
        apply_macos_native_overlay_window(webview.ns_window());
    });
}
