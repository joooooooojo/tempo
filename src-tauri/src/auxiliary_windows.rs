use tauri::menu::{Menu, MenuItem};
use tauri::window::Color;
#[cfg(target_os = "macos")]
use tauri::PhysicalPosition;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
#[cfg(not(target_os = "macos"))]
use tauri::{Monitor, PhysicalPosition, PhysicalSize};

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const EYE_CARE_PRIMARY_LABEL: &str = "eye-care-reminder";
pub const POMODORO_FLOAT_LABEL: &str = "pomodoro-float";

pub const COMMAND_PALETTE_LABEL: &str = "command-palette";
pub const COMMAND_PALETTE_WIDTH: f64 = 800.0;
pub const COMMAND_PALETTE_APP_WIDTH: f64 = 920.0;
pub const COMMAND_PALETTE_INITIAL_HEIGHT: f64 = 370.0;
pub const COMMAND_PALETTE_MIN_HEIGHT: f64 = 58.0;
pub const COMMAND_PALETTE_MAX_HEIGHT: f64 = 760.0;

pub const POMODORO_FLOAT_PANEL_WIDTH: f64 = 300.0;
pub const POMODORO_FLOAT_PANEL_HEIGHT: f64 = 56.0;

pub const SHELF_PICKER_LABEL: &str = "shelf-picker";
pub const SHELF_HEIGHT: f64 = 292.0;
#[cfg(target_os = "windows")]
pub const SHELF_SIDE_MARGIN: f64 = 0.0;
#[cfg(not(target_os = "windows"))]
pub const SHELF_SIDE_MARGIN: f64 = 8.0;
#[cfg(not(target_os = "windows"))]
pub const SHELF_BOTTOM_MARGIN: f64 = 8.0;
pub const CLIPBOARD_SHELF_WIDTH_RATIO: f64 = 1.0;

const SHELF_SHORTCUT_DEBOUNCE_MS: u64 = 280;
const SHELF_TAB_NONE: u8 = 0;
const SHELF_TAB_CLIPBOARD: u8 = 1;
const SHELF_TAB_SNIPPETS: u8 = 2;

static LAST_SHELF_SHORTCUT_MS: AtomicU64 = AtomicU64::new(0);
static SHELF_VISIBLE_TAB: AtomicU8 = AtomicU8::new(SHELF_TAB_NONE);
#[cfg(target_os = "windows")]
static SHELF_OUTSIDE_CLOSE_TOKEN: AtomicU64 = AtomicU64::new(0);

pub fn command_palette_window_size() -> (f64, f64) {
    (COMMAND_PALETTE_WIDTH, COMMAND_PALETTE_INITIAL_HEIGHT)
}

pub fn pomodoro_float_window_size() -> (f64, f64) {
    (POMODORO_FLOAT_PANEL_WIDTH, POMODORO_FLOAT_PANEL_HEIGHT)
}

fn emit_to_debug<P>(app: &AppHandle, target: &str, event: &str, payload: P)
where
    P: serde::Serialize + Clone,
{
    crate::logging::debug_if_err(
        app.emit_to(target, event, payload),
        "emit auxiliary window event",
    );
}

fn emit_debug<P>(app: &AppHandle, event: &str, payload: P)
where
    P: serde::Serialize + Clone,
{
    crate::logging::debug_if_err(app.emit(event, payload), "emit auxiliary app event");
}

pub fn precache_auxiliary_windows(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(COMMAND_PALETTE_LABEL).is_none() {
        let (width, height) = command_palette_window_size();
        let window = build_command_palette_window(app, width, height)?;
        polish_command_palette_window(&window);
        crate::logging::debug_if_err(window.hide(), "precache hide command palette window");
    }

    // Pre-create eye-care overlays during startup so the first reminder does not
    // build a WebView inside the invoke handler (Windows WebView2 can deadlock IPC).
    #[cfg(target_os = "macos")]
    {
        if app.get_webview_window(EYE_CARE_PRIMARY_LABEL).is_none() {
            let window = build_eye_care_overlay_window(app, EYE_CARE_PRIMARY_LABEL)?;
            crate::logging::debug_if_err(window.hide(), "precache hide eye care window");
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let monitors = match ordered_monitors(app) {
            Ok(monitors) => monitors,
            Err(error) => {
                tracing::debug!(error = %error, "failed to resolve monitors for eye care precache");
                Vec::new()
            }
        };
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
            crate::logging::debug_if_err(window.hide(), "precache hide eye care window");
        }
    }

    if app.get_webview_window(POMODORO_FLOAT_LABEL).is_none() {
        let (width, height) = pomodoro_float_window_size();
        let window = build_pomodoro_float_window(app, width, height)?;
        attach_pomodoro_float_menu_handler(app, &window);
        polish_pomodoro_float_window(&window);
        crate::logging::debug_if_err(window.hide(), "precache hide pomodoro float window");
    }

    if app.get_webview_window(SHELF_PICKER_LABEL).is_none() {
        let window = build_shelf_picker_window(app)?;
        // macOS: defer native NSWindow tweaks to F4/F5 show time. Calling with_webview
        // or setFrame during did_finish_launching aborts the process.
        #[cfg(not(target_os = "macos"))]
        polish_shelf_picker_window(&window, true);
        #[cfg(target_os = "macos")]
        {
            crate::logging::debug_if_err(
                window.set_background_color(Some(Color(0, 0, 0, 0))),
                "set shelf picker transparent background",
            );
            crate::logging::debug_if_err(window.set_shadow(true), "set shelf picker shadow");
            apply_macos_shelf_vibrancy(&window);
        }
        crate::logging::debug_if_err(window.hide(), "precache hide shelf picker window");
    }

    Ok(())
}

pub fn show_command_palette(app: &AppHandle) -> tauri::Result<()> {
    let (default_width, default_height) = command_palette_window_size();
    let mut width = default_width;
    let mut height = default_height;
    let window = if let Some(window) = app.get_webview_window(COMMAND_PALETTE_LABEL) {
        // Keep the last size so restoring an app session (or search height)
        // does not flash through the default search dimensions.
        if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
            width = size.width as f64 / scale;
            height = size.height as f64 / scale;
        }
        window
    } else {
        let window = build_command_palette_window(app, default_width, default_height)?;
        polish_command_palette_window(&window);
        window
    };

    place_command_palette_window(app, &window, width, height, true)?;
    crate::logging::debug_if_err(
        window.set_always_on_top(true),
        "set command palette always on top",
    );
    polish_command_palette_window(&window);

    #[cfg(target_os = "macos")]
    {
        let config = crate::macos_overlay_panel::shelf_picker_config();
        crate::macos_overlay_panel::ensure_input_panel(
            app,
            &window,
            COMMAND_PALETTE_LABEL,
            &config,
        )?;
        crate::macos_overlay_panel::show_input_overlay(app, COMMAND_PALETTE_LABEL)?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        window.show()?;
        window.set_focus()?;
    }

    emit_to_debug(app, COMMAND_PALETTE_LABEL, "command-palette:open", ());
    Ok(())
}

pub fn is_command_palette_visible(app: &AppHandle) -> bool {
    app.get_webview_window(COMMAND_PALETTE_LABEL)
        .map(|window| window_is_visible(&window, "check command palette visibility"))
        .unwrap_or(false)
}

pub fn hide_command_palette(app: &AppHandle) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(COMMAND_PALETTE_LABEL) else {
        return Ok(());
    };

    #[cfg(target_os = "macos")]
    {
        let _ = window;
        crate::macos_overlay_panel::hide_overlay(app, COMMAND_PALETTE_LABEL);
    }

    #[cfg(not(target_os = "macos"))]
    {
        window.hide()?;
    }

    emit_to_debug(app, COMMAND_PALETTE_LABEL, "command-palette:shortcut-hide", ());
    Ok(())
}

pub fn toggle_command_palette(app: &AppHandle) -> tauri::Result<()> {
    if is_command_palette_visible(app) {
        hide_command_palette(app)
    } else {
        show_command_palette(app)
    }
}

#[tauri::command]
pub fn set_command_palette_height(app: AppHandle, height: f64) -> Result<(), String> {
    set_command_palette_size(app, None, height)
}

#[tauri::command]
pub fn set_command_palette_size(
    app: AppHandle,
    width: Option<f64>,
    height: f64,
) -> Result<(), String> {
    let window = app
        .get_webview_window(COMMAND_PALETTE_LABEL)
        .ok_or_else(|| "未找到快捷面板窗口".to_string())?;
    let requested_width = width.unwrap_or(COMMAND_PALETTE_WIDTH);
    place_command_palette_window(&app, &window, requested_width, height, false)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn show_command_palette_window(app: AppHandle) -> Result<(), String> {
    show_command_palette(&app).map_err(|error| error.to_string())
}

/// Prepare overlay panels so macOS NSOpenPanel sheets are visible (ZTools uses modal-panel
/// level; Status-level nonactivating panels hide / block the picker).
#[tauri::command]
pub fn prepare_native_file_dialog(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    crate::macos_overlay_panel::prepare_for_native_dialog(&app);
    let _ = &app;
    Ok(())
}

#[tauri::command]
pub fn restore_after_native_file_dialog(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    crate::macos_overlay_panel::restore_after_native_dialog(&app);
    let _ = &app;
    Ok(())
}

/// Re-apply macOS vibrancy / native theme for the command palette after the frontend
/// theme changes while the window is already open (ZTools-style light/dark frosted glass).
#[tauri::command]
pub fn sync_command_palette_appearance(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(COMMAND_PALETTE_LABEL) {
        polish_command_palette_window(&window);
    }
    Ok(())
}

fn place_command_palette_window(
    app: &AppHandle,
    window: &WebviewWindow,
    requested_width: f64,
    requested_height: f64,
    follow_cursor: bool,
) -> tauri::Result<()> {
    let max_width = requested_width
        .max(COMMAND_PALETTE_WIDTH)
        .min(COMMAND_PALETTE_APP_WIDTH.max(COMMAND_PALETTE_WIDTH));
    let cursor_monitor = || {
        app.cursor_position().ok().and_then(|position| {
            app.monitor_from_point(position.x, position.y)
                .ok()
                .flatten()
        })
    };
    let monitor = follow_cursor
        .then(cursor_monitor)
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(cursor_monitor)
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        let height = requested_height.clamp(COMMAND_PALETTE_MIN_HEIGHT, COMMAND_PALETTE_MAX_HEIGHT);
        let width = requested_width.clamp(320.0, max_width);
        window.set_size(LogicalSize::new(width, height))?;
        return window.center();
    };

    let scale = monitor.scale_factor();
    let work_area = monitor.work_area();
    let available_width = work_area.size.width as f64 / scale;
    let available_height = work_area.size.height as f64 / scale;
    let width = (available_width - 32.0).clamp(320.0, max_width.min(requested_width.max(320.0)));
    let top_offset = ((available_height - COMMAND_PALETTE_MAX_HEIGHT) / 2.0).clamp(96.0, 320.0);
    let max_height = COMMAND_PALETTE_MAX_HEIGHT
        .min((available_height - top_offset - 24.0).max(COMMAND_PALETTE_MIN_HEIGHT));
    let height = requested_height.clamp(COMMAND_PALETTE_MIN_HEIGHT, max_height);

    window.set_size(LogicalSize::new(width, height))?;
    let physical_width = (width * scale).round() as i32;
    let x = work_area.position.x + (work_area.size.width as i32 - physical_width) / 2;
    let y = work_area.position.y + (top_offset * scale).round() as i32;
    window.set_position(PhysicalPosition::new(x, y))
}

#[tauri::command]
pub fn show_clipboard_picker(app: AppHandle) -> Result<(), String> {
    show_clipboard_picker_window(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn show_snippet_picker(app: AppHandle) -> Result<(), String> {
    show_snippet_picker_window(&app).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn hide_shelf_picker(app: AppHandle) -> Result<(), String> {
    hide_shelf_picker_window(&app).map_err(|error| error.to_string())
}

pub fn is_shelf_picker_visible(app: &AppHandle) -> bool {
    app.get_webview_window(SHELF_PICKER_LABEL)
        .map(|window| window_is_visible(&window, "check shelf picker visibility"))
        .unwrap_or(false)
}

pub fn hide_shelf_picker_window(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        crate::macos_overlay_panel::remove_shelf_outside_click_monitor();
        crate::macos_overlay_panel::hide_overlay(app, SHELF_PICKER_LABEL);
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(window) = app.get_webview_window(SHELF_PICKER_LABEL) {
            crate::logging::debug_if_err(window.hide(), "hide shelf picker window");
        }
    }

    emit_to_debug(app, SHELF_PICKER_LABEL, "shelf-picker:hide", ());
    SHELF_VISIBLE_TAB.store(SHELF_TAB_NONE, Ordering::Relaxed);
    #[cfg(target_os = "windows")]
    SHELF_OUTSIDE_CLOSE_TOKEN.fetch_add(1, Ordering::Relaxed);
    if let Err(error) = crate::unregister_shelf_escape_shortcut(app) {
        tracing::warn!(error = %error, "failed to unregister shelf Escape shortcut");
    }
    Ok(())
}

fn shelf_tab_id(tab: ShelfPickerTab) -> u8 {
    match tab {
        ShelfPickerTab::Clipboard => SHELF_TAB_CLIPBOARD,
        ShelfPickerTab::Snippets => SHELF_TAB_SNIPPETS,
    }
}

fn on_shelf_picker_shown(_app: &AppHandle, _window: &WebviewWindow, tab: ShelfPickerTab) {
    SHELF_VISIBLE_TAB.store(shelf_tab_id(tab), Ordering::Relaxed);
}

fn window_is_visible(window: &WebviewWindow, operation: &'static str) -> bool {
    match window.is_visible() {
        Ok(visible) => visible,
        Err(error) => {
            tracing::debug!(
                operation = %operation,
                error = %error,
                "failed to read window visibility"
            );
            false
        }
    }
}

fn show_shelf_window_without_stealing_focus(
    app: &AppHandle,
    label: &str,
    window: &WebviewWindow,
) -> tauri::Result<()> {
    #[cfg(not(target_os = "macos"))]
    let _ = (app, label);

    #[cfg(target_os = "macos")]
    {
        let config = crate::macos_overlay_panel::shelf_picker_config();
        crate::macos_overlay_panel::ensure_input_panel(app, window, label, &config)?;
        crate::macos_overlay_panel::show_input_overlay(app, label)?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        show_window_without_activation(window)
    }
}

#[cfg(not(target_os = "macos"))]
fn show_window_without_activation(window: &WebviewWindow) -> tauri::Result<()> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};

        window.show()?;
        if let Some(hwnd) = windows_hwnd(window) {
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    window.show()
}

#[derive(serde::Serialize)]
struct ShelfPickerTabPayload {
    tab: &'static str,
}

#[derive(Copy, Clone)]
enum ShelfPickerTab {
    Clipboard,
    Snippets,
}

fn shelf_picker_tab_name(tab: ShelfPickerTab) -> &'static str {
    match tab {
        ShelfPickerTab::Clipboard => "clipboard",
        ShelfPickerTab::Snippets => "snippets",
    }
}

pub fn show_clipboard_picker_window(app: &AppHandle) -> tauri::Result<()> {
    show_shelf_picker_window(app, ShelfPickerTab::Clipboard)
}

pub fn show_snippet_picker_window(app: &AppHandle) -> tauri::Result<()> {
    show_shelf_picker_window(app, ShelfPickerTab::Snippets)
}

fn show_shelf_picker_window(app: &AppHandle, tab: ShelfPickerTab) -> tauri::Result<()> {
    let window = if let Some(window) = app.get_webview_window(SHELF_PICKER_LABEL) {
        window
    } else {
        let window = build_shelf_picker_window(app)?;
        polish_shelf_picker_window(&window, true);
        window
    };

    let payload = ShelfPickerTabPayload {
        tab: shelf_picker_tab_name(tab),
    };

    if window_is_visible(&window, "check shelf picker visibility before show") {
        if !consume_shelf_shortcut_action() {
            return Ok(());
        }

        let tab_id = shelf_tab_id(tab);
        let current = SHELF_VISIBLE_TAB.load(Ordering::Relaxed);
        if current == tab_id {
            return hide_shelf_picker_window(app);
        }

        SHELF_VISIBLE_TAB.store(tab_id, Ordering::Relaxed);
        emit_to_debug(app, SHELF_PICKER_LABEL, "shelf-picker:activate", &payload);
        return Ok(());
    }

    if !consume_shelf_shortcut_action() {
        return Ok(());
    }

    emit_to_debug(app, SHELF_PICKER_LABEL, "shelf-picker:prepare", &payload);
    place_bottom_shelf_window(app, &window, CLIPBOARD_SHELF_WIDTH_RATIO)?;
    #[cfg(not(target_os = "macos"))]
    {
        crate::logging::debug_if_err(
            window.set_always_on_top(true),
            "set shelf picker always on top",
        );
    }
    polish_shelf_picker_window(&window, true);
    show_shelf_window_without_stealing_focus(app, SHELF_PICKER_LABEL, &window)?;
    #[cfg(target_os = "macos")]
    {
        crate::macos_overlay_panel::install_shelf_outside_click_monitor(app, SHELF_PICKER_LABEL);
    }
    #[cfg(target_os = "windows")]
    {
        align_windows_shelf_client_to_monitor(app, &window, CLIPBOARD_SHELF_WIDTH_RATIO);
        start_windows_shelf_outside_click_watcher(app, &window);
    }
    on_shelf_picker_shown(app, &window, tab);
    if let Err(error) = crate::register_shelf_escape_shortcut(app) {
        tracing::warn!(error = %error, "failed to register shelf Escape shortcut");
    }
    emit_to_debug(app, SHELF_PICKER_LABEL, "shelf-picker:open", &payload);
    Ok(())
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

    app.primary_monitor()?.ok_or(tauri::Error::WindowNotFound)
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
        (position, size.width as f64, size.height as f64)
    };

    #[cfg(target_os = "windows")]
    let (area_pos, area_w, area_h) = {
        let position = monitor.position();
        let size = monitor.size();
        (position, size.width as f64, size.height as f64)
    };

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let (area_pos, area_w, area_h) = {
        let work = monitor.work_area();
        (
            work.position,
            work.size.width as f64,
            work.size.height as f64,
        )
    };

    let side_margin = (SHELF_SIDE_MARGIN * scale).round() as i32;
    #[cfg(target_os = "windows")]
    let bottom_margin = 0;
    #[cfg(not(target_os = "windows"))]
    let bottom_margin = (SHELF_BOTTOM_MARGIN * scale).round() as i32;
    let full_width = (width_ratio - 1.0).abs() < f64::EPSILON;

    let width = if full_width {
        (area_w / scale) - SHELF_SIDE_MARGIN * 2.0
    } else {
        (area_w / scale) * width_ratio
    };
    let height = SHELF_HEIGHT;
    crate::logging::debug_if_err(
        window.set_size(LogicalSize::new(width, height)),
        "size shelf picker window",
    );
    let x = if full_width {
        area_pos.x + side_margin
    } else {
        area_pos.x + ((area_w - width * scale) / 2.0).round() as i32
    };
    let y = area_pos.y + (area_h - height * scale).round() as i32 - bottom_margin;
    crate::logging::debug_if_err(
        window.set_position(PhysicalPosition::new(x, y)),
        "position shelf picker window",
    );
    Ok(())
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
struct WindowsShelfTarget {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

#[cfg(target_os = "windows")]
fn windows_shelf_target(
    app: &AppHandle,
    window: &WebviewWindow,
    width_ratio: f64,
) -> Option<WindowsShelfTarget> {
    let monitor = match shelf_monitor(app, window) {
        Ok(monitor) => monitor,
        Err(error) => {
            tracing::debug!(error = %error, "failed to resolve shelf picker monitor");
            return None;
        }
    };
    let scale = monitor.scale_factor();
    let position = monitor.position();
    let size = monitor.size();
    let side_margin = (SHELF_SIDE_MARGIN * scale).round() as i32;
    let full_width = (width_ratio - 1.0).abs() < f64::EPSILON;
    let width = if full_width {
        size.width as i32 - side_margin * 2
    } else {
        (size.width as f64 * width_ratio).round() as i32
    };
    let height = (SHELF_HEIGHT * scale).round() as i32;
    let left = if full_width {
        position.x + side_margin
    } else {
        position.x + ((size.width as f64 - width as f64) / 2.0).round() as i32
    };
    let top = position.y + size.height as i32 - height;

    if width <= 0 || height <= 0 {
        return None;
    }

    Some(WindowsShelfTarget {
        left,
        top,
        width,
        height,
    })
}

#[cfg(target_os = "windows")]
fn align_windows_shelf_client_to_monitor(
    app: &AppHandle,
    window: &WebviewWindow,
    width_ratio: f64,
) {
    let Some(target) = windows_shelf_target(app, window, width_ratio) else {
        return;
    };
    let Some(hwnd) = windows_hwnd(window) else {
        return;
    };
    align_windows_shelf_client_to_target(hwnd, target);
}

#[cfg(target_os = "windows")]
fn align_windows_shelf_client_to_target(
    hwnd: windows::Win32::Foundation::HWND,
    target: WindowsShelfTarget,
) {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowRect, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE,
        SWP_NOOWNERZORDER, SWP_SHOWWINDOW,
    };

    unsafe {
        let mut window_rect = RECT::default();
        if GetWindowRect(hwnd, &mut window_rect).is_err() {
            return;
        }

        let mut client_rect = RECT::default();
        if GetClientRect(hwnd, &mut client_rect).is_err() {
            return;
        }

        let mut client_origin = POINT { x: 0, y: 0 };
        if !ClientToScreen(hwnd, &mut client_origin).as_bool() {
            return;
        }

        let client_width = client_rect.right - client_rect.left;
        let client_height = client_rect.bottom - client_rect.top;
        if client_width <= 0 || client_height <= 0 {
            return;
        }

        let client_right = client_origin.x + client_width;
        let client_bottom = client_origin.y + client_height;
        let left_inset = client_origin.x - window_rect.left;
        let top_inset = client_origin.y - window_rect.top;
        let right_inset = window_rect.right - client_right;
        let bottom_inset = window_rect.bottom - client_bottom;

        let window_left = target.left - left_inset;
        let window_top = target.top - top_inset;
        let window_width = target.width + left_inset + right_inset;
        let window_height = target.height + top_inset + bottom_inset;

        if window_width <= 0 || window_height <= 0 {
            return;
        }

        if let Err(error) = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            window_left,
            window_top,
            window_width,
            window_height,
            SWP_SHOWWINDOW | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        ) {
            tracing::debug!(error = %error, "failed to align windows shelf client");
        }
    }
}

#[cfg(target_os = "windows")]
fn start_windows_shelf_outside_click_watcher(app: &AppHandle, window: &WebviewWindow) {
    let Some(hwnd) = windows_hwnd(window) else {
        return;
    };
    let hwnd_value = hwnd.0 as isize;
    let app = app.clone();
    let token = SHELF_OUTSIDE_CLOSE_TOKEN.fetch_add(1, Ordering::Relaxed) + 1;

    crate::logging::spawn_named("tempo-shelf-outside-click-watcher", move || {
        let hwnd = windows::Win32::Foundation::HWND(hwnd_value as *mut _);
        let mut previous_buttons = windows_pressed_mouse_buttons();

        loop {
            if SHELF_OUTSIDE_CLOSE_TOKEN.load(Ordering::Relaxed) != token {
                break;
            }

            let visible = app
                .get_webview_window(SHELF_PICKER_LABEL)
                .map(|window| window_is_visible(&window, "check shelf picker watcher visibility"))
                .unwrap_or(false);
            if !visible {
                break;
            }

            let buttons = windows_pressed_mouse_buttons();
            let newly_pressed = buttons & !previous_buttons;
            previous_buttons = buttons;

            if newly_pressed != 0 && !windows_cursor_is_over_shelf(hwnd) {
                crate::logging::debug_if_err(
                    hide_shelf_picker_window(&app),
                    "hide shelf picker from outside click watcher",
                );
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    });
}

#[cfg(target_os = "windows")]
fn windows_pressed_mouse_buttons() -> u8 {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
    };

    fn pressed(vkey: u16) -> bool {
        unsafe { (GetAsyncKeyState(vkey as i32) as u16 & 0x8000) != 0 }
    }

    let mut buttons = 0;
    if pressed(VK_LBUTTON.0) {
        buttons |= 1;
    }
    if pressed(VK_RBUTTON.0) {
        buttons |= 2;
    }
    if pressed(VK_MBUTTON.0) {
        buttons |= 4;
    }
    buttons
}

#[cfg(target_os = "windows")]
fn windows_cursor_is_over_shelf(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetCursorPos, GetWindowRect, IsChild, WindowFromPoint, GA_ROOT,
    };

    unsafe {
        let mut point = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut point).is_err() {
            return true;
        }

        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }

        if point.x < rect.left
            || point.x >= rect.right
            || point.y < rect.top
            || point.y >= rect.bottom
        {
            return false;
        }

        let hit = WindowFromPoint(point);
        if hit == hwnd || IsChild(hwnd, hit).as_bool() {
            return true;
        }

        let root = GetAncestor(hit, GA_ROOT);
        root == hwnd
    }
}

fn build_shelf_picker_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(
        app,
        SHELF_PICKER_LABEL,
        WebviewUrl::App("/?view=shelf-picker".into()),
    )
    .title("")
    .inner_size(960.0, SHELF_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .background_color(Color(0, 0, 0, 0))
    .shadow(cfg!(any(target_os = "macos", target_os = "windows")))
    .skip_taskbar(true)
    .visible_on_all_workspaces(true)
    .visible(false)
    .focused(false);

    #[cfg(not(target_os = "macos"))]
    let builder = builder.focusable(false).always_on_top(true);

    let window = builder.build()?;

    Ok(window)
}

pub fn polish_shelf_picker_window(window: &WebviewWindow, topmost: bool) {
    crate::logging::debug_if_err(
        window.set_background_color(Some(Color(0, 0, 0, 0))),
        "set shelf picker transparent background",
    );

    #[cfg(target_os = "macos")]
    {
        crate::logging::debug_if_err(window.set_shadow(true), "set shelf picker shadow");
        polish_macos_shelf_picker_window(window, topmost);
    }

    #[cfg(target_os = "windows")]
    {
        crate::logging::debug_if_err(window.set_shadow(true), "set shelf picker shadow");
        apply_windows_shelf_appearance(window);
        let _ = topmost;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        crate::logging::debug_if_err(window.set_shadow(false), "unset shelf picker shadow");
        let _ = topmost;
    }
}

#[cfg(target_os = "macos")]
const MACOS_SHELF_WINDOW_LEVEL: i64 = 25;

#[cfg(target_os = "windows")]
fn apply_windows_shelf_appearance(window: &WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT,
    };

    let Some(hwnd) = windows_hwnd(window) else {
        return;
    };

    let border_color = DWMWA_COLOR_DEFAULT;
    unsafe {
        if let Err(error) = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border_color as *const _ as *const _,
            std::mem::size_of_val(&border_color) as u32,
        ) {
            tracing::debug!(error = %error, "failed to apply windows shelf border appearance");
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_hwnd(window: &WebviewWindow) -> Option<windows::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;

    let Ok(handle) = window.window_handle() else {
        return None;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };

    Some(HWND(handle.hwnd.get() as *mut _))
}

#[cfg(target_os = "macos")]
const MACOS_SHELF_CORNER_RADIUS: f64 = 16.0;

#[cfg(target_os = "macos")]
fn apply_macos_shelf_vibrancy(window: &WebviewWindow) {
    use window_vibrancy::{
        apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial, NSVisualEffectState,
    };

    crate::logging::debug_if_err(clear_vibrancy(window), "clear shelf picker macos vibrancy");
    // Popover follows the window's effective appearance (light/dark). HudWindow is a dark
    // HUD material and stays unreadable under Tempo's light-theme foreground colors.
    crate::logging::debug_if_err(
        apply_vibrancy(
            window,
            NSVisualEffectMaterial::Popover,
            Some(NSVisualEffectState::Active),
            Some(MACOS_SHELF_CORNER_RADIUS),
        ),
        "apply shelf picker macos vibrancy",
    );
}

#[cfg(target_os = "macos")]
fn polish_macos_shelf_picker_window(window: &WebviewWindow, topmost: bool) {
    sync_overlay_window_theme(window);
    apply_macos_shelf_vibrancy(window);

    crate::logging::debug_if_err(
        window.with_webview(move |webview| unsafe {
            apply_macos_shelf_appearance(webview.ns_window(), topmost);
        }),
        "apply shelf picker macos native appearance",
    );
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
}

pub fn is_pomodoro_float_visible(app: &AppHandle) -> bool {
    app.get_webview_window(POMODORO_FLOAT_LABEL)
        .map(|window| window_is_visible(&window, "check pomodoro float visibility"))
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

    crate::logging::debug_if_err(
        window.set_size(LogicalSize::new(width, height)),
        "size pomodoro float window",
    );
    place_pomodoro_float_window(app, &window, width, height)?;
    crate::logging::debug_if_err(
        window.set_always_on_top(true),
        "set pomodoro float always on top",
    );
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
    emit_debug(app, "pomodoro-float-visible", visible);
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
            crate::logging::warn_if_err(
                hide_pomodoro_float_window(&app_handle),
                "hide pomodoro float from menu",
            );
        }
    });
}

#[tauri::command]
pub fn popup_pomodoro_float_menu(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(POMODORO_FLOAT_LABEL)
        .ok_or_else(|| "未找到番茄钟悬浮窗".to_string())?;
    let hide = MenuItem::with_id(
        &app,
        "pomodoro-float-hide",
        "关闭悬浮窗",
        true,
        None::<&str>,
    )
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
            crate::logging::debug_if_err(
                window.set_position(PhysicalPosition::new(x, y)),
                "position pomodoro float window from settings",
            );
            return Ok(());
        }
    }

    if let Some(position) = default_pomodoro_float_position(app, width, height) {
        crate::logging::debug_if_err(
            window.set_position(position),
            "position pomodoro float window",
        );
    }

    Ok(())
}

fn default_pomodoro_float_position(
    app: &AppHandle,
    width: f64,
    height: f64,
) -> Option<PhysicalPosition<i32>> {
    let monitor = match app.primary_monitor() {
        Ok(Some(monitor)) => monitor,
        Ok(None) => {
            tracing::debug!("no primary monitor available for pomodoro float");
            return None;
        }
        Err(error) => {
            tracing::debug!(error = %error, "failed to resolve primary monitor for pomodoro float");
            return None;
        }
    };
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
    crate::logging::debug_if_err(
        window.set_background_color(Some(Color(0, 0, 0, 0))),
        "set pomodoro float transparent background",
    );

    #[cfg(target_os = "macos")]
    {
        crate::logging::debug_if_err(window.set_shadow(true), "set pomodoro float shadow");
        polish_macos_pomodoro_float_window(window);
    }

    #[cfg(target_os = "windows")]
    {
        crate::logging::debug_if_err(window.set_shadow(true), "set pomodoro float shadow");
        apply_windows_shelf_appearance(window);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        crate::logging::debug_if_err(window.set_shadow(false), "unset pomodoro float shadow");
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
    .shadow(cfg!(any(target_os = "macos", target_os = "windows")))
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
        crate::logging::debug_if_err(
            window.set_simple_fullscreen(true),
            "set eye care fullscreen",
        );
        polish_macos_eye_care_overlay(&window);
        present_eye_care_window(&window)?;
        emit_debug(app, "eye-care:reveal", ());
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
            crate::logging::debug_if_err(primary.set_focus(), "focus primary eye care overlay");
        }

        emit_debug(app, "eye-care:reveal", ());
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
        crate::logging::debug_if_err(
            window.set_simple_fullscreen(false),
            "unset eye care fullscreen",
        );

        if label == EYE_CARE_PRIMARY_LABEL {
            window.hide().map_err(|error| error.to_string())?;
        } else {
            // Keep secondary overlays around so the next reminder does not recreate
            // WebViews inside the invoke handler (Windows WebView2 can deadlock IPC).
            crate::logging::debug_if_err(window.hide(), "hide secondary eye care overlay");
        }
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
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
    crate::logging::debug_if_err(
        window.set_always_on_top(true),
        "set eye care overlay always on top",
    );
    crate::logging::debug_if_err(window.set_shadow(false), "unset eye care overlay shadow");
    window.show()?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn ordered_monitors(app: &AppHandle) -> tauri::Result<Vec<Monitor>> {
    let mut monitors = app.available_monitors()?;
    if let Some(primary) = app.primary_monitor()? {
        if let Some(index) = monitors
            .iter()
            .position(|monitor| same_monitor(monitor, &primary))
        {
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
    crate::logging::debug_if_err(
        window.set_position(PhysicalPosition::new(position.x, position.y)),
        "position eye care overlay",
    );
    crate::logging::debug_if_err(
        window.set_size(PhysicalSize::new(size.width, size.height)),
        "size eye care overlay",
    );
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
            crate::logging::debug_if_err(window.close(), "close stale eye care overlay");
            continue;
        };

        if index >= active_count {
            crate::logging::debug_if_err(window.close(), "close inactive eye care overlay");
        }
    }
}

pub fn polish_command_palette_window(window: &WebviewWindow) {
    sync_overlay_window_theme(window);

    #[cfg(target_os = "macos")]
    {
        crate::logging::debug_if_err(
            window.set_background_color(Some(Color(0, 0, 0, 0))),
            "set command palette transparent background",
        );
        crate::logging::debug_if_err(window.set_shadow(true), "set command palette shadow");
        apply_macos_command_palette_vibrancy(window);
    }

    #[cfg(target_os = "windows")]
    {
        let background = match window.theme() {
            Ok(tauri::Theme::Dark) => Color(18, 20, 24, 255),
            _ => Color(247, 249, 248, 255),
        };
        crate::logging::debug_if_err(
            window.set_background_color(Some(background)),
            "set command palette opaque background",
        );
        crate::logging::debug_if_err(window.set_shadow(true), "set command palette shadow");
        apply_windows_shelf_appearance(window);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        crate::logging::debug_if_err(window.set_shadow(true), "set command palette shadow");
    }
}

/// Keep native window appearance in sync with Tempo's theme setting so macOS vibrancy
/// materials (Popover/Menu) and Windows solid fills match CSS light/dark tokens.
fn sync_overlay_window_theme(window: &WebviewWindow) {
    let theme = window
        .app_handle()
        .try_state::<crate::db::AppState>()
        .map(|state| {
            let conn = state.db.lock();
            crate::db::get_setting(&conn, "theme", "system")
        })
        .unwrap_or_else(|| "system".into());

    let native = match theme.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    };
    crate::logging::debug_if_err(window.set_theme(native), "sync overlay window theme");
}

pub fn build_command_palette_window(
    app: &AppHandle,
    width: f64,
    height: f64,
) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        COMMAND_PALETTE_LABEL,
        WebviewUrl::App("/?view=command-palette".into()),
    )
    .title("快捷面板")
    .inner_size(width, height)
    .resizable(false)
    .decorations(false)
    .transparent(cfg!(target_os = "macos"))
    .background_color(if cfg!(target_os = "macos") {
        Color(0, 0, 0, 0)
    } else {
        Color(247, 249, 248, 255)
    })
    .shadow(cfg!(any(target_os = "macos", target_os = "windows")))
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(false)
    .center()
    .build()
}

fn build_eye_care_overlay_window(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("/?view=eye-care".into()))
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
    crate::logging::debug_if_err(
        window.with_webview(|webview| unsafe {
            apply_macos_overlay_level(webview.ns_window());
        }),
        "apply eye care macos overlay level",
    );
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
    crate::logging::debug_if_err(
        window.with_webview(|webview| unsafe {
            apply_macos_native_overlay_window(webview.ns_window());
        }),
        "apply pomodoro float macos native appearance",
    );
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
fn apply_macos_command_palette_vibrancy(window: &WebviewWindow) {
    use window_vibrancy::{
        apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial, NSVisualEffectState,
    };

    crate::logging::debug_if_err(clear_vibrancy(window), "clear command palette vibrancy");
    // Match ZTools / uTools style: Electron `vibrancy: 'fullscreen-ui'` + a light/dark CSS wash
    // on top (see `styles/platform/macos/command-palette.css`). FullScreenUI follows the
    // window appearance; HudWindow stays dark and washed out light-theme labels.
    crate::logging::debug_if_err(
        apply_vibrancy(
            window,
            NSVisualEffectMaterial::FullScreenUI,
            Some(NSVisualEffectState::Active),
            Some(16.0),
        ),
        "apply command palette vibrancy",
    );
}
