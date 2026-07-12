use crate::clipboard_db::{
    encode_rgba_png, get_clipboard_entry, hash_bytes, insert_clipboard_image,
    insert_clipboard_text, touch_clipboard_entry,
};
use crate::clipboard_images::{load_clipboard_image_rgba, save_clipboard_image_png};
use crate::db::{load_settings, AppState};
use crate::platform::{get_foreground_app, ForegroundApp};
use arboard::{Clipboard, ImageData};
use std::borrow::Cow;
use tauri::{AppHandle, Emitter, Manager};

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
                if pixel_count > 0 && pixel_count <= crate::clipboard_db::MAX_CLIPBOARD_IMAGE_PIXELS {
                    if let Some(png_bytes) = encode_rgba_png(width, height, image.bytes.as_ref()) {
                        if png_bytes.len() <= crate::clipboard_db::MAX_CLIPBOARD_IMAGE_BYTES {
                            let content_hash = hash_bytes(&png_bytes);
                            if content_hash != last_image_hash {
                                last_image_hash = content_hash.clone();
                                last_text_hash.clear();

                                let inserted = if let Ok(storage_key) =
                                    save_clipboard_image_png(&app, &content_hash, &png_bytes)
                                {
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

pub fn write_clipboard_text(state: &AppState, text: &str) -> Result<(), String> {
    with_skip_capture(state, || {
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
    let (width, height, rgba) =
        load_clipboard_image_rgba(app, content).ok_or_else(|| "图片数据无效".to_string())?;
    with_skip_capture(state, || {
        Clipboard::new()
            .map_err(|error| error.to_string())?
            .set_image(ImageData {
                width: width as usize,
                height: height as usize,
                bytes: Cow::Owned(rgba),
            })
            .map_err(|error| error.to_string())
    })
}

pub fn write_clipboard_entry(
    state: &AppState,
    app: &AppHandle,
    entry: &crate::clipboard_db::ClipboardEntry,
) -> Result<(), String> {
    if entry.kind == "image" {
        write_clipboard_image(state, app, &entry.content)
    } else {
        write_clipboard_text(state, &entry.content)
    }
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
        if settings.clipboard_plain_text_only {
            return Err("已开启纯文本粘贴，无法粘贴图片".into());
        }
        write_clipboard_image(state, app, &entry.content)?;
    } else {
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

pub fn copy_clipboard_entry_by_id(state: &AppState, app: &AppHandle, id: i64) -> Result<(), String> {
    let entry = {
        let conn = state.db.lock();
        get_clipboard_entry(&conn, id).ok_or_else(|| "记录不存在".to_string())?
    };
    use_clipboard_entry(state, app, &entry)?;
    {
        let conn = state.db.lock();
        touch_clipboard_entry(&conn, id);
    }
    emit_clipboard_update(app);
    Ok(())
}

fn with_skip_capture<T>(state: &AppState, write: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
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
