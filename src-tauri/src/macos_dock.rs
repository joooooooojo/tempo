#![allow(unexpected_cfgs)]

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{ActivationPolicy, AppHandle};

static MAIN_WINDOW_IN_TRAY: AtomicBool = AtomicBool::new(false);

pub fn hide_presence(app: &AppHandle) {
    if MAIN_WINDOW_IN_TRAY.swap(true, Ordering::SeqCst) {
        return;
    }

    let _ = app.set_activation_policy(ActivationPolicy::Accessory);
    hide_application_windows();
}

pub fn show_presence(app: &AppHandle) -> Result<(), String> {
    MAIN_WINDOW_IN_TRAY.store(false, Ordering::SeqCst);

    app.set_activation_policy(ActivationPolicy::Regular)
        .map_err(|e| e.to_string())
}

fn hide_application_windows() {
    unsafe {
        let ns_app: *mut objc::runtime::Object =
            msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![ns_app, hide: std::ptr::null_mut::<objc::runtime::Object>()];
    }
}
