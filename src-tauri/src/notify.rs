//! Cross-platform user notifications.
//!
//! On macOS, `tauri-plugin-notification` is feature-unified onto
//! `notify-rust`'s `preview-macos-un` path (`UNUserNotificationCenter`).
//! That only works from a real signed `.app` bundle — use a packaged build to
//! verify authorization sheets and banners.

use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;

/// Show a desktop notification.
pub fn show_notification<R: Runtime>(
    app: &AppHandle<R>,
    title: &str,
    body: &str,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    ensure_macos_authorized(app)?;

    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn show_user_notification(app: AppHandle, title: String, body: String) -> Result<(), String> {
    show_notification(&app, &title, &body)
}

#[cfg(target_os = "macos")]
fn ensure_macos_authorized<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    use tauri::ActivationPolicy;

    let _ = app.set_activation_policy(ActivationPolicy::Regular);
    activate_app_for_permission_prompt();

    let result = request_authorization_on_main_thread(app);
    let _ = app.set_activation_policy(ActivationPolicy::Accessory);

    let granted = result?;
    if granted {
        Ok(())
    } else {
        Err("未授予通知权限，请在系统弹窗中选择允许".into())
    }
}

#[cfg(target_os = "macos")]
fn request_authorization_on_main_thread<R: Runtime>(app: &AppHandle<R>) -> Result<bool, String> {
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel::<Result<bool, String>>();
    app.run_on_main_thread(move || {
        let result = request_authorization_blocking();
        let _ = tx.send(result);
    })
    .map_err(|error| error.to_string())?;

    rx.recv()
        .map_err(|_| "等待通知授权结果超时".to_string())?
}

#[cfg(target_os = "macos")]
fn request_authorization_blocking() -> Result<bool, String> {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::NSError;
    use objc2_user_notifications::{UNAuthorizationOptions, UNUserNotificationCenter};

    let (tx, rx) = mpsc::channel::<Result<bool, String>>();
    let tx = std::cell::Cell::new(Some(tx));

    let center = UNUserNotificationCenter::currentNotificationCenter();
    let options = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound;
    let handler = RcBlock::new(move |granted: Bool, error: *mut NSError| {
        let Some(sender) = tx.take() else {
            return;
        };
        if !error.is_null() {
            // SAFETY: UN passes a valid NSError pointer when non-null.
            let err = unsafe { &*error };
            let message = err.localizedDescription().to_string();
            let _ = sender.send(Err(format!("系统拒绝通知授权: {message}")));
            return;
        }
        let _ = sender.send(Ok(granted.as_bool()));
    });

    center.requestAuthorizationWithOptions_completionHandler(options, &handler);

    rx.recv_timeout(Duration::from_secs(120))
        .map_err(|_| "等待系统通知授权弹窗超时".to_string())?
}

#[cfg(target_os = "macos")]
fn activate_app_for_permission_prompt() {
    use objc::{class, msg_send, runtime::Object, sel, sel_impl};

    // SAFETY: NSApplication APIs; Tempo already links AppKit via Tauri.
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, activateIgnoringOtherApps: true];
        }
    }
}
