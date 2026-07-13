use crate::asset_protocol::{
    asset_dir, asset_protocol_response, asset_url_for_file_name, storage_key_from_protocol_url,
};
use crate::clipboard_db::hash_bytes;
use crate::db::{TodoImage, TodoNoteImage};
use base64::Engine as _;
use rusqlite::Connection;
use std::path::Path;
use tauri::http::{Request, Response};
use tauri::AppHandle;

use crate::commands::{TodoImageInput, MAX_TODO_IMAGE_BYTES};

pub const TODO_IMAGE_PROTOCOL: &str = "tempo-todo-image";
pub const TODO_IMAGE_SUBDIR: &str = "todo-images";

pub fn todo_images_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    asset_dir(app, TODO_IMAGE_SUBDIR)
}

pub fn is_todo_image_storage_key(value: &str) -> bool {
    let Some(file_name) = value.strip_prefix(&format!("{TODO_IMAGE_SUBDIR}/")) else {
        return false;
    };
    is_valid_todo_image_file_name(file_name)
}

pub fn save_todo_image_input(app: &AppHandle, image: &TodoImageInput) -> Result<String, String> {
    let (bytes, mime) = decode_todo_image_input(image)?;
    let hash = hash_bytes(&bytes);
    let extension = todo_image_extension(&mime)?;
    let file_name = format!("{hash}.{extension}");
    if !is_valid_todo_image_file_name(&file_name) {
        return Err("图片文件名无效".into());
    }

    let dir = todo_images_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&file_name);
    if !path.exists() {
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    }

    Ok(format!("{TODO_IMAGE_SUBDIR}/{file_name}"))
}

pub fn hydrate_todo_image_data_url(value: &str) -> String {
    if is_todo_image_storage_key(value) {
        let file_name = value
            .strip_prefix(&format!("{TODO_IMAGE_SUBDIR}/"))
            .unwrap_or(value);
        return asset_url_for_file_name(TODO_IMAGE_PROTOCOL, file_name);
    }
    if storage_key_from_protocol_url(TODO_IMAGE_PROTOCOL, TODO_IMAGE_SUBDIR, value).is_some() {
        return value.to_string();
    }
    value.to_string()
}

pub fn hydrate_todo_images(images: &mut [TodoImage]) {
    for image in images {
        image.data_url = hydrate_todo_image_data_url(&image.data_url);
    }
}

pub fn hydrate_todo_note_images(images: &mut [TodoNoteImage]) {
    for image in images {
        image.data_url = hydrate_todo_image_data_url(&image.data_url);
    }
}

pub fn migrate_legacy_todo_images(app: &AppHandle, conn: &Connection) {
    migrate_table_legacy_images(
        app,
        conn,
        "todo_images",
        "SELECT id, data_url, mime_type FROM todo_images WHERE data_url LIKE 'data:image/%'",
    );
    migrate_table_legacy_images(
        app,
        conn,
        "todo_note_images",
        "SELECT id, data_url, mime_type FROM todo_note_images WHERE data_url LIKE 'data:image/%'",
    );
}

pub fn todo_image_protocol_response(
    app: &AppHandle,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    asset_protocol_response(
        app,
        TODO_IMAGE_SUBDIR,
        todo_image_content_type_for_file,
        is_valid_todo_image_file_name,
        request,
    )
}

fn migrate_table_legacy_images(app: &AppHandle, conn: &Connection, table: &str, query: &str) {
    let mut stmt = match conn.prepare(query) {
        Ok(stmt) => stmt,
        Err(_) => return,
    };
    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) {
        Ok(rows) => rows.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return,
    };

    for (id, data_url, mime_type) in rows {
        let input = TodoImageInput {
            data_url,
            mime_type,
        };
        let Ok(storage_key) = save_todo_image_input(app, &input) else {
            continue;
        };
        let _ = conn.execute(
            &format!("UPDATE {table} SET data_url = ?1 WHERE id = ?2"),
            rusqlite::params![storage_key, id],
        );
    }
}

fn decode_todo_image_input(image: &TodoImageInput) -> Result<(Vec<u8>, String), String> {
    let mime = image.mime_type.trim().to_ascii_lowercase();
    if !matches!(
        mime.as_str(),
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    ) {
        return Err("仅支持 PNG、JPEG、WebP 或 GIF 图片".into());
    }

    if !image.data_url.starts_with("data:image/") {
        return Err("图片数据格式无效".into());
    }

    let Some((_, payload)) = image.data_url.split_once(',') else {
        return Err("图片数据格式无效".into());
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| "图片数据格式无效".to_string())?;
    if bytes.len() > MAX_TODO_IMAGE_BYTES {
        return Err("单张图片不能超过 5MB".into());
    }
    Ok((bytes, mime))
}

fn todo_image_extension(mime_type: &str) -> Result<&'static str, String> {
    match mime_type {
        "image/png" => Ok("png"),
        "image/jpeg" => Ok("jpg"),
        "image/webp" => Ok("webp"),
        "image/gif" => Ok("gif"),
        _ => Err("仅支持 PNG、JPEG、WebP 或 GIF 图片".into()),
    }
}

fn is_valid_todo_image_file_name(file_name: &str) -> bool {
    let Some(stem) = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
    else {
        return false;
    };
    if stem.len() != 16 || !stem.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return false;
    }
    matches!(
        Path::new(file_name)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png") | Some("jpg") | Some("jpeg") | Some("webp") | Some("gif")
    )
}

fn todo_image_content_type_for_file(file_name: &str) -> &'static str {
    match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}
