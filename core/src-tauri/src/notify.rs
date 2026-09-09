//! Cross-platform user notifications.
//!
//! On macOS, `tauri-plugin-notification` is feature-unified onto
//! `notify-rust`'s `preview-macos-un` path (`UNUserNotificationCenter`).
//! That requires a real Apple-signed `.app` (Development or Developer ID) —
//! ad-hoc signatures are rejected with UNErrorDomain code 1.

use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;

/// Show a desktop notification.
///
/// macOS: never presents the system authorization sheet here — that steals key
/// focus and dismisses the main panel. Call [`prime_macos_authorization`] once
/// at startup instead; this path only checks the already-resolved status.
pub fn show_notification<R: Runtime>(
    app: &AppHandle<R>,
    title: &str,
    body: &str,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    ensure_macos_already_authorized(app)?;

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

/// Request notification permission once at launch (before the main panel is key).
///
/// Safe to call when no overlay is focused so the system sheet cannot trigger
/// blur→hide. No-op when already decided / when the binary is not Apple-signed.
#[cfg(target_os = "macos")]
pub fn prime_macos_authorization<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = run_on_main_thread(app, || {
        match notification_authorization_status()? {
            AuthStatus::Allowed => Ok(()),
            AuthStatus::Denied => Ok(()),
            AuthStatus::NotDetermined => {
                let _ = request_authorization_with_runloop();
                Ok(())
            }
        }
    }) {
        tracing::debug!(error = %error, "macos notification prime skipped");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn prime_macos_authorization<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(target_os = "macos")]
fn ensure_macos_already_authorized<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    match run_on_main_thread(app, notification_authorization_status)? {
        AuthStatus::Allowed => Ok(()),
        AuthStatus::Denied => Err(
            "系统通知权限已被关闭。请在「系统设置 → 通知 → Tempo」中允许通知后重试".into(),
        ),
        AuthStatus::NotDetermined => Err(
            "尚未授予通知权限。请重启 Tempo 并在系统弹窗中选择允许，或到「系统设置 → 通知 → Tempo」开启"
                .into(),
        ),
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthStatus {
    Allowed,
    Denied,
    NotDetermined,
}

#[cfg(target_os = "macos")]
fn run_on_main_thread<R: Runtime, T: Send + 'static>(
    app: &AppHandle<R>,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel::<Result<T, String>>();
    app.run_on_main_thread(move || {
        let _ = tx.send(work());
    })
    .map_err(|error| error.to_string())?;

    rx.recv()
        .map_err(|_| "等待主线程执行通知授权超时".to_string())?
}

#[cfg(target_os = "macos")]
fn notification_authorization_status() -> Result<AuthStatus, String> {
    use std::ptr::NonNull;
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2_user_notifications::{
        UNAuthorizationStatus, UNNotificationSettings, UNUserNotificationCenter,
    };

    let (tx, rx) = mpsc::channel::<AuthStatus>();
    let tx = std::cell::Cell::new(Some(tx));

    let center = UNUserNotificationCenter::currentNotificationCenter();
    let handler = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        let Some(sender) = tx.take() else {
            return;
        };
        // SAFETY: UN retains the settings for the duration of the handler.
        let settings = unsafe { settings.as_ref() };
        let status = match settings.authorizationStatus() {
            UNAuthorizationStatus::Authorized
            | UNAuthorizationStatus::Provisional
            | UNAuthorizationStatus::Ephemeral => AuthStatus::Allowed,
            UNAuthorizationStatus::Denied => AuthStatus::Denied,
            _ => AuthStatus::NotDetermined,
        };
        let _ = sender.send(status);
    });

    center.getNotificationSettingsWithCompletionHandler(&handler);
    wait_for_callback(&rx, Duration::from_secs(5))
        .map_err(|_| "读取通知授权状态超时".to_string())
}

#[cfg(target_os = "macos")]
fn request_authorization_with_runloop() -> Result<bool, String> {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::NSError;
    use objc2_user_notifications::{UNAuthorizationOptions, UNUserNotificationCenter};

    // Stay Accessory / LSUIElement — do not flip ActivationPolicy or call
    // activateIgnoringOtherApps. Those flash the Dock and dismiss the main panel.
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
            let _ = sender.send(Err(format!(
                "系统拒绝通知授权: {message}。请确认 Tempo 使用 Apple 开发者证书签名（adhoc 签名无法使用通知），并在「系统设置 → 通知 → Tempo」中允许通知"
            )));
            return;
        }
        let _ = sender.send(Ok(granted.as_bool()));
    });

    center.requestAuthorizationWithOptions_completionHandler(options, &handler);
    wait_for_callback(&rx, Duration::from_secs(120))
        .map_err(|_| "等待系统通知授权弹窗超时".to_string())?
}

/// Pump the main CFRunLoop while waiting so authorization sheets can present.
#[cfg(target_os = "macos")]
fn wait_for_callback<T>(
    rx: &std::sync::mpsc::Receiver<T>,
    timeout: std::time::Duration,
) -> Result<T, ()> {
    use std::time::Instant;

    use core_foundation_sys::runloop::{kCFRunLoopDefaultMode, CFRunLoopRunInMode};

    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(value) = rx.try_recv() {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(());
        }
        // SAFETY: pumping the default mode is required while blocked on main.
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, 0);
        }
    }
}
