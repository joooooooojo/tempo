//! macOS: dismiss bottom shelf pickers on outside click.
//!
//! `onFocusChanged` / `WindowEvent::Focused(false)` are unreliable for
//! always-on-top auxiliary windows. Use NSEvent global + local monitors
//! instead (same approach as tray/HUD apps).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use block::ConcreteBlock;
use objc::runtime::{Class, Object};
use objc::{msg_send, sel, sel_impl};
use tauri::{AppHandle, Manager};

use crate::auxiliary_windows::{CLIPBOARD_PICKER_LABEL, SNIPPET_PICKER_LABEL};

const MOUSE_DOWN_MASK: u64 = (1 << 1) | (1 << 3) | (1 << 5);
const GLOBAL_DISMISS_SUPPRESS_MS: u64 = 200;

static LAST_SHELF_SHOW_MS: AtomicU64 = AtomicU64::new(0);

pub fn install_shelf_dismiss_monitors(app: &AppHandle) {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return;
    }

    let app_global = app.clone();
    let global_block = ConcreteBlock::new(move |_event: *mut Object| {
        if should_suppress_global_dismiss() {
            return;
        }
        hide_visible_shelf_pickers(app_global.clone());
    });
    let global_block = global_block.copy();

    let app_local = app.clone();
    let local_block = ConcreteBlock::new(move |event: *mut Object| -> *mut Object {
        unsafe {
            dismiss_shelf_pickers_on_local_click(&app_local, event);
        }
        event
    });
    let local_block = local_block.copy();

    unsafe {
        let Some(cls) = Class::get("NSEvent") else {
            return;
        };
        let _: *mut Object = msg_send![
            cls,
            addGlobalMonitorForEventsMatchingMask: MOUSE_DOWN_MASK
            handler: &*global_block
        ];
        let _: *mut Object = msg_send![
            cls,
            addLocalMonitorForEventsMatchingMask: MOUSE_DOWN_MASK
            handler: &*local_block
        ];
    }

    // Monitors borrow the blocks for the process lifetime.
    std::mem::forget(global_block);
    std::mem::forget(local_block);
}

pub fn note_shelf_shown() {
    LAST_SHELF_SHOW_MS.store(now_ms(), Ordering::Relaxed);
}

fn should_suppress_global_dismiss() -> bool {
    let last = LAST_SHELF_SHOW_MS.load(Ordering::Relaxed);
    last != 0 && now_ms().saturating_sub(last) < GLOBAL_DISMISS_SUPPRESS_MS
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn hide_visible_shelf_pickers(app: AppHandle) {
    for label in [CLIPBOARD_PICKER_LABEL, SNIPPET_PICKER_LABEL] {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        }
    }
}

unsafe fn dismiss_shelf_pickers_on_local_click(app: &AppHandle, event: *mut Object) {
    let click_window: *mut Object = msg_send![event, window];
    if click_window.is_null() {
        hide_visible_shelf_pickers(app.clone());
        return;
    }

    let click_ptr = click_window as *mut std::ffi::c_void;
    for label in [CLIPBOARD_PICKER_LABEL, SNIPPET_PICKER_LABEL] {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        if !window.is_visible().unwrap_or(false) {
            continue;
        }
        let Ok(picker_ns) = window.ns_window() else {
            continue;
        };
        if click_ptr != picker_ns {
            let _ = window.hide();
        }
    }
}
