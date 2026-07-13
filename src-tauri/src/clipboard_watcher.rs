use crate::clipboard_db::{
    encode_rgba_png, get_clipboard_entry, hash_bytes, insert_clipboard_image,
    insert_clipboard_text, touch_clipboard_entry,
};
#[cfg(target_os = "windows")]
use crate::clipboard_images::load_clipboard_image_png_bytes_timed;
use crate::clipboard_images::save_clipboard_image_png;
#[cfg(not(target_os = "windows"))]
use crate::clipboard_images::{
    load_clipboard_image_rgba_timed, normalize_clipboard_image_reference,
};
#[cfg(not(target_os = "windows"))]
use crate::db::CachedClipboardImage;
use crate::db::{load_settings, AppState};
use crate::platform::{get_foreground_app, ForegroundApp};
use arboard::Clipboard;
#[cfg(not(target_os = "windows"))]
use arboard::ImageData;
#[cfg(not(target_os = "windows"))]
use std::borrow::Cow;
#[cfg(not(target_os = "windows"))]
use std::collections::HashSet;
#[cfg(not(target_os = "windows"))]
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

#[cfg(not(target_os = "windows"))]
const DECODED_IMAGE_CACHE_MAX_ENTRIES: usize = 16;
#[cfg(not(target_os = "windows"))]
const DECODED_IMAGE_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

pub fn start_clipboard_watcher(app: AppHandle, state: AppState) {
    std::thread::spawn(move || {
        let mut clipboard = match Clipboard::new() {
            Ok(clipboard) => clipboard,
            Err(error) => {
                eprintln!("clipboard watcher unavailable: {error}");
                return;
            }
        };
        let mut last_text_hash = String::new();
        let mut last_image_hash = String::new();
        let mut last_retention_purge = std::time::Instant::now();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));

            let settings = {
                let conn = state.db.lock();
                load_settings(&conn)
            };
            if !settings.clipboard_monitor_enabled {
                continue;
            }

            if last_retention_purge.elapsed() >= std::time::Duration::from_secs(3600) {
                last_retention_purge = std::time::Instant::now();
                let retention = settings.clipboard_history_retention.clone();
                let conn = state.db.lock();
                crate::clipboard_db::purge_clipboard_history_by_retention(&conn, &retention);
            }

            if let Some(app_info) = get_foreground_app().filter(|app| !is_tempo_app(app)) {
                let mut runtime = state.clipboard.lock();
                runtime.last_source_app = Some(app_info.name.clone());
                runtime.last_source_process = Some(app_info.process_name.clone());
            }

            {
                let runtime = state.clipboard.lock();
                if runtime.skip_next_capture {
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
            let mut changed = false;

            if let Ok(image) = clipboard.get_image() {
                let width = image.width as u32;
                let height = image.height as u32;
                let pixel_count = width as u64 * height as u64;
                if pixel_count > 0 && pixel_count <= crate::clipboard_db::MAX_CLIPBOARD_IMAGE_PIXELS
                {
                    if let Some(png_bytes) = encode_rgba_png(width, height, image.bytes.as_ref()) {
                        if png_bytes.len() <= crate::clipboard_db::MAX_CLIPBOARD_IMAGE_BYTES {
                            let content_hash = hash_bytes(&png_bytes);
                            if content_hash != last_image_hash {
                                last_image_hash = content_hash.clone();
                                last_text_hash.clear();

                                let inserted = if let Ok(storage_key) =
                                    save_clipboard_image_png(&app, &content_hash, &png_bytes)
                                {
                                    #[cfg(not(target_os = "windows"))]
                                    cache_decoded_clipboard_image(
                                        &state,
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
                                        source_app.as_deref(),
                                        source_process.as_deref(),
                                        max_entries,
                                    )
                                } else {
                                    None
                                };
                                changed = inserted.is_some();
                            }
                        }
                    }
                }
            } else if let Ok(text) = clipboard.get_text() {
                if !text.is_empty() {
                    let hash = crate::clipboard_db::hash_content(&text);
                    if hash != last_text_hash {
                        last_text_hash = hash;
                        last_image_hash.clear();

                        let inserted = {
                            let conn = state.db.lock();
                            insert_clipboard_text(
                                &conn,
                                &text,
                                source_app.as_deref(),
                                source_process.as_deref(),
                                max_entries,
                            )
                        };
                        changed = inserted.is_some();
                    }
                }
            }

            if changed {
                emit_clipboard_update(&app);
            }
        }
    });
}

pub fn emit_clipboard_update(app: &AppHandle) {
    let _ = app.emit("clipboard-update", ());
    let _ = app.emit_to("shelf-picker", "clipboard-update", ());
    let _ = app.emit_to("main", "clipboard-update", ());
}

#[cfg(not(target_os = "windows"))]
pub fn prewarm_clipboard_image_cache(app: AppHandle, state: AppState, contents: Vec<String>) {
    std::thread::spawn(move || {
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
                debug_clipboard_log(format!("prewarm image failed key={cache_key}"));
                continue;
            };
            cache_decoded_clipboard_image(&state, &cache_key, width, height, rgba);
            debug_clipboard_log(format!(
                "prewarm image cached key={} png_bytes={} read_ms={} decode_ms={} total_ms={} outer_ms={}",
                cache_key,
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
        Clipboard::new()
            .map_err(|error| error.to_string())?
            .set_text(text.to_string())
            .map_err(|error| error.to_string())
    })
}

#[cfg(target_os = "windows")]
pub fn write_clipboard_image(
    state: &AppState,
    app: &AppHandle,
    content: &str,
) -> Result<(), String> {
    let total_start = Instant::now();
    debug_clipboard_log(format!(
        "write image requested windows_png_direct content_prefix={}",
        content.chars().take(96).collect::<String>()
    ));

    let load_start = Instant::now();
    let (png_bytes, timing) = match load_clipboard_image_png_bytes_timed(app, content) {
        Some(image) => image,
        None => {
            debug_clipboard_log("write image failed: load_clipboard_image_png_bytes returned None");
            return Err("image data is invalid".to_string());
        }
    };
    debug_clipboard_log(format!(
        "write image png loaded png_bytes={} read_ms={} load_total_ms={} outer_load_ms={}",
        timing.png_bytes,
        timing.read_ms,
        timing.total_ms,
        load_start.elapsed().as_millis()
    ));

    let set_png_start = Instant::now();
    let result = with_skip_capture(state, || write_clipboard_png_windows(&png_bytes));
    let set_png_ms = set_png_start.elapsed().as_millis();
    match &result {
        Ok(_) => debug_clipboard_log(format!(
            "write image succeeded windows_png_direct set_png_ms={} total_ms={}",
            set_png_ms,
            total_start.elapsed().as_millis()
        )),
        Err(error) => debug_clipboard_log(format!(
            "write image failed windows_png_direct: {error} set_png_ms={} total_ms={}",
            set_png_ms,
            total_start.elapsed().as_millis()
        )),
    }
    result
}

#[cfg(not(target_os = "windows"))]
pub fn write_clipboard_image(
    state: &AppState,
    app: &AppHandle,
    content: &str,
) -> Result<(), String> {
    let total_start = Instant::now();
    debug_clipboard_log(format!(
        "write image requested content_prefix={}",
        content.chars().take(96).collect::<String>()
    ));

    let cache_key = normalize_clipboard_image_reference(content);
    let image = if let Some(cached) = get_cached_clipboard_image(state, &cache_key) {
        debug_clipboard_log(format!(
            "write image cache hit key={} width={} height={} rgba_bytes={}",
            cache_key,
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
            "write image cache miss key={} png_bytes={} read_ms={} decode_ms={} load_total_ms={} outer_load_ms={} rgba_bytes={}",
            cache_key,
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
    std::thread::spawn(move || {
        if let Some(window) = app.get_webview_window("shelf-picker") {
            let _ = window.hide();
        }
        #[cfg(target_os = "macos")]
        {
            if crate::macos_dock::is_main_window_in_tray() {
                crate::macos_dock::ensure_main_window_hidden(&app);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(120));
        if let Err(error) = crate::platform::simulate_paste() {
            eprintln!("simulate paste failed: {error}");
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
        "copy entry fetched id={} kind={} content_prefix={}",
        entry.id,
        entry.kind,
        entry.content.chars().take(96).collect::<String>()
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
    #[cfg(debug_assertions)]
    eprintln!("[tempo-debug][clipboard] {}", message.as_ref());

    #[cfg(not(debug_assertions))]
    let _ = message;
}

#[cfg(target_os = "windows")]
fn write_clipboard_png_windows(png_bytes: &[u8]) -> Result<(), String> {
    use windows::core::{Error as WindowsError, PCWSTR};
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
    };

    if png_bytes.is_empty() {
        return Err("image data is empty".to_string());
    }

    unsafe {
        let png_format_name = "PNG"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let png_format = RegisterClipboardFormatW(PCWSTR(png_format_name.as_ptr()));
        if png_format == 0 {
            return Err(format!(
                "RegisterClipboardFormatW(PNG) failed: {}",
                WindowsError::from_win32()
            ));
        }

        let hglobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, png_bytes.len())
            .map_err(|error| format!("GlobalAlloc failed: {error}"))?;
        let data_ptr = GlobalLock(hglobal) as *mut u8;
        if data_ptr.is_null() {
            free_global(hglobal);
            return Err(format!("GlobalLock failed: {}", WindowsError::from_win32()));
        }
        std::ptr::copy_nonoverlapping(png_bytes.as_ptr(), data_ptr, png_bytes.len());
        // GlobalUnlock returns zero both on success when the lock count reaches zero and on
        // failure; the windows crate maps zero to Err, so do not treat that wrapper result as
        // authoritative here.
        let _ = GlobalUnlock(hglobal);

        OpenClipboard(HWND(std::ptr::null_mut())).map_err(|error| {
            free_global(hglobal);
            format!("OpenClipboard failed: {error}")
        })?;

        let mut ownership_transferred = false;
        let write_result = (|| -> Result<(), String> {
            EmptyClipboard().map_err(|error| format!("EmptyClipboard failed: {error}"))?;
            SetClipboardData(png_format, HANDLE(hglobal.0))
                .map_err(|error| format!("SetClipboardData(PNG) failed: {error}"))?;
            ownership_transferred = true;
            Ok(())
        })();

        let close_result = CloseClipboard();
        if !ownership_transferred {
            free_global(hglobal);
        }
        if let Err(error) = close_result {
            if write_result.is_ok() {
                return Err(format!("CloseClipboard failed: {error}"));
            }
        }

        write_result
    }
}

#[cfg(target_os = "windows")]
fn free_global(hglobal: windows::Win32::Foundation::HGLOBAL) {
    unsafe {
        let _ = windows::Win32::Foundation::GlobalFree(hglobal);
    }
}

#[cfg(not(target_os = "windows"))]
fn get_cached_clipboard_image(state: &AppState, key: &str) -> Option<CachedClipboardImage> {
    let mut runtime = state.clipboard.lock();
    let cached = runtime.decoded_image_cache.get(key).cloned()?;
    runtime
        .decoded_image_cache_order
        .retain(|cached_key| cached_key != key);
    runtime.decoded_image_cache_order.push_back(key.to_string());
    Some(cached)
}

#[cfg(not(target_os = "windows"))]
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
        "decoded image cached key={} image_bytes={} cache_entries={} cache_bytes={}",
        key,
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

    std::thread::spawn({
        let state = state.clone();
        move || {
            std::thread::sleep(std::time::Duration::from_millis(900));
            let mut runtime = state.clipboard.lock();
            runtime.skip_next_capture = false;
        }
    });

    result
}

fn resolve_clipboard_source(
    current: Option<ForegroundApp>,
    fallback_app: Option<&str>,
    fallback_process: Option<&str>,
) -> (Option<String>, Option<String>) {
    if let Some(app) = current.filter(|app| !is_tempo_app(app)) {
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

fn is_tempo_app(app: &ForegroundApp) -> bool {
    let name = app.name.to_ascii_lowercase();
    let process = app.process_name.to_ascii_lowercase();
    name.contains("tempo") || process.contains("tempo")
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
