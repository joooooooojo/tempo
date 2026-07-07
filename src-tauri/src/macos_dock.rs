#![allow(unexpected_cfgs)]

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::{ActivationPolicy, AppHandle, Manager};

static MAIN_WINDOW_IN_TRAY: AtomicBool = AtomicBool::new(false);
static CACHED_ICON_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn hide_presence(app: &AppHandle) {
    if MAIN_WINDOW_IN_TRAY.swap(true, Ordering::SeqCst) {
        return;
    }

    apply_branding(app);
    let _ = app.set_activation_policy(ActivationPolicy::Accessory);
    hide_application_windows();
}

pub fn show_presence(app: &AppHandle) -> Result<(), String> {
    MAIN_WINDOW_IN_TRAY.store(false, Ordering::SeqCst);

    apply_branding(app);
    app.set_activation_policy(ActivationPolicy::Regular)
        .map_err(|e| e.to_string())?;
    apply_branding(app);
    refresh_dock_tile();

    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        apply_branding(&app_handle);
        refresh_dock_tile();
    });

    Ok(())
}

pub fn apply_branding(app: &AppHandle) {
    let Some(path) = resolve_icon_path(app) else {
        return;
    };
    let _ = CACHED_ICON_PATH.set(path.clone());
    set_application_icon(&path);
}

fn hide_application_windows() {
    unsafe {
        let ns_app: *mut objc::runtime::Object = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![ns_app, hide: std::ptr::null_mut::<objc::runtime::Object>()];
    }
}

fn refresh_dock_tile() {
    unsafe {
        let ns_app: *mut objc::runtime::Object = msg_send![class!(NSApplication), sharedApplication];
        if ns_app.is_null() {
            return;
        }

        let dock_tile: *mut objc::runtime::Object = msg_send![ns_app, dockTile];
        if dock_tile.is_null() {
            return;
        }

        let _: () = msg_send![dock_tile, display];
    }
}

fn resolve_icon_path(app: &AppHandle) -> Option<PathBuf> {
    if let Some(path) = CACHED_ICON_PATH.get() {
        if path.exists() {
            return Some(path.clone());
        }
    }

    let manifest_icon = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons/icon.icns");
    if manifest_icon.exists() {
        return Some(manifest_icon);
    }

    app.path()
        .resource_dir()
        .ok()
        .map(|dir| dir.join("icons/icon.icns"))
        .filter(|path| path.exists())
}

fn set_application_icon(path: &Path) {
    let Ok(path_cstr) = CString::new(path.to_string_lossy().as_ref()) else {
        return;
    };

    unsafe {
        let ns_string: *mut objc::runtime::Object =
            msg_send![class!(NSString), stringWithUTF8String: path_cstr.as_ptr()];
        if ns_string.is_null() {
            return;
        }

        let ns_image: *mut objc::runtime::Object = msg_send![class!(NSImage), alloc];
        let ns_image: *mut objc::runtime::Object =
            msg_send![ns_image, initWithContentsOfFile: ns_string];
        if ns_image.is_null() {
            return;
        }

        let ns_app: *mut objc::runtime::Object = msg_send![class!(NSApplication), sharedApplication];
        if ns_app.is_null() {
            return;
        }

        let _: () = msg_send![ns_app, setApplicationIconImage: ns_image];
        refresh_dock_tile();
    }
}
