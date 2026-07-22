//! Non-activating NSPanel overlays: show/focus overlays without activating Tempo
//! or touching the main window.

use block::ConcreteBlock;
use objc::runtime::{Class, Object};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_nspanel::{
    tauri_panel, CollectionBehavior, ManagerExt, Panel, PanelLevel, StyleMask, WebviewWindowExt,
};

tauri_panel! {
    panel!(OverlayInputPanel {
        config: {
            can_become_key_window: true,
            can_become_main_window: false,
        }
    })
}

#[derive(Clone, Copy)]
pub struct OverlayPanelConfig {
    pub level: PanelLevel,
    pub collection_behavior: CollectionBehavior,
    pub has_shadow: bool,
    pub becomes_key_only_if_needed: bool,
}

impl OverlayPanelConfig {
    fn apply_input(&self, panel: &dyn Panel) {
        apply_base(panel, true, self);
    }
}

fn overlay_style_mask() -> StyleMask {
    StyleMask::empty().borderless().nonactivating_panel()
}

fn apply_base(panel: &dyn Panel, input: bool, config: &OverlayPanelConfig) {
    panel.set_style_mask(overlay_style_mask().into());
    panel.set_floating_panel(true);
    panel.set_becomes_key_only_if_needed(if input {
        config.becomes_key_only_if_needed
    } else {
        false
    });
    panel.set_hides_on_deactivate(false);
    panel.set_level(config.level.value());
    panel.set_collection_behavior(config.collection_behavior.into());
    panel.set_transparent(true);
    panel.set_has_shadow(config.has_shadow);
    panel.set_opaque(false);
}

pub fn shelf_picker_config() -> OverlayPanelConfig {
    // Match ZTools: Electron `setAlwaysOnTop(true, 'modal-panel')` → NSModalPanelWindowLevel (8).
    // Status (25) sits above system file sheets and makes Import Directory / zip look like a no-op.
    OverlayPanelConfig {
        level: PanelLevel::ModalPanel,
        collection_behavior: CollectionBehavior::new()
            .can_join_all_spaces()
            .stationary()
            .full_screen_auxiliary()
            .full_screen_none(),
        has_shadow: true,
        // false: command palette must become key on show so the search input can autofocus.
        // With true, AppKit ignores makeKeyWindow unless an NSTextField asks — WKWebView does not.
        becomes_key_only_if_needed: false,
    }
}

pub fn ensure_input_panel(
    app: &AppHandle,
    window: &WebviewWindow,
    label: &str,
    config: &OverlayPanelConfig,
) -> tauri::Result<()> {
    if let Ok(panel) = app.get_webview_panel(label) {
        // Re-apply on every ensure so level/style changes take effect without recreating the panel.
        config.apply_input(panel.as_ref());
        return Ok(());
    }

    let panel = window.to_panel::<OverlayInputPanel>()?;
    config.apply_input(panel.as_ref());
    Ok(())
}

pub fn show_input_overlay(app: &AppHandle, label: &str) -> tauri::Result<()> {
    let panel = app
        .get_webview_panel(label)
        .map_err(|_| tauri::Error::WindowNotFound)?;
    panel.show_and_make_key();
    Ok(())
}

pub fn hide_overlay(app: &AppHandle, label: &str) {
    if let Ok(panel) = app.get_webview_panel(label) {
        panel.hide();
        return;
    }

    if let Some(window) = app.get_webview_window(label) {
        crate::logging::debug_if_err(window.hide(), "hide macos overlay fallback window");
    }
}

/// Temporarily make overlay panels safe hosts for NSOpenPanel sheets (ZTools parents dialogs
/// to the palette). Nonactivating + high levels can swallow or hide the picker.
pub fn prepare_for_native_dialog(app: &AppHandle) {
    for label in ["command-palette", "shelf-picker"] {
        if let Ok(panel) = app.get_webview_panel(label) {
            // Keep borderless, but drop NonactivatingPanel so the sheet can key/focus.
            panel.set_style_mask(StyleMask::empty().borderless().into());
            panel.set_level(PanelLevel::ModalPanel.value());
            panel.set_becomes_key_only_if_needed(false);
            panel.show_and_make_key();
        }
    }
}

/// Restore overlay panels to the ZTools-aligned ModalPanel + nonactivating style.
pub fn restore_after_native_dialog(app: &AppHandle) {
    let config = shelf_picker_config();
    for label in ["command-palette", "shelf-picker"] {
        if let Ok(panel) = app.get_webview_panel(label) {
            config.apply_input(panel.as_ref());
        }
    }
}

#[derive(Clone, Copy)]
struct EventMonitorTokens {
    local: usize,
    global: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CocoaPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CocoaSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CocoaRect {
    origin: CocoaPoint,
    size: CocoaSize,
}

type NSEventMask = usize;

const NSEVENT_TYPE_LEFT_MOUSE_DOWN: NSEventMask = 1;
const NSEVENT_TYPE_RIGHT_MOUSE_DOWN: NSEventMask = 3;
const NSEVENT_TYPE_OTHER_MOUSE_DOWN: NSEventMask = 25;
const SHELF_OUTSIDE_CLICK_EVENT_MASK: NSEventMask = (1 << NSEVENT_TYPE_LEFT_MOUSE_DOWN)
    | (1 << NSEVENT_TYPE_RIGHT_MOUSE_DOWN)
    | (1 << NSEVENT_TYPE_OTHER_MOUSE_DOWN);

static SHELF_OUTSIDE_CLICK_MONITOR_GENERATION: AtomicU64 = AtomicU64::new(0);

fn shelf_outside_click_monitors() -> &'static Mutex<Option<EventMonitorTokens>> {
    static MONITORS: OnceLock<Mutex<Option<EventMonitorTokens>>> = OnceLock::new();
    MONITORS.get_or_init(|| Mutex::new(None))
}

/// Installs AppKit event monitors that close the shelf when the next mouse
/// down happens outside the shelf panel.
///
/// AppKit splits this into two official monitors: local monitors see events
/// dispatched inside Tempo, while global monitors see mouse events dispatched
/// to other apps/the desktop.  Keeping both lets the shelf remain a single
/// NSPanel instead of using a transparent backdrop window as a click target.
pub fn install_shelf_outside_click_monitor(app: &AppHandle, label: &'static str) {
    remove_shelf_outside_click_monitor();

    let generation = SHELF_OUTSIDE_CLICK_MONITOR_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let app_for_local = app.clone();
    let app_for_global = app.clone();

    let Some(event_class) = Class::get("NSEvent") else {
        return;
    };

    let local_block = ConcreteBlock::new(move |event: *mut Object| -> *mut Object {
        if should_close_shelf_for_current_mouse_location(&app_for_local, label, generation) {
            crate::logging::debug_if_err(
                crate::auxiliary_windows::hide_shelf_picker_window(&app_for_local),
                "hide shelf picker from local macos monitor",
            );
        }
        event
    })
    .copy();

    let global_block = ConcreteBlock::new(move |_event: *mut Object| {
        if should_close_shelf_for_current_mouse_location(&app_for_global, label, generation) {
            crate::logging::debug_if_err(
                crate::auxiliary_windows::hide_shelf_picker_window(&app_for_global),
                "hide shelf picker from global macos monitor",
            );
        }
    })
    .copy();

    let local_monitor = unsafe {
        use objc::{msg_send, sel, sel_impl};

        let monitor: *mut Object = msg_send![
            event_class,
            addLocalMonitorForEventsMatchingMask: SHELF_OUTSIDE_CLICK_EVENT_MASK
            handler: &*local_block
        ];
        retain_event_monitor(monitor)
    };

    let global_monitor = unsafe {
        use objc::{msg_send, sel, sel_impl};

        let monitor: *mut Object = msg_send![
            event_class,
            addGlobalMonitorForEventsMatchingMask: SHELF_OUTSIDE_CLICK_EVENT_MASK
            handler: &*global_block
        ];
        retain_event_monitor(monitor)
    };

    if local_monitor == 0 && global_monitor == 0 {
        return;
    }

    if let Ok(mut monitors) = shelf_outside_click_monitors().lock() {
        *monitors = Some(EventMonitorTokens {
            local: local_monitor,
            global: global_monitor,
        });
    } else {
        unsafe {
            remove_event_monitor(local_monitor);
            remove_event_monitor(global_monitor);
        }
    }
}

pub fn remove_shelf_outside_click_monitor() {
    SHELF_OUTSIDE_CLICK_MONITOR_GENERATION.fetch_add(1, Ordering::Relaxed);

    let tokens = match shelf_outside_click_monitors().lock() {
        Ok(mut monitors) => monitors.take(),
        Err(error) => {
            tracing::debug!(
                error = %error,
                "failed to lock shelf outside click monitor tokens for removal"
            );
            None
        }
    };

    if let Some(tokens) = tokens {
        unsafe {
            remove_event_monitor(tokens.local);
            remove_event_monitor(tokens.global);
        }
    }
}

fn should_close_shelf_for_current_mouse_location(
    app: &AppHandle,
    label: &str,
    generation: u64,
) -> bool {
    if SHELF_OUTSIDE_CLICK_MONITOR_GENERATION.load(Ordering::Relaxed) != generation {
        return false;
    }

    let visible = app
        .get_webview_window(label)
        .map(|window| match window.is_visible() {
            Ok(visible) => visible,
            Err(error) => {
                tracing::debug!(error = %error, "failed to read macos shelf visibility");
                false
            }
        })
        .unwrap_or(false);
    if !visible {
        return false;
    }

    !current_mouse_location_is_inside_window(app, label)
}

fn current_mouse_location_is_inside_window(app: &AppHandle, label: &str) -> bool {
    let Some(window) = app.get_webview_window(label) else {
        return false;
    };

    let inside = Arc::new(AtomicBool::new(false));
    let inside_for_webview = Arc::clone(&inside);

    crate::logging::debug_if_err(
        window.with_webview(move |webview| unsafe {
            use objc::{msg_send, sel, sel_impl};

            let ns_window = webview.ns_window().cast::<Object>();
            if ns_window.is_null() {
                return;
            }

            let Some(event_class) = Class::get("NSEvent") else {
                return;
            };

            let frame: CocoaRect = msg_send![ns_window, frame];
            let point: CocoaPoint = msg_send![event_class, mouseLocation];
            inside_for_webview.store(cocoa_point_in_rect(point, frame), Ordering::Relaxed);
        }),
        "check macos shelf mouse location",
    );

    inside.load(Ordering::Relaxed)
}

fn cocoa_point_in_rect(point: CocoaPoint, rect: CocoaRect) -> bool {
    let min_x = rect.origin.x;
    let max_x = rect.origin.x + rect.size.width;
    let min_y = rect.origin.y;
    let max_y = rect.origin.y + rect.size.height;

    point.x >= min_x && point.x <= max_x && point.y >= min_y && point.y <= max_y
}

unsafe fn remove_event_monitor(monitor: usize) {
    if monitor == 0 {
        return;
    }

    let Some(event_class) = Class::get("NSEvent") else {
        return;
    };

    use objc::{msg_send, sel, sel_impl};

    let monitor = monitor as *mut Object;
    let _: () = msg_send![event_class, removeMonitor: monitor];
    let _: () = msg_send![monitor, release];
}

unsafe fn retain_event_monitor(monitor: *mut Object) -> usize {
    if !monitor.is_null() {
        use objc::{msg_send, sel, sel_impl};

        let _: *mut Object = msg_send![monitor, retain];
    }
    monitor as usize
}
