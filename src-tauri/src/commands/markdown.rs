use crate::db::current_storage_dir;
use base64::Engine as _;
use chrono::Local;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::http::{
    header::{CONTENT_LENGTH, CONTENT_TYPE},
    Method, Request, Response, StatusCode,
};
use tauri::AppHandle;

use super::{TodoBackupFile, MARKDOWN_IMAGE_PROTOCOL, MAX_TODO_IMAGE_BYTES};

#[tauri::command]
pub fn save_markdown_image(
    app: AppHandle,
    data_url: String,
    mime_type: String,
) -> Result<String, String> {
    let mime = mime_type.trim().to_ascii_lowercase();
    let extension = markdown_image_extension(&mime)?;
    let prefix = format!("data:{mime};base64,");
    let payload = data_url
        .strip_prefix(&prefix)
        .ok_or_else(|| "图片数据格式无效".to_string())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| "图片数据格式无效".to_string())?;

    if bytes.len() > MAX_TODO_IMAGE_BYTES {
        return Err("单张图片不能超过 5MB".into());
    }

    let dir = markdown_images_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let mut attempt = 0;
    loop {
        let timestamp = Local::now().timestamp_nanos_opt().unwrap_or_default();
        let file_name = format!(
            "todo-md-{timestamp}-{}-{attempt}.{extension}",
            std::process::id()
        );
        let path = dir.join(file_name);

        if path.exists() {
            attempt += 1;
            continue;
        }

        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        return markdown_image_url_for_path(&path).ok_or_else(|| "图片文件名无效".to_string());
    }
}
pub(crate) struct ZipEntryInput {
    pub(crate) name: String,
    pub(crate) data: Vec<u8>,
}

struct ZipCentralEntry {
    name: String,
    crc32: u32,
    size: u32,
    local_offset: u32,
}

pub(crate) fn write_zip_archive(path: &Path, entries: &[ZipEntryInput]) -> Result<(), String> {
    let mut data = Vec::<u8>::new();
    let mut central_entries = Vec::<ZipCentralEntry>::new();

    for entry in entries {
        let name = entry.name.as_bytes();
        if name.len() > u16::MAX as usize || entry.data.len() > u32::MAX as usize {
            return Err("备份文件过大".into());
        }

        let local_offset = data.len() as u32;
        let crc32 = crc32(&entry.data);
        let size = entry.data.len() as u32;

        push_u32(&mut data, 0x0403_4b50);
        push_u16(&mut data, 20);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u32(&mut data, crc32);
        push_u32(&mut data, size);
        push_u32(&mut data, size);
        push_u16(&mut data, name.len() as u16);
        push_u16(&mut data, 0);
        data.extend_from_slice(name);
        data.extend_from_slice(&entry.data);

        central_entries.push(ZipCentralEntry {
            name: entry.name.clone(),
            crc32,
            size,
            local_offset,
        });
    }

    let central_offset = data.len() as u32;
    for entry in &central_entries {
        let name = entry.name.as_bytes();
        push_u32(&mut data, 0x0201_4b50);
        push_u16(&mut data, 20);
        push_u16(&mut data, 20);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u32(&mut data, entry.crc32);
        push_u32(&mut data, entry.size);
        push_u32(&mut data, entry.size);
        push_u16(&mut data, name.len() as u16);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u16(&mut data, 0);
        push_u32(&mut data, 0);
        push_u32(&mut data, entry.local_offset);
        data.extend_from_slice(name);
    }

    let central_size = data.len() as u32 - central_offset;
    if central_entries.len() > u16::MAX as usize {
        return Err("备份条目过多".into());
    }

    push_u32(&mut data, 0x0605_4b50);
    push_u16(&mut data, 0);
    push_u16(&mut data, 0);
    push_u16(&mut data, central_entries.len() as u16);
    push_u16(&mut data, central_entries.len() as u16);
    push_u32(&mut data, central_size);
    push_u32(&mut data, central_offset);
    push_u16(&mut data, 0);

    std::fs::write(path, data).map_err(|e| e.to_string())
}

pub(crate) fn read_backup_entries(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    match read_zip_archive(bytes) {
        Ok(entries) => Ok(entries),
        Err(_) => {
            let _: TodoBackupFile = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            Ok(HashMap::from([("todos.json".to_string(), bytes.to_vec())]))
        }
    }
}

fn read_zip_archive(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let eocd = find_eocd(bytes).ok_or_else(|| "备份文件格式无效".to_string())?;
    let entry_count = read_u16(bytes, eocd + 10)? as usize;
    let central_offset = read_u32(bytes, eocd + 16)? as usize;
    let mut cursor = central_offset;
    let mut entries = HashMap::new();

    for _ in 0..entry_count {
        if read_u32(bytes, cursor)? != 0x0201_4b50 {
            return Err("备份文件目录损坏".into());
        }

        let method = read_u16(bytes, cursor + 10)?;
        let compressed_size = read_u32(bytes, cursor + 20)? as usize;
        let name_len = read_u16(bytes, cursor + 28)? as usize;
        let extra_len = read_u16(bytes, cursor + 30)? as usize;
        let comment_len = read_u16(bytes, cursor + 32)? as usize;
        let local_offset = read_u32(bytes, cursor + 42)? as usize;
        let name_start = cursor + 46;
        let name_end = name_start + name_len;
        let name = std::str::from_utf8(
            bytes
                .get(name_start..name_end)
                .ok_or_else(|| "备份文件目录损坏".to_string())?,
        )
        .map_err(|_| "备份文件目录名称无效".to_string())?
        .replace('\\', "/");

        if method != 0 {
            return Err("备份文件使用了不支持的压缩方式".into());
        }
        if !is_safe_zip_name(&name) {
            return Err("备份文件包含不安全的路径".into());
        }

        if read_u32(bytes, local_offset)? != 0x0403_4b50 {
            return Err("备份文件内容损坏".into());
        }
        let local_name_len = read_u16(bytes, local_offset + 26)? as usize;
        let local_extra_len = read_u16(bytes, local_offset + 28)? as usize;
        let data_start = local_offset + 30 + local_name_len + local_extra_len;
        let data_end = data_start + compressed_size;
        let data = bytes
            .get(data_start..data_end)
            .ok_or_else(|| "备份文件内容损坏".to_string())?
            .to_vec();
        entries.insert(name, data);
        cursor = name_end + extra_len + comment_len;
    }

    Ok(entries)
}
pub(crate) fn rewrite_markdown_images_for_backup(
    content: &str,
    markdown_dir: &Path,
    markdown_images: &mut HashMap<String, PathBuf>,
) -> String {
    let mut next = content.to_string();
    for src in markdown_image_sources(content) {
        let Some((file_name, file_path)) = markdown_image_reference(&src, markdown_dir) else {
            continue;
        };
        markdown_images.insert(file_name.clone(), file_path);
        next = next.replace(&src, &format!("markdown-images/{file_name}"));
    }
    next
}

pub(crate) fn restore_backup_markdown_image_urls(
    content: &str,
    markdown_image_urls: &HashMap<String, String>,
) -> String {
    let mut next = content.to_string();
    for (relative, url) in markdown_image_urls {
        next = next.replace(relative, url);
    }
    next
}

pub(crate) fn cleanup_unreferenced_markdown_images(app: &AppHandle, conn: &Connection) {
    let Ok(markdown_dir) = markdown_images_dir(app) else {
        return;
    };
    let mut referenced = HashSet::<String>::new();

    if let Ok(mut stmt) = conn.prepare("SELECT content FROM todos") {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for content in rows.filter_map(|row| row.ok()) {
                for src in markdown_image_sources(&content) {
                    if let Some((file_name, _)) = markdown_image_reference(&src, &markdown_dir) {
                        referenced.insert(file_name);
                    }
                }
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(&markdown_dir) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !referenced.contains(file_name) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

pub(crate) fn markdown_image_sources(content: &str) -> Vec<String> {
    let mut sources = Vec::new();
    let mut rest = content;

    while let Some(start) = rest.find("![") {
        let after_start = &rest[start + 2..];
        let Some(label_end) = after_start.find("](") else {
            break;
        };
        let src_start = start + 2 + label_end + 2;
        let after_src = &rest[src_start..];
        let Some(src_end) = after_src.find(')') else {
            break;
        };
        let src = after_src[..src_end].trim();
        if !src.is_empty() {
            sources.push(src.to_string());
        }
        rest = &after_src[src_end + 1..];
    }

    sources
}

pub(crate) fn markdown_image_reference(
    src: &str,
    markdown_dir: &Path,
) -> Option<(String, PathBuf)> {
    if let Some(file_name) = markdown_image_file_name_from_url(src) {
        return Some((file_name.clone(), markdown_dir.join(file_name)));
    }

    let decoded = decode_asset_source_path(src);
    if let Some(relative) = decoded.strip_prefix("markdown-images/") {
        let file_name = backup_markdown_image_file_name(&format!("markdown-images/{relative}"))?;
        return Some((file_name.clone(), markdown_dir.join(file_name)));
    }

    let path = PathBuf::from(&decoded);
    let file_name = path.file_name()?.to_str()?.to_string();
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return None;
    }

    let canonical_path = path.canonicalize().ok()?;
    let canonical_markdown_dir = markdown_dir.canonicalize().ok()?;
    if !canonical_path.starts_with(&canonical_markdown_dir) {
        return None;
    }

    Some((file_name, canonical_path))
}

fn decode_asset_source_path(src: &str) -> String {
    let path_part = src
        .strip_prefix("http://asset.localhost/")
        .or_else(|| src.strip_prefix("https://asset.localhost/"))
        .or_else(|| src.strip_prefix("asset://localhost/"))
        .unwrap_or(src);
    percent_decode(path_part).replace('\\', "/")
}

pub(crate) fn backup_markdown_image_file_name(name: &str) -> Option<String> {
    let rest = name.strip_prefix("markdown-images/")?;
    if rest.is_empty()
        || rest.contains('/')
        || rest.contains('\\')
        || rest.contains("..")
        || rest.contains('\0')
    {
        return None;
    }
    Some(rest.to_string())
}

pub(crate) fn unique_markdown_image_path(markdown_dir: &Path, file_name: &str) -> PathBuf {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("image");
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("png");

    for attempt in 0..10_000 {
        let candidate = if attempt == 0 {
            markdown_dir.join(format!("{stem}.{extension}"))
        } else {
            markdown_dir.join(format!("{stem}-import-{attempt}.{extension}"))
        };
        if !candidate.exists() {
            return candidate;
        }
    }

    markdown_dir.join(format!(
        "{stem}-import-{}.{extension}",
        Local::now().timestamp_nanos_opt().unwrap_or_default()
    ))
}

pub(crate) fn markdown_images_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(current_storage_dir(app)?.join("markdown-images"))
}

pub fn markdown_image_protocol_response(
    app: &AppHandle,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return empty_markdown_image_response(StatusCode::METHOD_NOT_ALLOWED);
    }

    let Some(file_name) = markdown_image_file_name_from_request_path(request.uri().path()) else {
        return empty_markdown_image_response(StatusCode::BAD_REQUEST);
    };
    let Some(content_type) = markdown_image_content_type(&file_name) else {
        return empty_markdown_image_response(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    };

    let markdown_dir = match markdown_images_dir(app) {
        Ok(dir) => dir,
        Err(_) => return empty_markdown_image_response(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let path = markdown_dir.join(&file_name);
    let canonical_path = match path.canonicalize() {
        Ok(path) => path,
        Err(_) => return empty_markdown_image_response(StatusCode::NOT_FOUND),
    };
    let canonical_markdown_dir = match markdown_dir.canonicalize() {
        Ok(path) => path,
        Err(_) => return empty_markdown_image_response(StatusCode::NOT_FOUND),
    };
    if !canonical_path.starts_with(&canonical_markdown_dir) {
        return empty_markdown_image_response(StatusCode::FORBIDDEN);
    }

    if request.method() == Method::HEAD {
        return Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, content_type)
            .body(Vec::new())
            .unwrap();
    }

    match std::fs::read(canonical_path) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, content_type)
            .header(CONTENT_LENGTH, bytes.len())
            .body(bytes)
            .unwrap(),
        Err(_) => empty_markdown_image_response(StatusCode::NOT_FOUND),
    }
}

fn empty_markdown_image_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(status).body(Vec::new()).unwrap()
}

pub(crate) fn markdown_image_url_for_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let valid_file_name = backup_markdown_image_file_name(&format!("markdown-images/{file_name}"))?;
    markdown_image_content_type(&valid_file_name)?;
    Some(markdown_image_url_for_file_name(&valid_file_name))
}

fn markdown_image_url_for_file_name(file_name: &str) -> String {
    let encoded = percent_encode(file_name);
    if cfg!(target_os = "windows") {
        format!("http://{MARKDOWN_IMAGE_PROTOCOL}.localhost/{encoded}")
    } else {
        format!("{MARKDOWN_IMAGE_PROTOCOL}://localhost/{encoded}")
    }
}

fn percent_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~') {
            output.push(*byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

fn percent_decode(value: &str) -> String {
    let mut bytes = Vec::new();
    let mut index = 0;
    let raw = value.as_bytes();
    while index < raw.len() {
        if raw[index] == b'%' && index + 2 < raw.len() {
            if let Ok(hex) = std::str::from_utf8(&raw[index + 1..index + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    bytes.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        bytes.push(raw[index]);
        index += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn markdown_image_file_name_from_request_path(path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');
    let decoded = percent_decode(path);
    backup_markdown_image_file_name(&format!("markdown-images/{decoded}"))
}

fn markdown_image_file_name_from_url(src: &str) -> Option<String> {
    let windows_prefix = format!("http://{MARKDOWN_IMAGE_PROTOCOL}.localhost/");
    let windows_https_prefix = format!("https://{MARKDOWN_IMAGE_PROTOCOL}.localhost/");
    let unix_prefix = format!("{MARKDOWN_IMAGE_PROTOCOL}://localhost/");
    let path = src
        .strip_prefix(&windows_prefix)
        .or_else(|| src.strip_prefix(&windows_https_prefix))
        .or_else(|| src.strip_prefix(&unix_prefix))?;
    let path = path
        .split_once(['?', '#'])
        .map(|(value, _)| value)
        .unwrap_or(path);
    let decoded = percent_decode(path);
    backup_markdown_image_file_name(&format!("markdown-images/{decoded}"))
}

fn markdown_image_content_type(file_name: &str) -> Option<&'static str> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

fn is_safe_zip_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.contains(':')
        && !name.contains('\0')
        && !name.split('/').any(|part| part == ".." || part.is_empty())
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    (0..=bytes.len() - 22)
        .rev()
        .find(|index| bytes.get(*index..*index + 4) == Some(&[0x50, 0x4b, 0x05, 0x06]))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "备份文件内容损坏".to_string())?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "备份文件内容损坏".to_string())?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn push_u16(data: &mut Vec<u8>, value: u16) {
    data.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(data: &mut Vec<u8>, value: u32) {
    data.extend_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}
pub(crate) fn markdown_image_extension(mime_type: &str) -> Result<&'static str, String> {
    match mime_type {
        "image/png" => Ok("png"),
        "image/jpeg" => Ok("jpg"),
        "image/webp" => Ok("webp"),
        "image/gif" => Ok("gif"),
        _ => Err("仅支持 PNG、JPEG、WebP 或 GIF 图片".into()),
    }
}
