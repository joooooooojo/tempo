#![allow(unexpected_cfgs)]

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{ActivationPolicy, AppHandle, Manager};

static MAIN_WINDOW_IN_TRAY: AtomicBool = AtomicBool::new(false);

pub fn hide_presence(app: &AppHandle) {
    if MAIN_WINDOW_IN_TRAY.swap(true, Ordering::SeqCst) {
        ensure_main_window_hidden(app);
        return;
    }

    let _ = app.set_activation_policy(ActivationPolicy::Accessory);
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
        let _ = window.hide();
    }
}

pub fn show_presence(app: &AppHandle) -> Result<(), String> {
    MAIN_WINDOW_IN_TRAY.store(false, Ordering::SeqCst);

    app.set_activation_policy(ActivationPolicy::Regular)
        .map_err(|e| e.to_string())
}
