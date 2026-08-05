use crate::db::{current_storage_dir, default_storage_dir};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchStatus {
    pub ready: bool,
    pub engine: Option<String>,
    pub version: Option<String>,
    pub message: Option<String>,
    /// True while Everything is loading/rebuilding its database (Windows).
    #[serde(default)]
    pub indexing: bool,
    /// Human-readable indexing phase for the progress banner.
    #[serde(default)]
    pub indexing_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchQuery {
    pub query: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_category() -> String {
    "all".into()
}

fn default_sort() -> String {
    "mtime_desc".into()
}

fn default_limit() -> u32 {
    100
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified_at: Option<String>,
    pub extension: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchQueryResult {
    pub items: Vec<FileSearchItem>,
    pub total: u32,
    /// Whether more results exist beyond this page (`offset + items.len()`).
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchPreviewMeta {
    pub path: String,
    pub name: String,
    pub size: Option<u64>,
    pub modified_at: Option<String>,
    pub is_dir: bool,
    pub extension: Option<String>,
    pub mime_hint: Option<String>,
    /// `"image" | "video" | "audio" | "text" | "excel" | "word" | "ppt" | "archive" | "none"`
    pub preview_kind: String,
}

const ARCHIVE_LIST_MAX: usize = 800;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchArchiveEntry {
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub compressed_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchArchiveListing {
    pub format: String,
    pub entries: Vec<FileSearchArchiveEntry>,
    pub total_entries: usize,
    pub truncated: bool,
    pub message: Option<String>,
}

pub(crate) fn tools_root(app: &AppHandle) -> Result<PathBuf, String> {
    let base = current_storage_dir(app).or_else(|_| default_storage_dir(app))?;
    let dir = base.join("tools");
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建工具目录: {error}"))?;
    Ok(dir)
}

pub(crate) fn category_extensions(category: &str) -> Option<&'static [&'static str]> {
    match category {
        "excel" => Some(&["xlsx", "xls", "xlsm", "xlsb", "csv"]),
        "word" => Some(&["doc", "docx", "docm", "rtf", "odt"]),
        "ppt" => Some(&["ppt", "pptx", "pptm", "odp"]),
        "pdf" => Some(&["pdf"]),
        "image" => Some(&[
            "jpg", "jpeg", "png", "gif", "bmp", "webp", "ico", "svg", "heic", "tif", "tiff",
        ]),
        "video" => Some(&["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v"]),
        "audio" => Some(&["mp3", "wav", "flac", "aac", "m4a", "ogg", "wma", "aiff"]),
        "archive" => Some(&[
            "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "tgz", "jar", "apk", "whl", "war",
            "ear",
        ]),
        _ => None,
    }
}

/// Whether an item belongs to the given search category (for client-side filtering).
pub(crate) fn item_matches_category(item: &FileSearchItem, category: &str) -> bool {
    match category {
        "all" => true,
        "folder" => item.is_dir,
        other => {
            let Some(exts) = category_extensions(other) else {
                return true;
            };
            if item.is_dir {
                return false;
            }
            item.extension
                .as_ref()
                .map(|ext| exts.iter().any(|allowed| allowed.eq_ignore_ascii_case(ext)))
                .unwrap_or(false)
        }
    }
}

pub(crate) fn item_from_path(path: &Path) -> Option<FileSearchItem> {
    let meta = std::fs::metadata(path).ok();
    let is_dir = meta
        .as_ref()
        .map(|value| value.is_dir())
        .unwrap_or_else(|| path.is_dir());
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    if name.is_empty() {
        return None;
    }
    let extension = if is_dir {
        None
    } else {
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
    };
    let size = meta.as_ref().and_then(|value| {
        if value.is_file() {
            Some(value.len())
        } else {
            None
        }
    });
    let modified_at = meta
        .as_ref()
        .and_then(|value| value.modified().ok())
        .and_then(system_time_to_rfc3339);
    Some(FileSearchItem {
        name,
        path: path.to_string_lossy().into_owned(),
        is_dir,
        size,
        modified_at,
        extension,
    })
}

pub(crate) fn system_time_to_rfc3339(value: SystemTime) -> Option<String> {
    let datetime: chrono::DateTime<chrono::Local> = value.into();
    Some(datetime.to_rfc3339())
}

pub(crate) fn sort_items(items: &mut [FileSearchItem], sort: &str) {
    match sort {
        "mtime_asc" => items.sort_by(|left, right| {
            left.modified_at
                .cmp(&right.modified_at)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        }),
        "name_asc" => items.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.path.cmp(&right.path))
        }),
        "name_desc" => items.sort_by(|left, right| {
            right
                .name
                .to_lowercase()
                .cmp(&left.name.to_lowercase())
                .then_with(|| right.path.cmp(&left.path))
        }),
        "size_desc" => items.sort_by(|left, right| {
            right
                .size
                .unwrap_or(0)
                .cmp(&left.size.unwrap_or(0))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        }),
        "size_asc" => items.sort_by(|left, right| {
            left.size
                .unwrap_or(0)
                .cmp(&right.size.unwrap_or(0))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        }),
        _ => items.sort_by(|left, right| {
            right
                .modified_at
                .cmp(&left.modified_at)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        }),
    }
}

pub(crate) fn is_image_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "ico" | "svg"
    )
}

pub(crate) fn is_video_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v"
    )
}

pub(crate) fn is_audio_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "wma" | "aiff"
    )
}

pub(crate) fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        // plain / docs / logs
        "txt"
            | "text"
            | "log"
            | "md"
            | "markdown"
            | "rst"
            | "adoc"
            | "asciidoc"
            // data / markup
            | "json"
            | "jsonc"
            | "json5"
            | "yaml"
            | "yml"
            | "toml"
            | "xml"
            | "xsl"
            | "xsd"
            | "html"
            | "htm"
            | "xhtml"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "styl"
            | "svgz"
            // config / env
            | "ini"
            | "cfg"
            | "conf"
            | "config"
            | "env"
            | "properties"
            | "editorconfig"
            | "gitignore"
            | "gitattributes"
            | "dockerignore"
            | "npmrc"
            | "yarnrc"
            | "lock"
            // shell / scripts
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "ps1"
            | "psm1"
            | "psd1"
            | "bat"
            | "cmd"
            // languages
            | "rs"
            | "go"
            | "py"
            | "pyi"
            | "pyw"
            | "rb"
            | "php"
            | "java"
            | "kt"
            | "kts"
            | "swift"
            | "scala"
            | "cs"
            | "fs"
            | "fsx"
            | "c"
            | "h"
            | "cc"
            | "cpp"
            | "cxx"
            | "hpp"
            | "hxx"
            | "m"
            | "mm"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "ts"
            | "tsx"
            | "mts"
            | "cts"
            | "vue"
            | "svelte"
            | "astro"
            | "lua"
            | "r"
            | "pl"
            | "pm"
            | "tcl"
            | "groovy"
            | "gradle"
            | "cmake"
            | "sql"
            | "graphql"
            | "gql"
            | "proto"
            | "dart"
            | "ex"
            | "exs"
            | "erl"
            | "hrl"
            | "clj"
            | "cljs"
            | "edn"
            | "hs"
            | "elm"
            | "zig"
            | "nim"
            | "v"
            | "vb"
            | "vbs"
            // diffs / misc text
            | "diff"
            | "patch"
            | "csv"
            | "csvt"
            | "tsv"
            | "nfo"
            | "srt"
            | "vtt"
            | "ass"
            | "ssa"
            | "tex"
            | "bib"
            | "dockerfile"
            | "makefile"
            | "mk"
            | "cmakein"
    )
}

pub(crate) fn is_excel_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "xlsx" | "xls" | "xlsm" | "xlsb"
    )
}

pub(crate) fn is_word_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "doc" | "docx" | "docm" | "rtf" | "odt"
    )
}

pub(crate) fn is_ppt_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "ppt" | "pptx" | "pptm" | "odp"
    )
}

pub(crate) fn is_archive_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "zip"
            | "rar"
            | "7z"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "tgz"
            | "jar"
            | "apk"
            | "whl"
            | "war"
            | "ear"
            | "tbz"
            | "tbz2"
            | "txz"
    )
}

/// Classify a file for frontend preview (no bytes; path + kind only).
pub(crate) fn preview_kind_for(is_dir: bool, extension: &str) -> &'static str {
    if is_dir || extension.is_empty() {
        return "none";
    }
    if is_image_extension(extension) {
        "image"
    } else if is_video_extension(extension) {
        "video"
    } else if is_audio_extension(extension) {
        "audio"
    } else if is_excel_extension(extension) {
        "excel"
    } else if is_word_extension(extension) {
        "word"
    } else if is_ppt_extension(extension) {
        "ppt"
    } else if is_text_extension(extension) {
        "text"
    } else if is_archive_extension(extension) {
        "archive"
    } else {
        "none"
    }
}

fn mime_hint_for(is_dir: bool, extension: &str, kind: &str) -> Option<String> {
    if is_dir {
        return Some("inode/directory".into());
    }
    match kind {
        "image" => Some(format!(
            "image/{}",
            if extension.eq_ignore_ascii_case("jpg") {
                "jpeg"
            } else if extension.eq_ignore_ascii_case("svg") {
                "svg+xml"
            } else {
                extension
            }
        )),
        "video" => Some(format!("video/{extension}")),
        "audio" => Some(format!("audio/{extension}")),
        "text" => Some(match extension.to_ascii_lowercase().as_str() {
            "html" | "htm" | "xhtml" => "text/html".into(),
            "css" | "scss" | "sass" | "less" | "styl" => "text/css".into(),
            "js" | "jsx" | "mjs" | "cjs" => "text/javascript".into(),
            "json" | "jsonc" | "json5" => "application/json".into(),
            "xml" | "xsl" | "xsd" => "application/xml".into(),
            "md" | "markdown" => "text/markdown".into(),
            "yaml" | "yml" => "text/yaml".into(),
            "tsv" => "text/tab-separated-values".into(),
            "csv" => "text/csv".into(),
            _ => "text/plain".into(),
        }),
        "excel" => Some("application/vnd.ms-excel".into()),
        "word" => Some("application/msword".into()),
        "ppt" => Some("application/vnd.ms-powerpoint".into()),
        "archive" => Some(match extension.to_ascii_lowercase().as_str() {
            "zip" | "jar" | "apk" | "whl" | "war" | "ear" => "application/zip".into(),
            "rar" => "application/vnd.rar".into(),
            "7z" => "application/x-7z-compressed".into(),
            "tar" => "application/x-tar".into(),
            "gz" | "tgz" => "application/gzip".into(),
            "bz2" | "tbz" | "tbz2" => "application/x-bzip2".into(),
            "xz" | "txz" => "application/x-xz".into(),
            _ => "application/octet-stream".into(),
        }),
        _ => None,
    }
}

#[tauri::command]
pub async fn file_search_status(app: AppHandle) -> Result<FileSearchStatus, String> {
    tauri::async_runtime::spawn_blocking(move || status_sync(&app))
        .await
        .map_err(|error| format!("检查搜索引擎状态失败: {error}"))?
}

#[tauri::command]
pub async fn file_search_ensure_engine(app: AppHandle) -> Result<FileSearchStatus, String> {
    tauri::async_runtime::spawn_blocking(move || ensure_engine_sync(&app))
        .await
        .map_err(|error| format!("准备搜索引擎失败: {error}"))?
}

#[tauri::command]
pub async fn file_search_query(
    app: AppHandle,
    request: FileSearchQuery,
) -> Result<FileSearchQueryResult, String> {
    tauri::async_runtime::spawn_blocking(move || query_sync(&app, request))
        .await
        .map_err(|error| format!("文件搜索失败: {error}"))?
}

#[tauri::command]
pub fn file_search_open(app: AppHandle, path: String) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("路径无效".into());
    }
    if !Path::new(path).exists() {
        return Err(format!("路径不存在：{path}"));
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path.to_string(), None::<String>)
        .map_err(|error| format!("无法打开：{error}"))
}

#[tauri::command]
pub fn file_search_reveal(app: AppHandle, path: String) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("路径无效".into());
    }
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() {
        return Err(format!("路径不存在：{path}"));
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&path_buf)
        .map_err(|error| format!("无法打开位置：{error}"))
}

#[tauri::command]
pub fn file_search_preview_url(path: String) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("路径无效".into());
    }
    let path_buf = PathBuf::from(path);
    if !path_buf.is_absolute() {
        return Err("预览仅支持绝对路径".into());
    }
    if !path_buf.is_file() {
        return Err(format!("文件不存在：{path}"));
    }
    Ok(crate::builtin_plugins::file_search::preview_url_for_path(path))
}

#[tauri::command]
pub async fn file_search_preview_meta(path: String) -> Result<FileSearchPreviewMeta, String> {
    tauri::async_runtime::spawn_blocking(move || preview_meta_sync(&path))
        .await
        .map_err(|error| format!("读取预览信息失败: {error}"))?
}

fn status_sync(app: &AppHandle) -> Result<FileSearchStatus, String> {
    #[cfg(target_os = "windows")]
    {
        return crate::builtin_plugins::file_search::backend_windows::status(app);
    }
    #[cfg(target_os = "macos")]
    {
        return crate::builtin_plugins::file_search::backend_macos::status(app);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = app;
        Ok(FileSearchStatus {
            ready: false,
            engine: None,
            version: None,
            message: Some("当前平台暂不支持文件搜索".into()),
            indexing: false,
            indexing_message: None,
        })
    }
}

fn ensure_engine_sync(app: &AppHandle) -> Result<FileSearchStatus, String> {
    #[cfg(target_os = "windows")]
    {
        return crate::builtin_plugins::file_search::backend_windows::ensure_engine(app);
    }
    #[cfg(target_os = "macos")]
    {
        return crate::builtin_plugins::file_search::backend_macos::ensure_engine(app);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        status_sync(app)
    }
}

fn query_sync(app: &AppHandle, request: FileSearchQuery) -> Result<FileSearchQueryResult, String> {
    #[cfg(target_os = "windows")]
    {
        return crate::builtin_plugins::file_search::backend_windows::query(app, request);
    }
    #[cfg(target_os = "macos")]
    {
        return crate::builtin_plugins::file_search::backend_macos::query(app, request);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (app, request);
        Err("当前平台暂不支持文件搜索".into())
    }
}

fn preview_meta_sync(path: &str) -> Result<FileSearchPreviewMeta, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("路径无效".into());
    }
    let path_buf = PathBuf::from(path);
    let item = item_from_path(&path_buf).ok_or_else(|| "无法读取文件信息".to_string())?;
    let extension = item.extension.clone().unwrap_or_default();
    let preview_kind = preview_kind_for(item.is_dir, &extension).to_string();
    let mime_hint = mime_hint_for(item.is_dir, &extension, &preview_kind);
    Ok(FileSearchPreviewMeta {
        path: item.path,
        name: item.name,
        size: item.size,
        modified_at: item.modified_at,
        is_dir: item.is_dir,
        extension: item.extension,
        mime_hint,
        preview_kind,
    })
}

/// Detect archive format from path (handles compound suffixes like `.tar.gz`).
fn archive_format_for(path: &Path) -> Option<&'static str> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Some("tar.gz");
    }
    if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tbz") {
        return Some("tar.bz2");
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        return Some("tar.xz");
    }
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "zip" | "jar" | "apk" | "whl" | "war" | "ear" => Some("zip"),
        "tar" => Some("tar"),
        "gz" => Some("gz"),
        "tgz" => Some("tar.gz"),
        "7z" => Some("7z"),
        "rar" => Some("rar"),
        "bz2" => Some("bz2"),
        "xz" => Some("xz"),
        _ => None,
    }
}

fn unsupported_archive_listing(format: &str) -> FileSearchArchiveListing {
    let label = match format {
        "rar" => "RAR",
        "7z" => "7z",
        "bz2" | "tar.bz2" => "bzip2",
        "xz" | "tar.xz" => "xz",
        other => other,
    };
    FileSearchArchiveListing {
        format: format.to_string(),
        entries: Vec::new(),
        total_entries: 0,
        truncated: false,
        message: Some(format!(
            "暂不支持 {label} 目录预览（当前支持 ZIP / JAR / APK / WHL / TAR / TAR.GZ）"
        )),
    }
}

fn list_zip_archive(path: &Path) -> Result<FileSearchArchiveListing, String> {
    let file = File::open(path).map_err(|error| format!("打开压缩包失败: {error}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        let msg = error.to_string();
        if msg.to_ascii_lowercase().contains("password")
            || msg.to_ascii_lowercase().contains("encrypt")
        {
            format!("压缩包受密码保护，无法预览目录: {msg}")
        } else {
            format!("无法解析压缩包（可能已损坏）: {msg}")
        }
    })?;

    let total_entries = archive.len();
    let mut entries = Vec::with_capacity(total_entries.min(ARCHIVE_LIST_MAX));
    let mut encrypted = false;
    let limit = total_entries.min(ARCHIVE_LIST_MAX);

    for index in 0..limit {
        let entry = archive.by_index(index).map_err(|error| {
            let msg = error.to_string();
            if msg.to_ascii_lowercase().contains("password")
                || msg.to_ascii_lowercase().contains("encrypt")
            {
                format!("压缩包受密码保护，无法预览目录: {msg}")
            } else {
                format!("读取压缩条目失败: {msg}")
            }
        })?;
        if entry.encrypted() {
            encrypted = true;
        }
        let name = entry.name().replace('\\', "/");
        let is_dir = entry.is_dir() || name.ends_with('/');
        entries.push(FileSearchArchiveEntry {
            path: name.trim_end_matches('/').to_string(),
            is_dir,
            size: if is_dir {
                None
            } else {
                Some(entry.size())
            },
            compressed_size: if is_dir {
                None
            } else {
                Some(entry.compressed_size())
            },
        });
    }

    let truncated = total_entries > ARCHIVE_LIST_MAX;
    let mut message = None;
    if encrypted {
        message = Some("部分条目受密码保护，已尽量列出目录名".into());
    }
    if truncated {
        let note = format!("仅显示前 {ARCHIVE_LIST_MAX} 项（共 {total_entries} 项）");
        message = Some(match message {
            Some(prev) => format!("{prev}；{note}"),
            None => note,
        });
    }

    Ok(FileSearchArchiveListing {
        format: "zip".into(),
        entries,
        total_entries,
        truncated,
        message,
    })
}

fn push_tar_entries<R: Read>(
    archive: &mut tar::Archive<R>,
    entries: &mut Vec<FileSearchArchiveEntry>,
) -> Result<(usize, bool), String> {
    let mut total = 0usize;
    let mut truncated = false;
    for entry in archive
        .entries()
        .map_err(|error| format!("读取 tar 条目失败: {error}"))?
    {
        let entry = entry.map_err(|error| format!("读取 tar 条目失败: {error}"))?;
        total += 1;
        if entries.len() >= ARCHIVE_LIST_MAX {
            truncated = true;
            continue;
        }
        let path = entry
            .path()
            .map_err(|error| format!("解析 tar 路径失败: {error}"))?
            .to_string_lossy()
            .replace('\\', "/");
        let is_dir = entry.header().entry_type().is_dir() || path.ends_with('/');
        let size = entry.header().size().ok();
        entries.push(FileSearchArchiveEntry {
            path: path.trim_end_matches('/').to_string(),
            is_dir,
            size: if is_dir { None } else { size },
            compressed_size: None,
        });
    }
    Ok((total, truncated))
}

fn list_tar_archive(path: &Path, gzip: bool) -> Result<FileSearchArchiveListing, String> {
    let file = File::open(path).map_err(|error| format!("打开压缩包失败: {error}"))?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    let (total_entries, truncated) = if gzip {
        let decoder = flate2::read::GzDecoder::new(reader);
        let mut archive = tar::Archive::new(decoder);
        push_tar_entries(&mut archive, &mut entries)?
    } else {
        let mut archive = tar::Archive::new(reader);
        push_tar_entries(&mut archive, &mut entries)?
    };

    let message = if truncated {
        Some(format!(
            "仅显示前 {ARCHIVE_LIST_MAX} 项（共 {total_entries} 项）"
        ))
    } else {
        None
    };

    Ok(FileSearchArchiveListing {
        format: if gzip { "tar.gz".into() } else { "tar".into() },
        entries,
        total_entries,
        truncated,
        message,
    })
}

fn list_gzip_single(path: &Path) -> Result<FileSearchArchiveListing, String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file")
        .to_string();
    let inner = if name.to_ascii_lowercase().ends_with(".gz") && name.len() > 3 {
        name[..name.len() - 3].to_string()
    } else {
        name.clone()
    };
    let meta_size = std::fs::metadata(path).ok().map(|m| m.len());
    Ok(FileSearchArchiveListing {
        format: "gz".into(),
        entries: vec![FileSearchArchiveEntry {
            path: inner,
            is_dir: false,
            size: None,
            compressed_size: meta_size,
        }],
        total_entries: 1,
        truncated: false,
        message: Some("Gzip 单文件压缩，已显示内部文件名".into()),
    })
}

fn list_archive_sync(path: &str) -> Result<FileSearchArchiveListing, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("路径无效".into());
    }
    let path_buf = PathBuf::from(path);
    if !path_buf.is_file() {
        return Err(format!("文件不存在：{path}"));
    }
    let Some(format) = archive_format_for(&path_buf) else {
        return Err("不是可识别的压缩文件".into());
    };

    match format {
        "zip" => list_zip_archive(&path_buf),
        "tar" => list_tar_archive(&path_buf, false),
        "tar.gz" => list_tar_archive(&path_buf, true),
        "gz" => list_gzip_single(&path_buf),
        other => Ok(unsupported_archive_listing(other)),
    }
}

#[tauri::command]
pub async fn file_search_list_archive(path: String) -> Result<FileSearchArchiveListing, String> {
    tauri::async_runtime::spawn_blocking(move || list_archive_sync(&path))
        .await
        .map_err(|error| format!("读取压缩包目录失败: {error}"))?
}
