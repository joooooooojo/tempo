use crate::db::current_storage_dir;
use std::path::Path;
use tauri::http::{
    header::{CONTENT_LENGTH, CONTENT_TYPE},
    Method, Request, Response, StatusCode,
};
use tauri::AppHandle;

pub fn asset_dir(app: &AppHandle, subdir: &str) -> Result<std::path::PathBuf, String> {
    Ok(current_storage_dir(app)?.join(subdir))
}

pub fn asset_url_for_file_name(protocol: &str, file_name: &str) -> String {
    let encoded = percent_encode(file_name);
    if cfg!(target_os = "windows") {
        format!("http://{protocol}.localhost/{encoded}")
    } else {
        format!("{protocol}://localhost/{encoded}")
    }
}

pub fn asset_protocol_response(
    app: &AppHandle,
    subdir: &str,
    resolve_content_type: fn(&str) -> &'static str,
    validate_file_name: fn(&str) -> bool,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return empty_response(StatusCode::METHOD_NOT_ALLOWED);
    }

    let Some(file_name) =
        asset_file_name_from_request_path(request.uri().path(), validate_file_name)
    else {
        return empty_response(StatusCode::BAD_REQUEST);
    };

    let asset_dir = match asset_dir(app, subdir) {
        Ok(dir) => dir,
        Err(_) => return empty_response(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let path = asset_dir.join(&file_name);
    let canonical_path = match path.canonicalize() {
        Ok(path) => path,
        Err(_) => return empty_response(StatusCode::NOT_FOUND),
    };
    let canonical_asset_dir = match asset_dir.canonicalize() {
        Ok(path) => path,
        Err(_) => return empty_response(StatusCode::NOT_FOUND),
    };
    if !canonical_path.starts_with(&canonical_asset_dir) {
        return empty_response(StatusCode::FORBIDDEN);
    }

    let content_type = resolve_content_type(&file_name);

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
        Err(_) => empty_response(StatusCode::NOT_FOUND),
    }
}

pub fn storage_key_from_protocol_url(protocol: &str, subdir: &str, value: &str) -> Option<String> {
    let file_name = protocol_file_name(protocol, value)?;
    if !Path::new(&file_name)
        .file_name()
        .is_some_and(|name| name == file_name.as_str())
    {
        return None;
    }
    Some(format!("{subdir}/{file_name}"))
}

pub fn protocol_file_name(protocol: &str, value: &str) -> Option<String> {
    let windows_prefix = format!("http://{protocol}.localhost/");
    let unix_prefix = format!("{protocol}://localhost/");
    let path = value
        .strip_prefix(&windows_prefix)
        .or_else(|| value.strip_prefix(&unix_prefix))?;
    let decoded = percent_decode(path.trim_start_matches('/'));
    if decoded.is_empty() {
        return None;
    }
    Some(decoded)
}

fn asset_file_name_from_request_path(
    path: &str,
    validate_file_name: fn(&str) -> bool,
) -> Option<String> {
    let path = path.trim_start_matches('/');
    let decoded = percent_decode(path);
    validate_file_name(&decoded).then_some(decoded)
}

fn empty_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(status).body(Vec::new()).unwrap()
}

pub fn percent_encode(value: &str) -> String {
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

pub fn percent_decode(value: &str) -> String {
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
