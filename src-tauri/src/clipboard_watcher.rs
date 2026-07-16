use crate::clipboard_db::{
    encode_rgba_png, get_clipboard_entry, hash_bytes, insert_clipboard_image,
    insert_clipboard_text, touch_clipboard_entry,
};
use crate::clipboard_images::save_clipboard_image_png;
use crate::clipboard_images::{
    load_clipboard_image_rgba_timed, normalize_clipboard_image_reference,
};
use crate::db::CachedClipboardImage;
use crate::db::{load_settings, AppState};
use crate::platform::{get_foreground_app, ForegroundApp};
use arboard::ImageData;
use arboard::{Clipboard, Error as ClipboardError};
use std::borrow::Cow;
use std::collections::HashSet;
#[cfg(target_os = "windows")]
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(not(target_os = "windows"))]
const CLIPBOARD_POLL_MS: u64 = 500;
const DECODED_IMAGE_CACHE_MAX_ENTRIES: usize = 16;
const DECODED_IMAGE_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;
#[cfg(target_os = "windows")]
const WINDOWS_CLIPBOARD_SETTLE_MS: u64 = 650;
#[cfg(target_os = "windows")]
const WINDOWS_CLIPBOARD_BUSY_BACKOFF_MS: u64 = 400;

enum ClipboardCaptureResult {
    Captured { changed: bool },
    Busy,
}

enum ClipboardSnapshot {
    Image(ImageData<'static>),
    Text(String),
    Empty,
}

static CLIPBOARD_ACCESS: OnceLock<std::sync::Mutex<()>> = OnceLock::new();

fn clipboard_access_guard() -> MutexGuard<'static, ()> {
    CLIPBOARD_ACCESS
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn start_clipboard_watcher(app: AppHandle, state: AppState) {
    crate::logging::spawn_named("tempo-clipboard-watcher", move || {
        let mut last_text_hash = String::new();
        let mut last_image_hash = String::new();
        let mut last_retention_purge = std::time::Instant::now();

        #[cfg(target_os = "windows")]
        let clipboard_update_rx = start_windows_clipboard_listener();
        #[cfg(target_os = "windows")]
        let mut last_windows_clipboard_sequence = 0;

        #[cfg(target_os = "macos")]
        let mut last_pasteboard_change_count = None;

        loop {
            let should_capture = wait_for_clipboard_change(
                #[cfg(target_os = "windows")]
                &clipboard_update_rx,
                #[cfg(target_os = "windows")]
                &last_windows_clipboard_sequence,
                #[cfg(target_os = "macos")]
                &mut last_pasteboard_change_count,
            );

            let settings = {
                let conn = state.db.lock();
                load_settings(&conn)
            };

            if last_retention_purge.elapsed() >= std::time::Duration::from_secs(3600) {
                last_retention_purge = std::time::Instant::now();
                let retention = settings.clipboard_history_retention.clone();
                let conn = state.db.lock();
                crate::clipboard_db::purge_clipboard_history_by_retention(&conn, &retention);
            }

            if let Some(app_info) = get_foreground_app() {
                let mut runtime = state.clipboard.lock();
                runtime.last_source_app = Some(app_info.name.clone());
                runtime.last_source_process = Some(app_info.process_name.clone());
            }

            if !settings.clipboard_monitor_enabled {
                #[cfg(target_os = "windows")]
                sync_windows_clipboard_sequence(&mut last_windows_clipboard_sequence);
                continue;
            }

            if !should_capture {
                continue;
            }

            {
                let runtime = state.clipboard.lock();
                if runtime.skip_next_capture {
                    #[cfg(target_os = "windows")]
                    sync_windows_clipboard_sequence(&mut last_windows_clipboard_sequence);
                    continue;
                }
            }

            let (source_app, source_process) = {
                let runtime = state.clipboard.lock();
                (
                    runtime.last_source_app.clone(),
                    runtime.last_source_process.clone(),
                )
            };
            let (source_app, source_process) = resolve_clipboard_source(
                get_foreground_app(),
                source_app.as_deref(),
                source_process.as_deref(),
            );

            let max_entries = settings.clipboard_max_entries.max(1).min(1000);
            let changed = match capture_clipboard_snapshot(
                &app,
                &state,
                source_app.as_deref(),
                source_process.as_deref(),
                max_entries,
                &mut last_text_hash,
                &mut last_image_hash,
            ) {
                ClipboardCaptureResult::Captured { changed } => changed,
                ClipboardCaptureResult::Busy => {
                    // Yield to Windows Clipboard History (Win+V), then retry once.
                    #[cfg(target_os = "windows")]
                    {
                        std::thread::sleep(Duration::from_millis(
                            WINDOWS_CLIPBOARD_BUSY_BACKOFF_MS,
                        ));
                        match capture_clipboard_snapshot(
                            &app,
                            &state,
                            source_app.as_deref(),
                            source_process.as_deref(),
                            max_entries,
                            &mut last_text_hash,
                            &mut last_image_hash,
                        ) {
                            ClipboardCaptureResult::Captured { changed } => changed,
                            ClipboardCaptureResult::Busy => {
                                sync_windows_clipboard_sequence(
                                    &mut last_windows_clipboard_sequence,
                                );
                                continue;
                            }
                        }
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        continue;
                    }
                }
            };
            #[cfg(target_os = "windows")]
            sync_windows_clipboard_sequence(&mut last_windows_clipboard_sequence);

            if changed {
                emit_clipboard_update(&app);
            }
        }
    });
}

fn wait_for_clipboard_change(
    #[cfg(target_os = "windows")] clipboard_update_rx: &Receiver<()>,
    #[cfg(target_os = "windows")] last_windows_clipboard_sequence: &u32,
    #[cfg(target_os = "macos")] last_pasteboard_change_count: &mut Option<isize>,
) -> bool {
    #[cfg(target_os = "windows")]
    {
        return wait_for_windows_clipboard_change(
            clipboard_update_rx,
            *last_windows_clipboard_sequence,
        );
    }

    #[cfg(target_os = "macos")]
    {
        std::thread::sleep(Duration::from_millis(CLIPBOARD_POLL_MS));
        return macos_pasteboard_changed(last_pasteboard_change_count);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::thread::sleep(Duration::from_millis(CLIPBOARD_POLL_MS));
        true
    }
}

fn capture_clipboard_snapshot(
    app: &AppHandle,
    state: &AppState,
    source_app: Option<&str>,
    source_process: Option<&str>,
    max_entries: u32,
    last_text_hash: &mut String,
    last_image_hash: &mut String,
) -> ClipboardCaptureResult {
    match read_clipboard_snapshot() {
        Ok(ClipboardSnapshot::Image(image)) => {
            let width = image.width as u32;
            let height = image.height as u32;
            let pixel_count = width as u64 * height as u64;
            if pixel_count == 0 || pixel_count > crate::clipboard_db::MAX_CLIPBOARD_IMAGE_PIXELS {
                return ClipboardCaptureResult::Captured { changed: false };
            }

            let Some(png_bytes) = encode_rgba_png(width, height, image.bytes.as_ref()) else {
                return ClipboardCaptureResult::Captured { changed: false };
            };
            if png_bytes.len() > crate::clipboard_db::MAX_CLIPBOARD_IMAGE_BYTES {
                return ClipboardCaptureResult::Captured { changed: false };
            }

            let content_hash = hash_bytes(&png_bytes);
            if content_hash == *last_image_hash {
                return ClipboardCaptureResult::Captured { changed: false };
            }

            last_image_hash.clear();
            last_image_hash.push_str(&content_hash);
            last_text_hash.clear();

            let inserted =
                if let Ok(storage_key) = save_clipboard_image_png(app, &content_hash, &png_bytes) {
                    cache_decoded_clipboard_image(
                        state,
                        &storage_key,
                        width,
                        height,
                        image.bytes.as_ref().to_vec(),
                    );
                    let conn = state.db.lock();
                    insert_clipboard_image(
                        &conn,
                        &storage_key,
                        &content_hash,
                        width,
                        height,
                        source_app,
                        source_process,
                        max_entries,
                    )
                } else {
                    tracing::warn!(
                        image_width = width,
                        image_height = height,
                        png_bytes = png_bytes.len(),
                        "failed to persist clipboard image"
                    );
                    None
                };

            return ClipboardCaptureResult::Captured {
                changed: inserted.is_some(),
            };
        }
        Ok(ClipboardSnapshot::Text(text)) => {
            if text.is_empty() {
                return ClipboardCaptureResult::Captured { changed: false };
            }

            let hash = crate::clipboard_db::hash_content(&text);
            if hash == *last_text_hash {
                return ClipboardCaptureResult::Captured { changed: false };
            }

            last_text_hash.clear();
            last_text_hash.push_str(&hash);
            last_image_hash.clear();

            let inserted = {
                let conn = state.db.lock();
                insert_clipboard_text(&conn, &text, source_app, source_process, max_entries)
            };
            ClipboardCaptureResult::Captured {
                changed: inserted.is_some(),
            }
        }
        Ok(ClipboardSnapshot::Empty) => ClipboardCaptureResult::Captured { changed: false },
        Err(error) if clipboard_error_is_busy(&error) => ClipboardCaptureResult::Busy,
        Err(error) => {
            tracing::debug!(error = %error, "failed to read clipboard snapshot");
            ClipboardCaptureResult::Captured { changed: false }
        }
    }
}

fn read_clipboard_snapshot() -> Result<ClipboardSnapshot, ClipboardError> {
    let _guard = clipboard_access_guard();
    let mut clipboard = Clipboard::new()?;

    #[cfg(target_os = "windows")]
    {
        return match windows_preferred_clipboard_format() {
            Some(WindowsClipboardFormat::Image) => {
                clipboard.get_image().map(ClipboardSnapshot::Image)
            }
            Some(WindowsClipboardFormat::Text) => clipboard.get_text().map(ClipboardSnapshot::Text),
            None => Ok(ClipboardSnapshot::Empty),
        };
    }

    #[cfg(not(target_os = "windows"))]
    {
        match clipboard.get_image() {
            Ok(image) => Ok(ClipboardSnapshot::Image(image)),
            Err(error) if clipboard_error_is_busy(&error) => Err(error),
            Err(image_error) => match clipboard.get_text() {
                Ok(text) => Ok(ClipboardSnapshot::Text(text)),
                Err(error) if clipboard_error_is_busy(&error) => Err(error),
                Err(text_error) => {
                    tracing::debug!(
                        image_error = %image_error,
                        text_error = %text_error,
                        "clipboard snapshot did not contain readable image or text"
                    );
                    Ok(ClipboardSnapshot::Empty)
                }
            },
        }
    }
}

fn clipboard_error_is_busy(error: &ClipboardError) -> bool {
    matches!(error, ClipboardError::ClipboardOccupied)
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
enum WindowsClipboardFormat {
    Image,
    Text,
}

#[cfg(target_os = "windows")]
fn windows_preferred_clipboard_format() -> Option<WindowsClipboardFormat> {
    use windows::Win32::System::DataExchange::{
        IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };

    const CF_UNICODETEXT: u32 = 13;
    const CF_DIBV5: u32 = 17;

    let png_name = windows_wide("PNG");
    let png_format = unsafe { RegisterClipboardFormatW(windows::core::PCWSTR(png_name.as_ptr())) };
    let has_png = png_format != 0 && unsafe { IsClipboardFormatAvailable(png_format).is_ok() };
    let has_dib = unsafe { IsClipboardFormatAvailable(CF_DIBV5).is_ok() };
    if has_png || has_dib {
        return Some(WindowsClipboardFormat::Image);
    }

    let has_text = unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() };
    has_text.then_some(WindowsClipboardFormat::Text)
}

#[cfg(target_os = "windows")]
fn start_windows_clipboard_listener() -> Receiver<()> {
    static WINDOWS_CLIPBOARD_SIGNAL: OnceLock<std::sync::Mutex<Option<Sender<()>>>> =
        OnceLock::new();

    unsafe extern "system" fn clipboard_listener_wnd_proc(
        hwnd: windows::Win32::Foundation::HWND,
        msg: u32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, WM_CLIPBOARDUPDATE};

        if msg == WM_CLIPBOARDUPDATE {
            if let Some(signal) = WINDOWS_CLIPBOARD_SIGNAL.get() {
                let tx = match signal.lock() {
                    Ok(guard) => guard.as_ref().cloned(),
                    Err(error) => {
                        tracing::debug!(
                            error = %error,
                            "failed to lock windows clipboard listener signal"
                        );
                        None
                    }
                };
                if let Some(tx) = tx {
                    crate::logging::debug_if_err(tx.send(()), "signal windows clipboard update");
                }
            }
            return windows::Win32::Foundation::LRESULT(0);
        }

        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    let (tx, rx) = mpsc::channel();
    *WINDOWS_CLIPBOARD_SIGNAL
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(tx.clone());

    crate::logging::spawn_named("tempo-windows-clipboard-listener", move || {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::DataExchange::AddClipboardFormatListener;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage,
            HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW,
        };

        unsafe {
            let class_name = windows_wide("TempoClipboardListener");
            let empty_title = windows_wide("");
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(clipboard_listener_wnd_proc),
                lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            let class_atom = RegisterClassW(&window_class);
            if class_atom == 0 {
                tracing::debug!(
                    "windows clipboard listener window class registration returned zero"
                );
            }

            let Ok(hwnd) = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::PCWSTR(class_name.as_ptr()),
                windows::core::PCWSTR(empty_title.as_ptr()),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                None,
                None,
            ) else {
                tracing::error!("failed to create windows clipboard listener window");
                crate::logging::debug_if_err(
                    tx.send(()),
                    "signal windows clipboard listener startup failure",
                );
                return;
            };

            if let Err(error) = AddClipboardFormatListener(hwnd) {
                tracing::error!(error = %error, "failed to register windows clipboard format listener");
                crate::logging::debug_if_err(
                    tx.send(()),
                    "signal windows clipboard listener registration failure",
                );
                return;
            }

            crate::logging::debug_if_err(tx.send(()), "signal windows clipboard listener ready");

            let mut message = MSG::default();
            while GetMessageW(&mut message, HWND(std::ptr::null_mut()), 0, 0).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    });

    rx
}

#[cfg(target_os = "windows")]
fn windows_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn windows_clipboard_sequence() -> u32 {
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(target_os = "windows")]
fn sync_windows_clipboard_sequence(last_sequence: &mut u32) {
    let sequence = windows_clipboard_sequence();
    if sequence != 0 {
        *last_sequence = sequence;
    }
}

#[cfg(target_os = "windows")]
fn wait_for_windows_clipboard_change(rx: &Receiver<()>, last_sequence: u32) -> bool {
    let got_signal = match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(_) => {
            while rx.try_recv().is_ok() {}
            true
        }
        Err(RecvTimeoutError::Timeout) => false,
        Err(RecvTimeoutError::Disconnected) => {
            std::thread::sleep(Duration::from_secs(1));
            false
        }
    };

    let sequence = windows_clipboard_sequence();
    let sequence_changed = sequence != 0 && sequence != last_sequence;
    if !got_signal && !sequence_changed {
        return false;
    }

    std::thread::sleep(Duration::from_millis(WINDOWS_CLIPBOARD_SETTLE_MS));
    true
}

#[cfg(target_os = "macos")]
fn macos_pasteboard_changed(last_change_count: &mut Option<isize>) -> bool {
    let Some(change_count) = macos_pasteboard_change_count() else {
        return true;
    };

    let changed = last_change_count
        .map(|last| last != change_count)
        .unwrap_or(true);
    *last_change_count = Some(change_count);

    if changed {
        std::thread::sleep(Duration::from_millis(160));
    }
    changed
}

#[cfg(target_os = "macos")]
fn macos_pasteboard_change_count() -> Option<isize> {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let pasteboard_class = Class::get("NSPasteboard")?;
        let pasteboard: *mut Object = msg_send![pasteboard_class, generalPasteboard];
        if pasteboard.is_null() {
            return None;
        }
        let change_count: isize = msg_send![pasteboard, changeCount];
        Some(change_count)
    }
}

pub fn emit_clipboard_update(app: &AppHandle) {
    crate::logging::debug_if_err(app.emit("clipboard-update", ()), "emit clipboard update");
    crate::logging::debug_if_err(
        app.emit_to("shelf-picker", "clipboard-update", ()),
        "emit shelf picker clipboard update",
    );
    crate::logging::debug_if_err(
        app.emit_to("main", "clipboard-update", ()),
        "emit main clipboard update",
    );
}

pub fn prewarm_clipboard_image_cache(app: AppHandle, state: AppState, contents: Vec<String>) {
    crate::logging::spawn_named("tempo-clipboard-image-prewarm", move || {
        let mut seen = HashSet::new();
        for content in contents.into_iter().take(DECODED_IMAGE_CACHE_MAX_ENTRIES) {
            let cache_key = normalize_clipboard_image_reference(&content);
            if !seen.insert(cache_key.clone()) {
                continue;
            }
            if get_cached_clipboard_image(&state, &cache_key).is_some() {
                continue;
            }

            let start = Instant::now();
            let Some((width, height, rgba, timing)) =
                load_clipboard_image_rgba_timed(&app, &content)
            else {
                debug_clipboard_log(format!(
                    "prewarm image failed reference_kind={}",
                    clipboard_reference_kind(&content)
                ));
                continue;
            };
            cache_decoded_clipboard_image(&state, &cache_key, width, height, rgba);
            debug_clipboard_log(format!(
                "prewarm image cached reference_kind={} width={} height={} png_bytes={} read_ms={} decode_ms={} total_ms={} outer_ms={}",
                clipboard_reference_kind(&content),
                width,
                height,
                timing.png_bytes,
                timing.read_ms,
                timing.decode_ms,
                timing.total_ms,
                start.elapsed().as_millis()
            ));
        }
    });
}

pub fn write_clipboard_text(state: &AppState, text: &str) -> Result<(), String> {
    debug_clipboard_log(format!(
        "write text requested chars={}",
        text.chars().count()
    ));
    with_skip_capture(state, || {
        let _guard = clipboard_access_guard();
        Clipboard::new()
            .map_err(|error| error.to_string())?
            .set_text(text.to_string())
            .map_err(|error| error.to_string())
    })
}

pub fn write_clipboard_image(
    state: &AppState,
    app: &AppHandle,
    content: &str,
) -> Result<(), String> {
    let total_start = Instant::now();
    debug_clipboard_log(format!(
        "write image requested reference_kind={} chars={}",
        clipboard_reference_kind(content),
        content.chars().count()
    ));

    let cache_key = normalize_clipboard_image_reference(content);
    let image = if let Some(cached) = get_cached_clipboard_image(state, &cache_key) {
        debug_clipboard_log(format!(
            "write image cache hit reference_kind={} width={} height={} rgba_bytes={}",
            clipboard_reference_kind(content),
            cached.width,
            cached.height,
            cached.rgba.len()
        ));
        cached
    } else {
        let load_start = Instant::now();
        let (width, height, rgba, timing) = match load_clipboard_image_rgba_timed(app, content) {
            Some(image) => image,
            None => {
                debug_clipboard_log("write image failed: load_clipboard_image_rgba returned None");
                return Err("image data is invalid".to_string());
            }
        };
        debug_clipboard_log(format!(
            "write image cache miss reference_kind={} png_bytes={} read_ms={} decode_ms={} load_total_ms={} outer_load_ms={} rgba_bytes={}",
            clipboard_reference_kind(content),
            timing.png_bytes,
            timing.read_ms,
            timing.decode_ms,
            timing.total_ms,
            load_start.elapsed().as_millis(),
            rgba.len()
        ));
        cache_decoded_clipboard_image(state, &cache_key, width, height, rgba)
    };

    let clipboard_new_start = Instant::now();
    let clipboard = Clipboard::new();
    let clipboard_new_ms = clipboard_new_start.elapsed().as_millis();
    let mut clipboard = match clipboard {
        Ok(clipboard) => clipboard,
        Err(error) => {
            debug_clipboard_log(format!(
                "write image failed: Clipboard::new error={error} clipboard_new_ms={clipboard_new_ms}"
            ));
            return Err(error.to_string());
        }
    };

    let set_image_start = Instant::now();
    let result = with_skip_capture(state, || {
        let _guard = clipboard_access_guard();
        clipboard
            .set_image(ImageData {
                width: image.width as usize,
                height: image.height as usize,
                bytes: Cow::Borrowed(image.rgba.as_slice()),
            })
            .map_err(|error| error.to_string())
    });
    let set_image_ms = set_image_start.elapsed().as_millis();
    match &result {
        Ok(_) => debug_clipboard_log(format!(
            "write image succeeded clipboard_new_ms={} set_image_ms={} total_ms={}",
            clipboard_new_ms,
            set_image_ms,
            total_start.elapsed().as_millis()
        )),
        Err(error) => debug_clipboard_log(format!(
            "write image failed: {error} clipboard_new_ms={} set_image_ms={} total_ms={}",
            clipboard_new_ms,
            set_image_ms,
            total_start.elapsed().as_millis()
        )),
    }
    result
}

pub fn use_clipboard_text(state: &AppState, app: &AppHandle, text: &str) -> Result<(), String> {
    let settings = {
        let conn = state.db.lock();
        load_settings(&conn)
    };
    write_clipboard_text(state, text)?;
    maybe_simulate_paste(app, &settings);
    Ok(())
}

pub fn use_clipboard_entry(
    state: &AppState,
    app: &AppHandle,
    entry: &crate::clipboard_db::ClipboardEntry,
) -> Result<(), String> {
    let settings = {
        let conn = state.db.lock();
        load_settings(&conn)
    };

    if entry.kind == "image" {
        debug_clipboard_log(format!(
            "use entry id={} kind=image width={:?} height={:?}",
            entry.id, entry.image_width, entry.image_height
        ));
        if settings.clipboard_plain_text_only {
            debug_clipboard_log("plain_text_only=true; image copy continues because this setting only applies to text");
        }
        write_clipboard_image(state, app, &entry.content)?;
    } else {
        debug_clipboard_log(format!(
            "use entry id={} kind={} text_chars={}",
            entry.id,
            entry.kind,
            entry.content.chars().count()
        ));
        write_clipboard_text(state, &entry.content)?;
    }

    maybe_simulate_paste(app, &settings);
    Ok(())
}

fn maybe_simulate_paste(app: &AppHandle, settings: &crate::db::Settings) {
    if settings.clipboard_paste_mode != "active_app" {
        return;
    }

    let app = app.clone();
    crate::logging::spawn_named("tempo-clipboard-simulate-paste", move || {
        if let Some(window) = app.get_webview_window("shelf-picker") {
            crate::logging::debug_if_err(window.hide(), "hide shelf picker before paste");
        }
        #[cfg(target_os = "macos")]
        {
            if crate::macos_dock::is_main_window_in_tray() {
                crate::macos_dock::ensure_main_window_hidden(&app);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(120));
        if let Err(error) = crate::platform::simulate_paste() {
            tracing::warn!(error = %error, "simulate paste failed");
        }
    });
}

pub fn copy_clipboard_entry_by_id(
    state: &AppState,
    app: &AppHandle,
    id: i64,
) -> Result<(), String> {
    debug_clipboard_log(format!("copy entry by id requested id={id}"));
    let entry = {
        let conn = state.db.lock();
        get_clipboard_entry(&conn, id).ok_or_else(|| "clipboard entry not found".to_string())?
    };
    debug_clipboard_log(format!(
        "copy entry fetched id={} kind={} content_summary={}",
        entry.id,
        entry.kind,
        clipboard_entry_content_summary(&entry)
    ));
    use_clipboard_entry(state, app, &entry)?;
    {
        let conn = state.db.lock();
        touch_clipboard_entry(&conn, id);
    }
    emit_clipboard_update(app);
    Ok(())
}

fn debug_clipboard_log(message: impl AsRef<str>) {
    tracing::debug!(
        target: "tempo::clipboard",
        message = %crate::logging::sanitize_log_value(message.as_ref()),
        "clipboard runtime"
    );
}

fn clipboard_reference_kind(content: &str) -> &'static str {
    if crate::clipboard_images::is_legacy_clipboard_image_data_url(content) {
        "legacy-data-url"
    } else if crate::clipboard_images::is_clipboard_image_storage_key(content) {
        "storage-key"
    } else {
        "external-reference"
    }
}

pub(crate) fn clipboard_entry_content_summary(
    entry: &crate::clipboard_db::ClipboardEntry,
) -> String {
    if entry.kind == "image" {
        format!(
            "reference_kind={} chars={}",
            clipboard_reference_kind(&entry.content),
            entry.content.chars().count()
        )
    } else {
        format!("text_chars={}", entry.content.chars().count())
    }
}

fn get_cached_clipboard_image(state: &AppState, key: &str) -> Option<CachedClipboardImage> {
    let mut runtime = state.clipboard.lock();
    let cached = runtime.decoded_image_cache.get(key).cloned()?;
    runtime
        .decoded_image_cache_order
        .retain(|cached_key| cached_key != key);
    runtime.decoded_image_cache_order.push_back(key.to_string());
    Some(cached)
}

fn cache_decoded_clipboard_image(
    state: &AppState,
    key: &str,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> CachedClipboardImage {
    let image = CachedClipboardImage {
        width,
        height,
        rgba: Arc::new(rgba),
    };
    let image_bytes = image.rgba.len();

    let mut runtime = state.clipboard.lock();
    if let Some(previous) = runtime.decoded_image_cache.remove(key) {
        runtime.decoded_image_cache_bytes = runtime
            .decoded_image_cache_bytes
            .saturating_sub(previous.rgba.len());
        runtime
            .decoded_image_cache_order
            .retain(|cached_key| cached_key != key);
    }

    runtime
        .decoded_image_cache
        .insert(key.to_string(), image.clone());
    runtime.decoded_image_cache_order.push_back(key.to_string());
    runtime.decoded_image_cache_bytes += image_bytes;

    while runtime.decoded_image_cache.len() > DECODED_IMAGE_CACHE_MAX_ENTRIES
        || runtime.decoded_image_cache_bytes > DECODED_IMAGE_CACHE_MAX_BYTES
    {
        let Some(oldest_key) = runtime.decoded_image_cache_order.pop_front() else {
            break;
        };
        if let Some(oldest) = runtime.decoded_image_cache.remove(&oldest_key) {
            runtime.decoded_image_cache_bytes = runtime
                .decoded_image_cache_bytes
                .saturating_sub(oldest.rgba.len());
        }
    }

    debug_clipboard_log(format!(
        "decoded image cached reference_kind={} image_bytes={} cache_entries={} cache_bytes={}",
        clipboard_reference_kind(key),
        image_bytes,
        runtime.decoded_image_cache.len(),
        runtime.decoded_image_cache_bytes
    ));

    image
}

fn with_skip_capture<T>(
    state: &AppState,
    write: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    {
        let mut runtime = state.clipboard.lock();
        runtime.skip_next_capture = true;
    }

    let result = write();

    crate::logging::spawn_named("tempo-clipboard-skip-reset", {
        let state = state.clone();
        move || {
            std::thread::sleep(std::time::Duration::from_millis(900));
            let mut runtime = state.clipboard.lock();
            runtime.skip_next_capture = false;
        }
    });

    result
}

pub(crate) fn resolve_clipboard_source(
    current: Option<ForegroundApp>,
    fallback_app: Option<&str>,
    fallback_process: Option<&str>,
) -> (Option<String>, Option<String>) {
    if let Some(app) = current {
        if is_system_clipboard_source(&app.name, &app.process_name) {
            return (
                fallback_app.map(str::to_string),
                fallback_process.map(str::to_string),
            );
        }
        return (Some(app.name), Some(app.process_name));
    }

    (
        fallback_app.map(str::to_string),
        fallback_process.map(str::to_string),
    )
}

fn is_system_clipboard_source(name: &str, process: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let process = process.to_ascii_lowercase();
    [
        "screencaptureui",
        "screenshot",
        "截图",
        "screen capture",
        "snippingtool",
        "snip & sketch",
    ]
    .iter()
    .any(|needle| name.contains(needle) || process.contains(needle))
}
