#![allow(unexpected_cfgs)]

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{ActivationPolicy, AppHandle, Manager};

static MAIN_WINDOW_IN_TRAY: AtomicBool = AtomicBool::new(false);

/// Tempo is a menu-bar / tray app on macOS: never show a Dock icon.
/// Prefer setting Accessory on `App` before `run()` (see `lib.rs`) so the Dock never flashes;
/// this helper is for later runtime reinforcement.
pub fn ensure_accessory_policy(app: &AppHandle) {
    crate::logging::debug_if_err(
        app.set_activation_policy(ActivationPolicy::Accessory),
        "set macos accessory activation policy",
    );
    crate::logging::debug_if_err(app.set_dock_visibility(false), "hide macos dock icon");
}

pub fn hide_presence(app: &AppHandle) {
    MAIN_WINDOW_IN_TRAY.store(true, Ordering::SeqCst);
    ensure_accessory_policy(app);
    ensure_main_window_hidden(app);
}

pub fn is_main_window_in_tray() -> bool {
    MAIN_WINDOW_IN_TRAY.load(Ordering::SeqCst)
}

pub fn ensure_main_window_hidden(app: &AppHandle) {
    if !is_main_window_in_tray() {
        return;
    }

    hide_main_window(app);
}

fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        crate::logging::debug_if_err(window.hide(), "hide main window for macos tray");
    }
}

/// Show the main window without promoting the app to a Dock-visible Regular policy.
pub fn show_presence(app: &AppHandle) -> Result<(), String> {
    MAIN_WINDOW_IN_TRAY.store(false, Ordering::SeqCst);
    ensure_accessory_policy(app);
    Ok(())
}
