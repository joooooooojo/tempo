use tauri::window::Color;
#[cfg(target_os = "macos")]
use tauri::PhysicalPosition;
#[cfg(not(target_os = "macos"))]
use tauri::PhysicalPosition;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, Monitor, PhysicalSize, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAIN_PANEL_LABEL: &str = "main-panel";
pub const MAIN_PANEL_WIDTH: f64 = 800.0;
pub const MAIN_PANEL_MAX_WIDTH: f64 = 920.0;
pub const MAIN_PANEL_INITIAL_HEIGHT: f64 = 370.0;
pub const MAIN_PANEL_MIN_HEIGHT: f64 = 58.0;
pub const MAIN_PANEL_MAX_HEIGHT: f64 = 760.0;
const MAIN_PANEL_POSITION_SETTING: &str = "main_panel_position";

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

pub fn main_panel_window_size() -> (f64, f64) {
    (MAIN_PANEL_WIDTH, MAIN_PANEL_INITIAL_HEIGHT)
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

pub fn precache_auxiliary_windows(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(MAIN_PANEL_LABEL).is_none() {
        let (width, height) = main_panel_window_size();
        let window = build_main_panel_window(app, width, height)?;
        polish_main_panel_window(&window);
        crate::logging::debug_if_err(window.hide(), "precache hide main panel window");
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

    crate::launcher_context_menu::precache(app)?;

    Ok(())
}

pub fn show_main_panel(app: &AppHandle) -> tauri::Result<()> {
    let (default_width, default_height) = main_panel_window_size();
    let mut width = default_width;
    let mut height = default_height;
    let window = if let Some(window) = app.get_webview_window(MAIN_PANEL_LABEL) {
        // Keep the last size so restoring an app session (or search height)
        // does not flash through the default search dimensions.
        if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
            width = size.width as f64 / scale;
            height = size.height as f64 / scale;
        }
        window
    } else {
        let window = build_main_panel_window(app, default_width, default_height)?;
        polish_main_panel_window(&window);
        window
    };

    if !place_main_panel_at_saved_position(app, &window, width, height)? {
        place_main_panel_window(app, &window, width, height, true)?;
    }
    crate::logging::debug_if_err(
        window.set_always_on_top(true),
        "set main panel always on top",
    );
    polish_main_panel_window(&window);

    #[cfg(target_os = "macos")]
    {
        let config = crate::macos_overlay_panel::main_panel_config();
        crate::macos_overlay_panel::ensure_input_panel(app, &window, MAIN_PANEL_LABEL, &config)?;
        crate::macos_overlay_panel::show_input_overlay(app, MAIN_PANEL_LABEL)?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        window.show()?;
        window.set_focus()?;
    }

    emit_to_debug(app, MAIN_PANEL_LABEL, "main-panel:open", ());
    crate::commands::launcher::request_launcher_index_refresh(app);
    Ok(())
}

pub fn is_main_panel_visible(app: &AppHandle) -> bool {
    app.get_webview_window(MAIN_PANEL_LABEL)
        .map(|window| window_is_visible(&window, "check main panel visibility"))
        .unwrap_or(false)
}

pub fn hide_main_panel(app: &AppHandle) -> tauri::Result<()> {
    crate::launcher_context_menu::hide_with_main_panel(app);

    let Some(window) = app.get_webview_window(MAIN_PANEL_LABEL) else {
        return Ok(());
    };

    #[cfg(target_os = "macos")]
    {
        let _ = window;
        crate::macos_overlay_panel::hide_overlay(app, MAIN_PANEL_LABEL);
    }

    #[cfg(not(target_os = "macos"))]
    {
        window.hide()?;
    }

    emit_to_debug(app, MAIN_PANEL_LABEL, "main-panel:shortcut-hide", ());
    Ok(())
}

pub fn toggle_main_panel(app: &AppHandle) -> tauri::Result<()> {
    if is_main_panel_visible(app) {
        hide_main_panel(app)
    } else {
        show_main_panel(app)
    }
}

#[tauri::command]
pub fn set_main_panel_height(app: AppHandle, height: f64) -> Result<(), String> {
    set_main_panel_size(app, None, height)
}

#[tauri::command]
pub fn set_main_panel_size(app: AppHandle, width: Option<f64>, height: f64) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_PANEL_LABEL)
        .ok_or_else(|| "未找到主面板窗口".to_string())?;
    // Search mode passes width=None and only changes height while typing. Avoid
    // monitor/cursor/position work on every keystroke — that was a major input lag source.
    if width.is_none() {
        return resize_main_panel_height_only(&app, &window, height)
            .map_err(|error| error.to_string());
    }
    let requested_width = width.unwrap_or(MAIN_PANEL_WIDTH);
    place_main_panel_window(&app, &window, requested_width, height, false)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_main_panel_rect(
    app: AppHandle,
    rect: crate::plugins::manifest::AppRect,
) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_PANEL_LABEL)
        .ok_or_else(|| "未找到主面板窗口".to_string())?;
    let current_position = window.outer_position().map_err(|error| error.to_string())?;
    let resolved = crate::plugins::windows::resolve_window_rect(&app, Some(&window), &rect)?;
    let position = crate::plugins::windows::preserve_omitted_rect_position(
        &rect,
        resolved.position,
        current_position,
    );
    window
        .set_size(resolved.physical_size)
        .map_err(|error| error.to_string())?;
    window
        .set_position(position)
        .map_err(|error| error.to_string())
}

fn ensure_main_panel_caller(window: &WebviewWindow) -> Result<(), String> {
    if window.label() != MAIN_PANEL_LABEL {
        return Err("仅主面板可以调整主面板位置".to_string());
    }
    Ok(())
}

#[derive(serde::Serialize)]
pub struct MainPanelPosition {
    x: f64,
    y: f64,
}

#[tauri::command]
pub fn get_main_panel_position(window: WebviewWindow) -> Result<MainPanelPosition, String> {
    ensure_main_panel_caller(&window)?;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let position = window
        .outer_position()
        .map_err(|error| error.to_string())?
        .to_logical::<f64>(scale);
    Ok(MainPanelPosition {
        x: position.x,
        y: position.y,
    })
}

#[tauri::command]
pub fn set_main_panel_position(window: WebviewWindow, x: f64, y: f64) -> Result<(), String> {
    ensure_main_panel_caller(&window)?;
    window
        .set_position(tauri::LogicalPosition::new(x.round(), y.round()))
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
struct StoredMainPanelPosition {
    x: i32,
    y: i32,
}

#[tauri::command]
pub fn save_main_panel_position(
    window: WebviewWindow,
    state: State<'_, crate::db::AppState>,
) -> Result<(), String> {
    ensure_main_panel_caller(&window)?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let stored = StoredMainPanelPosition {
        x: position.x,
        y: position.y,
    };
    let value = serde_json::to_string(&stored).map_err(|error| error.to_string())?;
    let conn = state.db.lock();
    crate::db::set_setting(&conn, MAIN_PANEL_POSITION_SETTING, &value);
    Ok(())
}

fn load_main_panel_position(app: &AppHandle) -> Option<PhysicalPosition<i32>> {
    let state = app.try_state::<crate::db::AppState>()?;
    let conn = state.db.lock();
    let value = crate::db::get_setting(&conn, MAIN_PANEL_POSITION_SETTING, "");
    let stored = serde_json::from_str::<StoredMainPanelPosition>(&value).ok()?;
    Some(PhysicalPosition::new(stored.x, stored.y))
}

pub(crate) fn monitor_containing_position(
    app: &AppHandle,
    position: PhysicalPosition<i32>,
) -> Option<Monitor> {
    app.available_monitors().ok()?.into_iter().find(|monitor| {
        let origin = monitor.position();
        let size = monitor.size();
        position.x >= origin.x
            && position.y >= origin.y
            && i64::from(position.x) < i64::from(origin.x) + i64::from(size.width)
            && i64::from(position.y) < i64::from(origin.y) + i64::from(size.height)
    })
}

pub(crate) fn clamp_position_to_monitor(
    position: PhysicalPosition<i32>,
    window_size: PhysicalSize<u32>,
    monitor: &Monitor,
) -> PhysicalPosition<i32> {
    let work_area = monitor.work_area();
    let max_x = i64::from(work_area.position.x)
        + i64::from(work_area.size.width.saturating_sub(window_size.width));
    let max_y = i64::from(work_area.position.y)
        + i64::from(work_area.size.height.saturating_sub(window_size.height));
    PhysicalPosition::new(
        i64::from(position.x).clamp(i64::from(work_area.position.x), max_x) as i32,
        i64::from(position.y).clamp(i64::from(work_area.position.y), max_y) as i32,
    )
}

fn place_main_panel_at_saved_position(
    app: &AppHandle,
    window: &WebviewWindow,
    width: f64,
    height: f64,
) -> tauri::Result<bool> {
    let Some(position) = load_main_panel_position(app) else {
        return Ok(false);
    };
    let Some(monitor) = monitor_containing_position(app, position) else {
        return Ok(false);
    };

    window.set_position(position)?;
    place_main_panel_window(app, window, width, height, false)?;
    let window_size = window.outer_size()?;
    window.set_position(clamp_position_to_monitor(position, window_size, &monitor))?;
    Ok(true)
}

fn resize_main_panel_height_only(
    app: &AppHandle,
    window: &WebviewWindow,
    requested_height: f64,
) -> tauri::Result<()> {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let (width, height) = if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let work_area = monitor.work_area();
        let available_height = work_area.size.height as f64 / scale;
        let top_offset = ((available_height - MAIN_PANEL_MAX_HEIGHT) / 2.0).clamp(96.0, 320.0);
        let max_height = MAIN_PANEL_MAX_HEIGHT
            .min((available_height - top_offset - 24.0).max(MAIN_PANEL_MIN_HEIGHT));
        let height = requested_height.clamp(MAIN_PANEL_MIN_HEIGHT, max_height);
        // Preserve inner width. `set_size(LogicalSize)` sets the inner client area;
        // reading outer_size (shadow/DWM frame) and writing it back widens the window
        // once — visible as a width jitter when the panel opens or returns to search.
        let window_scale = window.scale_factor().unwrap_or(scale);
        let width = window
            .inner_size()
            .ok()
            .map(|size| size.width as f64 / window_scale)
            .unwrap_or(MAIN_PANEL_WIDTH)
            .clamp(320.0, MAIN_PANEL_MAX_WIDTH.max(MAIN_PANEL_WIDTH));
        (width, height)
    } else {
        (
            MAIN_PANEL_WIDTH,
            requested_height.clamp(MAIN_PANEL_MIN_HEIGHT, MAIN_PANEL_MAX_HEIGHT),
        )
    };
    // Skip no-op resizes (e.g. ResizeObserver remount on every open) to avoid a flash.
    if let (Ok(current), Ok(window_scale)) = (window.inner_size(), window.scale_factor()) {
        let current_width = current.width as f64 / window_scale;
        let current_height = current.height as f64 / window_scale;
        if (current_width - width).abs() < 0.5 && (current_height - height).abs() < 0.5 {
            return Ok(());
        }
    }
    window.set_size(LogicalSize::new(width, height))?;
    Ok(())
}

#[tauri::command]
pub fn show_main_panel_window(app: AppHandle) -> Result<(), String> {
    show_main_panel(&app).map_err(|error| error.to_string())
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

/// Re-apply the native theme and solid background after the frontend theme changes.
#[tauri::command]
pub fn sync_main_panel_appearance(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(MAIN_PANEL_LABEL) {
        polish_main_panel_window(&window);
    }
    Ok(())
}

/// Re-apply shelf vibrancy material after theme changes (HudWindow ↔ Popover).
#[tauri::command]
pub fn sync_shelf_picker_appearance(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window(SHELF_PICKER_LABEL) {
        apply_macos_shelf_vibrancy(&window);
    }
    let _ = &app;
    Ok(())
}

fn place_main_panel_window(
    app: &AppHandle,
    window: &WebviewWindow,
    requested_width: f64,
    requested_height: f64,
    follow_cursor: bool,
) -> tauri::Result<()> {
    let max_width = requested_width
        .max(MAIN_PANEL_WIDTH)
        .min(MAIN_PANEL_MAX_WIDTH.max(MAIN_PANEL_WIDTH));
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
        let height = requested_height.clamp(MAIN_PANEL_MIN_HEIGHT, MAIN_PANEL_MAX_HEIGHT);
        let width = requested_width.clamp(320.0, max_width);
        window.set_size(LogicalSize::new(width, height))?;
        if follow_cursor {
            return window.center();
        }
        return Ok(());
    };

    let scale = monitor.scale_factor();
    let work_area = monitor.work_area();
    let available_width = work_area.size.width as f64 / scale;
    let available_height = work_area.size.height as f64 / scale;
    let width = (available_width - 32.0).clamp(320.0, max_width.min(requested_width.max(320.0)));
    let top_offset = ((available_height - MAIN_PANEL_MAX_HEIGHT) / 2.0).clamp(96.0, 320.0);
    let max_height = MAIN_PANEL_MAX_HEIGHT
        .min((available_height - top_offset - 24.0).max(MAIN_PANEL_MIN_HEIGHT));
    let height = requested_height.clamp(MAIN_PANEL_MIN_HEIGHT, max_height);

    let physical_width = (width * scale).round() as i32;
    let physical_height = (height * scale).round() as u32;
    // Opening follows the cursor/monitor; later resizes keep the user's drag position.
    if follow_cursor {
        let x = work_area.position.x + (work_area.size.width as i32 - physical_width) / 2;
        let y = work_area.position.y + (top_offset * scale).round() as i32;
        window.set_position(PhysicalPosition::new(x, y))?;
    }
    window.set_size(PhysicalSize::new(
        physical_width.max(1) as u32,
        physical_height.max(1),
    ))
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
    // Apply NSPanel config first, then shelf-only vibrancy / level so main-panel chrome
    // cannot overwrite HudWindow + Status (25).
    show_shelf_window_without_stealing_focus(app, SHELF_PICKER_LABEL, &window)?;
    polish_shelf_picker_window(&window, true);
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
#[cfg(target_os = "macos")]
const MACOS_SHELF_CORNER_RADIUS: f64 = 16.0;

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
fn shelf_resolves_dark(window: &WebviewWindow) -> bool {
    let theme = window
        .app_handle()
        .try_state::<crate::db::AppState>()
        .map(|state| {
            let conn = state.db.lock();
            crate::db::get_setting(&conn, "theme", "system")
        })
        .unwrap_or_else(|| "system".into());
    match theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => crate::platform::system_prefers_dark(),
    }
}

#[cfg(target_os = "macos")]
fn apply_macos_shelf_vibrancy(window: &WebviewWindow) {
    use window_vibrancy::{
        apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial, NSVisualEffectState,
    };

    crate::logging::debug_if_err(clear_vibrancy(window), "clear shelf picker macos vibrancy");
    // Dark → HudWindow (white-on-glass CSS). Light → Popover so black type stays readable.
    let material = if shelf_resolves_dark(window) {
        NSVisualEffectMaterial::HudWindow
    } else {
        NSVisualEffectMaterial::Popover
    };
    crate::logging::debug_if_err(
        apply_vibrancy(
            window,
            material,
            Some(NSVisualEffectState::Active),
            Some(MACOS_SHELF_CORNER_RADIUS),
        ),
        "apply shelf picker macos vibrancy",
    );
}

#[cfg(target_os = "macos")]
fn polish_macos_shelf_picker_window(window: &WebviewWindow, topmost: bool) {
    // Resolve system → concrete theme before vibrancy so HudWindow/Popover matches.
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

pub fn polish_main_panel_window(window: &WebviewWindow) {
    sync_overlay_window_theme(window);

    #[cfg(target_os = "macos")]
    {
        clear_macos_main_panel_vibrancy(window);
        crate::macos_overlay_panel::apply_system_window_chrome(window);
    }

    let theme_setting = window
        .app_handle()
        .try_state::<crate::db::AppState>()
        .map(|state| {
            let conn = state.db.lock();
            crate::db::get_setting(&conn, "theme", "system")
        })
        .unwrap_or_else(|| "system".into());
    let background = match theme_setting.as_str() {
        "dark" => Color(18, 20, 24, 255),
        "light" => Color(247, 249, 248, 255),
        _ => {
            // Prefer OS preference over a possibly stale window.theme() cache.
            if crate::platform::system_prefers_dark() {
                Color(18, 20, 24, 255)
            } else {
                Color(247, 249, 248, 255)
            }
        }
    };
    crate::logging::debug_if_err(
        window.set_background_color(Some(background)),
        "set main panel opaque background",
    );
    crate::logging::debug_if_err(window.set_shadow(true), "set main panel shadow");

    #[cfg(target_os = "windows")]
    apply_windows_shelf_appearance(window);
}

/// Keep native window appearance in sync with Tempo's theme setting so the solid fill
/// matches the CSS light/dark tokens.
fn sync_overlay_window_theme(window: &WebviewWindow) {
    let theme = window
        .app_handle()
        .try_state::<crate::db::AppState>()
        .map(|state| {
            let conn = state.db.lock();
            crate::db::get_setting(&conn, "theme", "system")
        })
        .unwrap_or_else(|| "system".into());

    // Resolve "system" to a concrete theme. set_theme(None) is unreliable for
    // WKWebView / vibrancy on macOS Accessory (LSUIElement) apps.
    let native = match theme.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => Some(if crate::platform::system_prefers_dark() {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }),
    };
    crate::logging::debug_if_err(window.set_theme(native), "sync overlay window theme");
}

pub fn build_main_panel_window(
    app: &AppHandle,
    width: f64,
    height: f64,
) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(
        app,
        MAIN_PANEL_LABEL,
        WebviewUrl::App("/?view=main-panel".into()),
    )
    .title("主面板")
    .inner_size(width, height)
    .resizable(false)
    .decorations(false)
    .transparent(false)
    .background_color(Color(247, 249, 248, 255))
    .shadow(cfg!(any(target_os = "macos", target_os = "windows")))
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(false)
    .center()
    .build()
}

#[cfg(target_os = "macos")]
fn clear_macos_main_panel_vibrancy(window: &WebviewWindow) {
    use window_vibrancy::clear_vibrancy;

    crate::logging::debug_if_err(clear_vibrancy(window), "clear main panel vibrancy");
}
