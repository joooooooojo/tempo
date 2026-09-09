//! Host-owned file access restricted to a plugin's private data directory.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::{json, Value};

use super::bridge::{codes, RpcError, MAX_MESSAGE_BYTES};

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_LIST_ENTRIES: usize = 256;
const MAX_RELATIVE_PATH_CHARS: usize = 1_024;
const MAX_PATH_SEGMENTS: usize = 64;

pub fn dispatch(root: &Path, method: &str, params: &Value) -> Result<Value, RpcError> {
    let root = prepare_root(root)?;
    let result = match method {
        "files.readText" => read_text(&root, require_path(params, "path")?),
        "files.writeText" => write_text(
            &root,
            require_path(params, "path")?,
            require_string(params, "base64")?,
        ),
        "files.readBytes" => read_bytes(&root, require_path(params, "path")?),
        "files.writeBytes" => write_bytes(
            &root,
            require_path(params, "path")?,
            require_string(params, "base64")?,
        ),
        "files.list" => list(
            &root,
            params.get("path").and_then(Value::as_str).unwrap_or(""),
        ),
        "files.stat" => stat(&root, require_path(params, "path")?),
        "files.mkdir" => mkdir(
            &root,
            require_path(params, "path")?,
            option_bool(params, "recursive")?,
        ),
        "files.remove" => remove(
            &root,
            require_path(params, "path")?,
            option_bool(params, "recursive")?,
        ),
        "files.rename" => rename(
            &root,
            require_path(params, "from")?,
            require_path(params, "to")?,
        ),
        _ => Err(RpcError::new(
            codes::NOT_FOUND,
            format!("unknown host method: {method}"),
        )),
    }?;
    if serde_json::to_vec(&result)
        .map(|bytes| bytes.len() > MAX_MESSAGE_BYTES - 1024)
        .unwrap_or(true)
    {
        return Err(RpcError::new(
            codes::PAYLOAD_TOO_LARGE,
            "file API response exceeds the Host message limit",
        ));
    }
    Ok(result)
}

fn prepare_root(root: &Path) -> Result<PathBuf, RpcError> {
    fs::create_dir_all(root).map_err(|error| io_error("create data directory", error))?;
    root.canonicalize()
        .map_err(|error| io_error("open data directory", error))
}

fn require_path<'a>(params: &'a Value, field: &str) -> Result<&'a str, RpcError> {
    let path = require_string(params, field)?;
    if path.is_empty() {
        return Err(invalid_path("path must not be empty"));
    }
    Ok(path)
}

fn require_string<'a>(params: &'a Value, field: &str) -> Result<&'a str, RpcError> {
    params
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::new(codes::INVALID_REQUEST, format!("{field} is required")))
}

fn option_bool(params: &Value, field: &str) -> Result<bool, RpcError> {
    match params.get(field) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(RpcError::new(
            codes::INVALID_REQUEST,
            format!("{field} must be a boolean"),
        )),
    }
}

fn validate_relative(path: &str, allow_empty: bool) -> Result<Vec<&str>, RpcError> {
    if path.contains('\0') {
        return Err(invalid_path("path contains a NUL byte"));
    }
    if path.chars().count() > MAX_RELATIVE_PATH_CHARS {
        return Err(invalid_path("path is too long"));
    }
    if path.is_empty() {
        return if allow_empty {
            Ok(Vec::new())
        } else {
            Err(invalid_path("path must not be empty"))
        };
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') || path.contains(':')
    {
        return Err(invalid_path("path must be a forward-slash relative path"));
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() > MAX_PATH_SEGMENTS {
        return Err(invalid_path("path has too many segments"));
    }
    if segments.iter().any(|segment| {
        segment.is_empty()
            || *segment == "."
            || *segment == ".."
            || segment.ends_with(' ')
            || segment.ends_with('.')
    }) {
        return Err(invalid_path("path contains an invalid segment"));
    }
    Ok(segments)
}

fn resolve_path(root: &Path, path: &str, allow_empty: bool) -> Result<PathBuf, RpcError> {
    let segments = validate_relative(path, allow_empty)?;
    let mut current = root.to_path_buf();
    for segment in segments {
        let candidate = current.join(segment);
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) => {
                if is_link_like(&metadata) {
                    return Err(RpcError::new(
                        codes::FORBIDDEN,
                        "symbolic links and junctions are not allowed in plugin data",
                    ));
                }
                let canonical = candidate
                    .canonicalize()
                    .map_err(|error| io_error("resolve data path", error))?;
                if !canonical.starts_with(root) {
                    return Err(RpcError::new(
                        codes::FORBIDDEN,
                        "path escapes the plugin data directory",
                    ));
                }
                current = canonical;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => current = candidate,
            Err(error) => return Err(io_error("inspect data path", error)),
        }
    }
    if !current.starts_with(root) {
        return Err(RpcError::new(
            codes::FORBIDDEN,
            "path escapes the plugin data directory",
        ));
    }
    Ok(current)
}

fn is_link_like(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

fn read_limited(path: &Path) -> Result<Vec<u8>, RpcError> {
    let metadata = fs::metadata(path).map_err(|error| io_error("read file", error))?;
    if !metadata.is_file() {
        return Err(RpcError::new(codes::INVALID_REQUEST, "path is not a file"));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(file_too_large());
    }
    let bytes = fs::read(path).map_err(|error| io_error("read file", error))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(file_too_large());
    }
    Ok(bytes)
}

fn ensure_write_size(size: usize) -> Result<(), RpcError> {
    if size as u64 > MAX_FILE_BYTES {
        Err(file_too_large())
    } else {
        Ok(())
    }
}

fn require_parent(path: &Path) -> Result<(), RpcError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid_path("path has no parent directory"))?;
    let metadata =
        fs::metadata(parent).map_err(|error| io_error("open parent directory", error))?;
    if !metadata.is_dir() {
        return Err(RpcError::new(
            codes::INVALID_REQUEST,
            "parent path is not a directory",
        ));
    }
    Ok(())
}

fn read_text(root: &Path, relative: &str) -> Result<Value, RpcError> {
    let bytes = read_limited(&resolve_path(root, relative, false)?)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| RpcError::new(codes::INVALID_REQUEST, "file is not valid UTF-8"))?;
    Ok(json!({
        "base64": base64::engine::general_purpose::STANDARD.encode(bytes)
    }))
}

fn write_text(root: &Path, relative: &str, encoded: &str) -> Result<Value, RpcError> {
    let bytes = decode_limited(encoded)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| RpcError::new(codes::INVALID_REQUEST, "content is not valid UTF-8"))?;
    let path = resolve_path(root, relative, false)?;
    require_parent(&path)?;
    fs::write(path, bytes).map_err(|error| io_error("write file", error))?;
    Ok(Value::Null)
}

fn read_bytes(root: &Path, relative: &str) -> Result<Value, RpcError> {
    let bytes = read_limited(&resolve_path(root, relative, false)?)?;
    Ok(json!({
        "base64": base64::engine::general_purpose::STANDARD.encode(bytes)
    }))
}

fn write_bytes(root: &Path, relative: &str, encoded: &str) -> Result<Value, RpcError> {
    let bytes = decode_limited(encoded)?;
    let path = resolve_path(root, relative, false)?;
    require_parent(&path)?;
    fs::write(path, bytes).map_err(|error| io_error("write file", error))?;
    Ok(Value::Null)
}

fn decode_limited(encoded: &str) -> Result<Vec<u8>, RpcError> {
    if encoded.len() as u64 > MAX_FILE_BYTES.saturating_mul(4).div_ceil(3) + 4 {
        return Err(file_too_large());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| RpcError::new(codes::INVALID_REQUEST, "base64 is invalid"))?;
    ensure_write_size(bytes.len())?;
    Ok(bytes)
}

fn list(root: &Path, relative: &str) -> Result<Value, RpcError> {
    let directory = resolve_path(root, relative, true)?;
    if !fs::metadata(&directory)
        .map_err(|error| io_error("list directory", error))?
        .is_dir()
    {
        return Err(RpcError::new(
            codes::INVALID_REQUEST,
            "path is not a directory",
        ));
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| io_error("list directory", error))? {
        if entries.len() >= MAX_LIST_ENTRIES {
            return Err(RpcError::new(
                codes::RESOURCE_EXHAUSTED,
                "directory contains more than 256 entries",
            ));
        }
        let entry = entry.map_err(|error| io_error("list directory", error))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| io_error("inspect directory entry", error))?;
        if is_link_like(&metadata) {
            return Err(RpcError::new(
                codes::FORBIDDEN,
                "symbolic links and junctions are not allowed in plugin data",
            ));
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        entries.push(metadata_value(Some(&name), Some(&path), &metadata));
    }
    entries.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    Ok(json!({ "entries": entries }))
}

fn stat(root: &Path, relative: &str) -> Result<Value, RpcError> {
    let path = resolve_path(root, relative, false)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(json!({ "stat": null })),
        Err(error) => return Err(io_error("inspect path", error)),
    };
    if is_link_like(&metadata) {
        return Err(RpcError::new(
            codes::FORBIDDEN,
            "symbolic links and junctions are not allowed in plugin data",
        ));
    }
    Ok(json!({
        "stat": metadata_value(None, Some(relative), &metadata)
    }))
}

fn metadata_value(name: Option<&str>, path: Option<&str>, metadata: &fs::Metadata) -> Value {
    let kind = if metadata.is_file() {
        "file"
    } else if metadata.is_dir() {
        "directory"
    } else {
        "other"
    };
    let modified_at = metadata
        .modified()
        .ok()
        .map(chrono::DateTime::<chrono::Utc>::from)
        .map(|value| value.to_rfc3339());
    json!({
        "name": name,
        "path": path,
        "type": kind,
        "size": metadata.is_file().then(|| metadata.len()),
        "modifiedAt": modified_at,
    })
}

fn mkdir(root: &Path, relative: &str, recursive: bool) -> Result<Value, RpcError> {
    let path = resolve_path(root, relative, false)?;
    let result = if recursive {
        fs::create_dir_all(path)
    } else {
        fs::create_dir(path)
    };
    result.map_err(|error| io_error("create directory", error))?;
    Ok(Value::Null)
}

fn remove(root: &Path, relative: &str, recursive: bool) -> Result<Value, RpcError> {
    let path = resolve_path(root, relative, false)?;
    let metadata = fs::metadata(&path).map_err(|error| io_error("remove path", error))?;
    if metadata.is_dir() {
        if recursive {
            fs::remove_dir_all(path)
        } else {
            fs::remove_dir(path)
        }
    } else if metadata.is_file() {
        fs::remove_file(path)
    } else {
        return Err(RpcError::new(
            codes::INVALID_REQUEST,
            "path is neither a file nor a directory",
        ));
    }
    .map_err(|error| io_error("remove path", error))?;
    Ok(Value::Null)
}

fn rename(root: &Path, from: &str, to: &str) -> Result<Value, RpcError> {
    let source = resolve_path(root, from, false)?;
    fs::metadata(&source).map_err(|error| io_error("rename path", error))?;
    let destination = resolve_path(root, to, false)?;
    require_parent(&destination)?;
    if destination.exists() {
        return Err(RpcError::new(
            codes::INVALID_REQUEST,
            "destination already exists",
        ));
    }
    fs::rename(source, destination).map_err(|error| io_error("rename path", error))?;
    Ok(Value::Null)
}

fn invalid_path(message: &str) -> RpcError {
    RpcError::new(codes::INVALID_REQUEST, message)
}

fn file_too_large() -> RpcError {
    RpcError::new(
        codes::PAYLOAD_TOO_LARGE,
        "file exceeds the 512 KiB Host API limit",
    )
}

fn io_error(operation: &str, error: std::io::Error) -> RpcError {
    match error.kind() {
        ErrorKind::NotFound => RpcError::new(codes::NOT_FOUND, "path not found"),
        ErrorKind::PermissionDenied => RpcError::new(codes::FORBIDDEN, "file access denied"),
        ErrorKind::AlreadyExists | ErrorKind::InvalidInput | ErrorKind::DirectoryNotEmpty => {
            RpcError::new(codes::INVALID_REQUEST, format!("{operation} failed"))
        }
        _ => RpcError::internal(operation, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "tempo-plugin-files-{}",
            super::super::host::generate_id()
        ))
    }

    #[test]
    fn rejects_paths_that_can_escape_or_alias_the_data_root() {
        for path in [
            "",
            ".",
            "..",
            "a/../b",
            "/tmp/a",
            r"C:\temp\a",
            r"\\server\share",
            "a:b",
            r"a\b",
            "a//b",
            "a.",
        ] {
            assert!(validate_relative(path, false).is_err(), "accepted {path:?}");
        }
        assert!(validate_relative("", true).is_ok());
        assert!(validate_relative("notes/2026.txt", false).is_ok());
    }

    #[test]
    fn supports_private_data_crud_and_binary_round_trips() {
        let root = temp_root();
        let result = (|| -> Result<(), RpcError> {
            dispatch(&root, "files.mkdir", &json!({ "path": "notes" }))?;
            dispatch(
                &root,
                "files.writeText",
                &json!({
                    "path": "notes/hello.txt",
                    "base64": base64::engine::general_purpose::STANDARD.encode("hello")
                }),
            )?;
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(
                        dispatch(
                            &root,
                            "files.readText",
                            &json!({ "path": "notes/hello.txt" }),
                        )?["base64"]
                            .as_str()
                            .unwrap(),
                    )
                    .unwrap(),
                b"hello"
            );
            let encoded = base64::engine::general_purpose::STANDARD.encode([0, 1, 2, 255]);
            dispatch(
                &root,
                "files.writeBytes",
                &json!({ "path": "raw.bin", "base64": encoded }),
            )?;
            assert_eq!(
                dispatch(&root, "files.readBytes", &json!({ "path": "raw.bin" }),)?["base64"],
                encoded
            );
            let entries = dispatch(&root, "files.list", &json!({}))?;
            assert_eq!(entries["entries"].as_array().unwrap().len(), 2);
            dispatch(
                &root,
                "files.rename",
                &json!({ "from": "raw.bin", "to": "renamed.bin" }),
            )?;
            assert_eq!(
                dispatch(&root, "files.stat", &json!({ "path": "raw.bin" }))?["stat"],
                Value::Null
            );
            dispatch(
                &root,
                "files.remove",
                &json!({ "path": "notes", "recursive": true }),
            )?;
            Ok(())
        })();
        let _ = fs::remove_dir_all(&root);
        result.unwrap();
    }

    #[test]
    fn enforces_file_size_limit_before_transport_expansion() {
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("large.bin"), vec![0; MAX_FILE_BYTES as usize + 1]).unwrap();
        let error =
            dispatch(&root, "files.readBytes", &json!({ "path": "large.bin" })).unwrap_err();
        let _ = fs::remove_dir_all(&root);
        assert_eq!(error.code, codes::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn rejects_symbolic_links_inside_the_data_directory() {
        let root = temp_root();
        let outside = temp_root();
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        let link = root.join("linked.txt");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(outside.join("secret.txt"), &link);
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(outside.join("secret.txt"), &link);
        if linked.is_ok() {
            let error =
                dispatch(&root, "files.readText", &json!({ "path": "linked.txt" })).unwrap_err();
            assert_eq!(error.code, codes::FORBIDDEN);
        }
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }
}
