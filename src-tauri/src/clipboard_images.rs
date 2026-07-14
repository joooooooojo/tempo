use crate::asset_protocol::{
    asset_dir, asset_protocol_response, asset_url_for_file_name, storage_key_from_protocol_url,
};
use crate::clipboard_db::{hash_bytes, ClipboardEntry};
use base64::Engine as _;
use rusqlite::Connection;
use std::path::Path;
use std::time::Instant;
use tauri::http::{Request, Response};
use tauri::AppHandle;

pub const CLIPBOARD_IMAGE_PROTOCOL: &str = "tempo-clipboard-image";
pub const CLIPBOARD_IMAGE_SUBDIR: &str = "clipboard-images";

pub fn clipboard_images_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    asset_dir(app, CLIPBOARD_IMAGE_SUBDIR)
}

pub fn clipboard_image_storage_key(content_hash: &str) -> String {
    format!("{CLIPBOARD_IMAGE_SUBDIR}/{content_hash}.png")
}

pub fn is_clipboard_image_storage_key(content: &str) -> bool {
    let Some(file_name) = content.strip_prefix(&format!("{CLIPBOARD_IMAGE_SUBDIR}/")) else {
        return false;
    };
    is_valid_clipboard_image_file_name(file_name)
}

pub fn is_legacy_clipboard_image_data_url(content: &str) -> bool {
    content.starts_with("data:image/png;base64,")
}

pub fn save_clipboard_image_png(
    app: &AppHandle,
    content_hash: &str,
    png_bytes: &[u8],
) -> Result<String, String> {
    let file_name = format!("{content_hash}.png");
    if !is_valid_clipboard_image_file_name(&file_name) {
        return Err("图片文件名无效".into());
    }

    let dir = clipboard_images_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&file_name);
    if !path.exists() {
        std::fs::write(&path, png_bytes).map_err(|e| e.to_string())?;
    }
    Ok(clipboard_image_storage_key(content_hash))
}

pub fn hydrate_clipboard_image_urls(
    app: &AppHandle,
    conn: &Connection,
    entries: &mut [ClipboardEntry],
) {
    for entry in entries.iter_mut() {
        if entry.kind != "image" {
            continue;
        }
        let original = entry.content.clone();
        if is_legacy_clipboard_image_data_url(&original) {
            if let Ok(storage_key) = migrate_legacy_image_content(app, &original) {
                if let Err(error) = conn.execute(
                    "UPDATE clipboard_history SET content = ?1 WHERE id = ?2",
                    rusqlite::params![storage_key, entry.id],
                ) {
                    tracing::warn!(
                        entry_id = entry.id,
                        error = %error,
                        "failed to update hydrated clipboard image storage key"
                    );
                }
                entry.content = hydrate_clipboard_image_content(&storage_key);
                continue;
            }
            tracing::debug!(
                entry_id = entry.id,
                "failed to hydrate legacy clipboard image"
            );
        }
        entry.content = hydrate_clipboard_image_content(&original);
    }
}

pub fn hydrate_clipboard_image_content(content: &str) -> String {
    if is_clipboard_image_storage_key(content) {
        let file_name = content
            .strip_prefix(&format!("{CLIPBOARD_IMAGE_SUBDIR}/"))
            .unwrap_or(content);
        return asset_url_for_file_name(CLIPBOARD_IMAGE_PROTOCOL, file_name);
    }
    if storage_key_from_protocol_url(CLIPBOARD_IMAGE_PROTOCOL, CLIPBOARD_IMAGE_SUBDIR, content)
        .is_some()
    {
        return content.to_string();
    }
    content.to_string()
}

pub fn normalize_clipboard_image_reference(value: &str) -> String {
    if let Some(storage_key) =
        storage_key_from_protocol_url(CLIPBOARD_IMAGE_PROTOCOL, CLIPBOARD_IMAGE_SUBDIR, value)
    {
        return storage_key;
    }
    if is_clipboard_image_storage_key(value) {
        return value.to_string();
    }
    value.to_string()
}

#[derive(Debug, Clone, Copy)]
pub struct ClipboardImageLoadTiming {
    pub read_ms: u128,
    pub decode_ms: u128,
    pub total_ms: u128,
    pub png_bytes: usize,
}

pub fn load_clipboard_image_rgba_timed(
    app: &AppHandle,
    content: &str,
) -> Option<(u32, u32, Vec<u8>, ClipboardImageLoadTiming)> {
    let total_start = Instant::now();
    if is_legacy_clipboard_image_data_url(content) {
        let decode_start = Instant::now();
        let png_bytes = match decode_legacy_png_bytes(content) {
            Some(bytes) => bytes,
            None => {
                tracing::debug!("failed to decode legacy clipboard image data url");
                return None;
            }
        };
        let png_len = png_bytes.len();
        let (width, height, rgba) = match decode_png_bytes(&png_bytes) {
            Some(decoded) => decoded,
            None => {
                tracing::debug!(
                    png_bytes = png_len,
                    "failed to decode legacy clipboard image png"
                );
                return None;
            }
        };
        let decode_ms = decode_start.elapsed().as_millis();
        return Some((
            width,
            height,
            rgba,
            ClipboardImageLoadTiming {
                read_ms: 0,
                decode_ms,
                total_ms: total_start.elapsed().as_millis(),
                png_bytes: png_len,
            },
        ));
    }
    let storage_key = normalize_clipboard_image_reference(content);
    if !is_clipboard_image_storage_key(&storage_key) {
        return None;
    }
    let read_start = Instant::now();
    let png_bytes = match read_clipboard_image_bytes(app, &storage_key) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::debug!(
                error = %error,
                "failed to read clipboard image bytes"
            );
            return None;
        }
    };
    let read_ms = read_start.elapsed().as_millis();
    let png_len = png_bytes.len();
    let decode_start = Instant::now();
    let (width, height, rgba) = match decode_png_bytes(&png_bytes) {
        Some(decoded) => decoded,
        None => {
            tracing::debug!(png_bytes = png_len, "failed to decode clipboard image png");
            return None;
        }
    };
    let decode_ms = decode_start.elapsed().as_millis();
    Some((
        width,
        height,
        rgba,
        ClipboardImageLoadTiming {
            read_ms,
            decode_ms,
            total_ms: total_start.elapsed().as_millis(),
            png_bytes: png_len,
        },
    ))
}

pub fn maybe_delete_clipboard_image_file(conn: &Connection, app: &AppHandle, content: &str) {
    let storage_key = normalize_clipboard_image_reference(content);
    if !is_clipboard_image_storage_key(&storage_key) {
        return;
    }
    let still_referenced = match conn.query_row(
        "SELECT COUNT(*) FROM clipboard_history WHERE content = ?1",
        [&storage_key],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(count) => count > 0,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "failed to count clipboard image references"
            );
            true
        }
    };
    if still_referenced {
        return;
    }
    if let Ok(dir) = clipboard_images_dir(app) {
        if let Some(file_name) = storage_key.strip_prefix(&format!("{CLIPBOARD_IMAGE_SUBDIR}/")) {
            if let Err(error) = std::fs::remove_file(dir.join(file_name)) {
                tracing::debug!(error = %error, "failed to remove unreferenced clipboard image");
            }
        }
    } else {
        tracing::debug!("failed to resolve clipboard image directory for cleanup");
    }
}

pub fn migrate_legacy_clipboard_images(app: &AppHandle, conn: &Connection) {
    let mut stmt = match conn.prepare(
        "SELECT id, content FROM clipboard_history
         WHERE kind = 'image' AND content LIKE 'data:image/png;base64,%'",
    ) {
        Ok(stmt) => stmt,
        Err(error) => {
            tracing::warn!(error = %error, "failed to prepare legacy clipboard image migration query");
            return;
        }
    };
    let rows = match stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    }) {
        Ok(rows) => {
            let mut values = Vec::new();
            for row in rows {
                match row {
                    Ok(value) => values.push(value),
                    Err(error) => {
                        tracing::warn!(error = %error, "failed to read legacy clipboard image row");
                    }
                }
            }
            values
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to query legacy clipboard images");
            return;
        }
    };

    for (id, content) in rows {
        let Ok(storage_key) = migrate_legacy_image_content(app, &content) else {
            tracing::debug!(
                entry_id = id,
                "failed to migrate legacy clipboard image content"
            );
            continue;
        };
        if let Err(error) = conn.execute(
            "UPDATE clipboard_history SET content = ?1 WHERE id = ?2",
            rusqlite::params![storage_key, id],
        ) {
            tracing::warn!(
                entry_id = id,
                error = %error,
                "failed to update migrated clipboard image row"
            );
        }
    }
}

pub fn clipboard_image_protocol_response(
    app: &AppHandle,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    asset_protocol_response(
        app,
        CLIPBOARD_IMAGE_SUBDIR,
        |_| "image/png",
        is_valid_clipboard_image_file_name,
        request,
    )
}

fn migrate_legacy_image_content(app: &AppHandle, data_url: &str) -> Result<String, String> {
    let png_bytes = decode_legacy_png_bytes(data_url).ok_or_else(|| "图片数据无效".to_string())?;
    let content_hash = hash_bytes(&png_bytes);
    save_clipboard_image_png(app, &content_hash, &png_bytes)
}

fn read_clipboard_image_bytes(app: &AppHandle, storage_key: &str) -> Result<Vec<u8>, String> {
    let file_name = storage_key
        .strip_prefix(&format!("{CLIPBOARD_IMAGE_SUBDIR}/"))
        .ok_or_else(|| "图片路径无效".to_string())?;
    if !is_valid_clipboard_image_file_name(file_name) {
        return Err("图片路径无效".into());
    }
    let dir = clipboard_images_dir(app)?;
    let path = dir.join(file_name);
    let canonical_path = path.canonicalize().map_err(|_| "图片不存在".to_string())?;
    let canonical_dir = dir.canonicalize().map_err(|_| "图片目录无效".to_string())?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err("图片路径无效".into());
    }
    std::fs::read(canonical_path).map_err(|_| "图片读取失败".to_string())
}

fn decode_legacy_png_bytes(data_url: &str) -> Option<Vec<u8>> {
    let payload = data_url.strip_prefix("data:image/png;base64,")?;
    match base64::engine::general_purpose::STANDARD.decode(payload) {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            tracing::debug!(error = %error, "failed to decode legacy clipboard image base64");
            None
        }
    }
}

fn decode_png_bytes(png_bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = match decoder.read_info() {
        Ok(reader) => reader,
        Err(error) => {
            tracing::debug!(
                png_bytes = png_bytes.len(),
                error = %error,
                "failed to read clipboard png metadata"
            );
            return None;
        }
    };
    let width = reader.info().width;
    let height = reader.info().height;
    let mut rgba = vec![0_u8; reader.output_buffer_size()];
    if let Err(error) = reader.next_frame(&mut rgba) {
        tracing::debug!(
            width = width,
            height = height,
            png_bytes = png_bytes.len(),
            error = %error,
            "failed to decode clipboard png frame"
        );
        return None;
    }
    rgba.truncate((width as usize) * (height as usize) * 4);
    Some((width, height, rgba))
}

fn is_valid_clipboard_image_file_name(file_name: &str) -> bool {
    let Some(stem) = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
    else {
        return false;
    };
    file_name.ends_with(".png") && stem.len() == 16 && stem.chars().all(|ch| ch.is_ascii_hexdigit())
}
