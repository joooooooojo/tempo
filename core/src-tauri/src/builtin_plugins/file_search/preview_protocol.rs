use crate::asset_protocol::{percent_decode, percent_encode};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tauri::http::{
    header::{
        ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
        ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_MAX_AGE, CONTENT_LENGTH, CONTENT_RANGE,
        CONTENT_TYPE, RANGE,
    },
    HeaderMap, HeaderValue, Method, Request, Response, StatusCode,
};

pub const FILE_PREVIEW_PROTOCOL: &str = "tempo-file-preview";

/// Build a WebView URL for an absolute filesystem path (no base64).
///
/// The full absolute path (including drive letters, backslashes, spaces, and
/// non-ASCII) is percent-encoded so Chinese / spaced Windows paths survive the
/// custom-protocol URL round-trip.
pub fn preview_url_for_path(path: &str) -> String {
    let encoded = percent_encode(path);
    if cfg!(windows) {
        // WebView2 treats custom schemes as `http://{scheme}.localhost/...`.
        format!("http://{FILE_PREVIEW_PROTOCOL}.localhost/{encoded}")
    } else {
        format!("{FILE_PREVIEW_PROTOCOL}://localhost/{encoded}")
    }
}

pub fn file_preview_protocol_response(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    // Dev/prod pages are cross-origin vs `tempo-file-preview.localhost`.
    // `<img>`/`<video>` load without CORS; Office/text `fetch()` needs ACAO.
    if request.method() == Method::OPTIONS {
        return cors_preflight();
    }

    if request.method() != Method::GET && request.method() != Method::HEAD {
        return empty(StatusCode::METHOD_NOT_ALLOWED);
    }

    let Some(path) = path_from_request(request.uri().path()) else {
        return empty(StatusCode::BAD_REQUEST);
    };
    if !path.is_absolute() || !path.exists() || !path.is_file() {
        return empty(StatusCode::NOT_FOUND);
    }

    let Ok(meta) = std::fs::metadata(&path) else {
        return empty(StatusCode::NOT_FOUND);
    };
    let len = meta.len();
    let content_type = content_type_for_path(&path);

    if request.method() == Method::HEAD {
        return with_cors(
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, content_type)
                .header(CONTENT_LENGTH, len)
                .body(Vec::new())
                .unwrap(),
        );
    }

    if let Some((start, end)) = parse_byte_range(request.headers(), len) {
        return read_range_response(&path, content_type, len, start, end);
    }

    // Full body (images / office / small media). Large videos should use Range.
    match std::fs::read(&path) {
        Ok(bytes) => with_cors(
            Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, content_type)
                .header(CONTENT_LENGTH, bytes.len())
                .body(bytes)
                .unwrap(),
        ),
        Err(_) => empty(StatusCode::NOT_FOUND),
    }
}

fn path_from_request(uri_path: &str) -> Option<PathBuf> {
    let trimmed = uri_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let decoded = percent_decode(trimmed);
    if decoded.is_empty() {
        return None;
    }
    // Reject path traversal fragments after decode.
    if decoded.split(['/', '\\']).any(|part| part == "..") {
        return None;
    }
    Some(PathBuf::from(decoded))
}

fn content_type_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "aac" | "m4a" => "audio/mp4",
        "ogg" => "audio/ogg",
        "wma" => "audio/x-ms-wma",
        "aiff" | "aif" => "audio/aiff",
        "xlsx" | "xlsm" | "xlsb" => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        }
        "xls" => "application/vnd.ms-excel",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "docx" | "docm" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        }
        "doc" => "application/msword",
        "rtf" => "application/rtf",
        "odt" => "application/vnd.oasis.opendocument.text",
        "pptx" | "pptm" => {
            "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        }
        "ppt" => "application/vnd.ms-powerpoint",
        "odp" => "application/vnd.oasis.opendocument.presentation",
        "pdf" => "application/pdf",
        // Text / source — broad allowlist for preview protocol
        "html" | "htm" | "xhtml" => "text/html; charset=utf-8",
        "css" | "scss" | "sass" | "less" | "styl" => "text/css; charset=utf-8",
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => {
            "text/javascript; charset=utf-8"
        }
        "json" | "jsonc" | "json5" => "application/json; charset=utf-8",
        "xml" | "xsl" | "xsd" => "application/xml; charset=utf-8",
        "md" | "markdown" => "text/markdown; charset=utf-8",
        "yaml" | "yml" => "text/yaml; charset=utf-8",
        "toml" | "ini" | "cfg" | "conf" | "config" | "env" | "properties" | "txt"
        | "text" | "log" | "rst" | "adoc" | "asciidoc" | "sh" | "bash" | "zsh"
        | "fish" | "ps1" | "psm1" | "psd1" | "bat" | "cmd" | "rs" | "go" | "py"
        | "pyi" | "pyw" | "rb" | "php" | "java" | "kt" | "kts" | "swift" | "scala"
        | "cs" | "fs" | "fsx" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hxx"
        | "m" | "mm" | "vue" | "svelte" | "astro" | "lua" | "r" | "pl" | "pm"
        | "tcl" | "groovy" | "gradle" | "cmake" | "sql" | "graphql" | "gql"
        | "proto" | "dart" | "ex" | "exs" | "erl" | "hrl" | "clj" | "cljs"
        | "edn" | "hs" | "elm" | "zig" | "nim" | "v" | "vb" | "vbs" | "diff"
        | "patch" | "nfo" | "srt" | "vtt" | "ass" | "ssa" | "tex" | "bib"
        | "dockerfile" | "makefile" | "mk" | "lock" | "editorconfig"
        | "gitignore" | "gitattributes" | "dockerignore" | "npmrc" | "yarnrc" => {
            "text/plain; charset=utf-8"
        }
        _ => "application/octet-stream",
    }
}

fn parse_byte_range(headers: &HeaderMap, len: u64) -> Option<(u64, u64)> {
    let value = headers.get(RANGE)?.to_str().ok()?;
    let value = value.strip_prefix("bytes=")?;
    let (start_raw, end_raw) = value.split_once('-')?;
    let start = if start_raw.is_empty() {
        0
    } else {
        start_raw.parse().ok()?
    };
    let end = if end_raw.is_empty() {
        len.saturating_sub(1)
    } else {
        end_raw.parse::<u64>().ok()?.min(len.saturating_sub(1))
    };
    if start > end || start >= len {
        return None;
    }
    Some((start, end))
}

fn read_range_response(
    path: &Path,
    content_type: &'static str,
    len: u64,
    start: u64,
    end: u64,
) -> Response<Vec<u8>> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return empty(StatusCode::NOT_FOUND);
    };
    if file.seek(SeekFrom::Start(start)).is_err() {
        return empty(StatusCode::NOT_FOUND);
    }
    let take = (end - start + 1) as usize;
    let mut buf = vec![0_u8; take];
    let Ok(read) = file.read(&mut buf) else {
        return empty(StatusCode::NOT_FOUND);
    };
    buf.truncate(read);
    with_cors(
        Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(CONTENT_TYPE, content_type)
            .header(CONTENT_LENGTH, buf.len())
            .header(CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
            .body(buf)
            .unwrap(),
    )
}

fn cors_preflight() -> Response<Vec<u8>> {
    with_cors(
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
            .header(ACCESS_CONTROL_ALLOW_HEADERS, "Range, Content-Type")
            .header(ACCESS_CONTROL_MAX_AGE, "86400")
            .body(Vec::new())
            .unwrap(),
    )
}

fn with_cors(mut response: Response<Vec<u8>>) -> Response<Vec<u8>> {
    let headers = response.headers_mut();
    headers.insert(
        ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Content-Length, Content-Range, Content-Type, Accept-Ranges"),
    );
    response
}

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    with_cors(
        Response::builder()
            .status(status)
            .body(Vec::new())
            .unwrap(),
    )
}
