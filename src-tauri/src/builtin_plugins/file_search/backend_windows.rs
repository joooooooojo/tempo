use super::commands::{
    category_extensions, item_from_path, tools_root, FileSearchItem, FileSearchQuery,
    FileSearchQueryResult, FileSearchStatus,
};
use super::tools::{download_file, emit_engine_progress, extract_zip, find_file_named};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tauri::AppHandle;

const EVERYTHING_ZIP_URL: &str = "https://www.voidtools.com/Everything-1.4.1.1032.x64.zip";
const ES_ZIP_URL: &str = "https://www.voidtools.com/ES-1.1.0.30.x64.zip";
const EVERYTHING_VERSION: &str = "1.4.1.1032";
const QUERY_TIMEOUT_SECS: u64 = 12;

pub(crate) fn status(app: &AppHandle) -> Result<FileSearchStatus, String> {
    // IPC queries do not need ES.exe — only Everything itself.
    match resolve_runtime(app, false)? {
        Some(runtime) => {
            let index = everything_index_state();
            let indexing = index.indexing();
            Ok(FileSearchStatus {
                ready: true,
                engine: Some("everything".into()),
                version: Some(runtime.version),
                message: Some(if indexing {
                    index
                        .message()
                        .unwrap_or_else(|| "正在建立索引…".into())
                } else if runtime.portable {
                    "已就绪（便携 Everything）".into()
                } else {
                    "已就绪（系统 Everything）".into()
                }),
                indexing,
                indexing_message: index.message(),
            })
        }
        None => Ok(FileSearchStatus {
            ready: false,
            engine: Some("everything".into()),
            version: None,
            message: Some("未检测到 Everything，可下载便携版以启用全盘搜索".into()),
            indexing: false,
            indexing_message: None,
        }),
    }
}

pub(crate) fn ensure_engine(app: &AppHandle) -> Result<FileSearchStatus, String> {
    if let Some(runtime) = resolve_runtime(app, false)? {
        // ES is optional fallback only; IPC is the primary query path.
        if !runtime.es_path.exists() {
            let _ = ensure_es_installed(app);
        }
        let runtime = resolve_runtime(app, false)?
            .ok_or_else(|| "已检测到 Everything，但未能定位可执行文件".to_string())?;
        ensure_everything_running(app, &runtime, true)?;
        emit_engine_progress(app, "done", 0, None, Some(100.0), Some("完成"));
        return Ok(with_index_fields(FileSearchStatus {
            ready: true,
            engine: Some("everything".into()),
            version: Some(runtime.version),
            message: Some("Everything 已就绪".into()),
            indexing: false,
            indexing_message: None,
        }));
    }
    install_portable(app)?;
    let runtime = resolve_runtime(app, false)?
        .ok_or_else(|| "便携 Everything 安装后仍无法定位可执行文件".to_string())?;
    ensure_everything_running(app, &runtime, true)?;
    // Give IPC a moment after first launch.
    std::thread::sleep(Duration::from_millis(800));
    emit_engine_progress(app, "done", 0, None, Some(100.0), Some("完成"));
    Ok(with_index_fields(FileSearchStatus {
        ready: true,
        engine: Some("everything".into()),
        version: Some(runtime.version),
        message: Some("便携 Everything 已下载并启动".into()),
        indexing: false,
        indexing_message: None,
    }))
}

pub(crate) fn query(app: &AppHandle, request: FileSearchQuery) -> Result<FileSearchQueryResult, String> {
    let runtime = resolve_runtime(app, false)?
        .ok_or_else(|| "搜索引擎未就绪，请先下载并启用 Everything".to_string())?;
    ensure_everything_running(app, &runtime, false)?;

    // Official SDK: wait until IsDBLoaded before querying; otherwise results are empty.
    // Do NOT use IsDBBusy — that flag is true during normal searches (including ours).
    if everything_index_state().indexing() {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while everything_index_state().indexing() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        if everything_index_state().indexing() {
            return Err("Everything 正在建立文件索引，请稍候再搜".into());
        }
    }

    let query = request.query.trim();
    let category = request.category.as_str();

    let limit = request.limit.clamp(1, 200);
    let offset = request.offset;
    let search = build_everything_search(query, category);
    if search.is_empty() {
        return Ok(FileSearchQueryResult {
            items: Vec::new(),
            total: 0,
            has_more: false,
        });
    }

    // Primary path: in-process WM IPC (same class of latency as Everything's own UI).
    // ES.exe spawn + CSV temp file was ~0.5–1s+ per keystroke.
    match query_via_ipc(&request.sort, &search, limit, offset) {
        Ok(result) => return Ok(result),
        Err(ipc_error) => {
            if !runtime.es_path.exists() {
                return Err(format!(
                    "无法通过 Everything IPC 搜索（{ipc_error}）。请确认 Everything 正在运行。"
                ));
            }
            // Fall through to ES CLI.
        }
    }

    query_via_es(&runtime.es_path, &request.sort, &search, limit, offset)
}

#[derive(Debug, Clone)]
struct EverythingRuntime {
    everything_exe: PathBuf,
    es_path: PathBuf,
    version: String,
    portable: bool,
}

fn resolve_runtime(app: &AppHandle, require_es: bool) -> Result<Option<EverythingRuntime>, String> {
    let portable_dir = tools_root(app)?.join("everything");
    let portable_exe = find_file_named(&portable_dir, &["Everything.exe"]);
    let portable_es = find_file_named(&portable_dir, &["ES.exe", "es.exe"]);

    let system_exe = find_system_everything();
    let system_es = find_system_es();

    if let Some(exe) = system_exe.clone() {
        let es = system_es
            .clone()
            .or_else(|| portable_es.clone())
            .or_else(|| find_file_named(exe.parent().unwrap_or(Path::new(".")), &["ES.exe", "es.exe"]));
        if !require_es || es.is_some() {
            return Ok(Some(EverythingRuntime {
                everything_exe: exe,
                es_path: es.unwrap_or_default(),
                version: detect_version_label(&system_exe, false),
                portable: false,
            }));
        }
    }

    if let Some(exe) = portable_exe {
        let es = portable_es.or_else(|| system_es);
        if !require_es || es.as_ref().is_some_and(|path| path.exists()) {
            return Ok(Some(EverythingRuntime {
                everything_exe: exe,
                es_path: es.unwrap_or_default(),
                version: EVERYTHING_VERSION.to_string(),
                portable: true,
            }));
        }
    }

    Ok(None)
}

fn detect_version_label(exe: &Option<PathBuf>, portable: bool) -> String {
    if portable {
        return EVERYTHING_VERSION.to_string();
    }
    exe.as_ref()
        .and_then(|path| path.parent())
        .map(|parent| parent.display().to_string())
        .map(|_| "system".to_string())
        .unwrap_or_else(|| "system".into())
}

fn find_system_everything() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        candidates.push(PathBuf::from(program_files).join("Everything/Everything.exe"));
    }
    if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
        candidates.push(PathBuf::from(program_files_x86).join("Everything/Everything.exe"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join("Everything/Everything.exe"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn find_system_es() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        candidates.push(PathBuf::from(&program_files).join("Everything/ES.exe"));
        candidates.push(PathBuf::from(program_files).join("ES/ES.exe"));
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("ES.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn install_portable(app: &AppHandle) -> Result<(), String> {
    let dir = tools_root(app)?.join("everything");
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建 Everything 目录失败: {error}"))?;
    let everything_zip = dir.join("everything.zip");

    download_file(app, EVERYTHING_ZIP_URL, &everything_zip, "download_everything")
        .map_err(|error| format!("下载 Everything 失败: {error}"))?;
    emit_engine_progress(app, "extract", 0, None, None, Some("正在解压…"));
    extract_zip(&everything_zip, &dir)?;
    let _ = std::fs::remove_file(&everything_zip);

    if find_file_named(&dir, &["Everything.exe"]).is_none() {
        return Err("Everything 压缩包中未找到 Everything.exe（可能被杀软拦截）".into());
    }
    // ES is optional (IPC is primary); keep it when possible for fallback.
    let _ = ensure_es_installed(app);
    Ok(())
}

fn ensure_es_installed(app: &AppHandle) -> Result<(), String> {
    let dir = tools_root(app)?.join("everything");
    if find_file_named(&dir, &["ES.exe", "es.exe"]).is_some() || find_system_es().is_some() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建 Everything 目录失败: {error}"))?;
    let es_zip = dir.join("es.zip");
    download_file(app, ES_ZIP_URL, &es_zip, "download_es")
        .map_err(|error| format!("下载 ES 命令行失败: {error}"))?;
    emit_engine_progress(app, "extract", 0, None, None, Some("正在解压…"));
    extract_zip(&es_zip, &dir)?;
    let _ = std::fs::remove_file(&es_zip);
    if find_file_named(&dir, &["ES.exe", "es.exe"]).is_none() {
        return Err("ES 压缩包中未找到 ES.exe".into());
    }
    Ok(())
}

fn ensure_everything_running(
    app: &AppHandle,
    runtime: &EverythingRuntime,
    report_progress: bool,
) -> Result<(), String> {
    // Prefer FindWindow IPC (microseconds) over `tasklist` (can take 100ms–1s+).
    if everything_ipc_available() || everything_process_running() {
        return Ok(());
    }
    if report_progress {
        emit_engine_progress(app, "start", 0, None, None, Some("正在启动 Everything…"));
    }
    Command::new(&runtime.everything_exe)
        .arg("-startup")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!(
                "无法启动 Everything（{}）: {error}",
                runtime.everything_exe.display()
            )
        })?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if everything_ipc_available() || everything_process_running() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("Everything 启动超时，请确认未被杀软拦截，或手动打开 Everything 后重试".into())
}

fn everything_process_running() -> bool {
    let Ok(output) = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq Everything.exe", "/NH"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    text.contains("everything.exe")
}

/// Official guidance (voidtools): only `IS_DB_LOADED` means the database is still
/// starting. `IS_DB_BUSY` is true whenever *any* query is running — including our
/// own ES searches — so it must not be treated as "indexing".
#[derive(Debug, Clone, Copy)]
struct EverythingIndexState {
    /// `Some(false)` = DB still loading; `Some(true)` = ready; `None` = IPC unavailable.
    loaded: Option<bool>,
}

impl EverythingIndexState {
    fn indexing(self) -> bool {
        matches!(self.loaded, Some(false))
    }

    fn message(self) -> Option<String> {
        self.indexing()
            .then(|| "正在建立文件索引…".to_string())
    }
}

fn with_index_fields(mut status: FileSearchStatus) -> FileSearchStatus {
    let index = everything_index_state();
    status.indexing = index.indexing();
    status.indexing_message = index.message();
    if status.indexing {
        if let Some(message) = index.message() {
            status.message = Some(message);
        }
    }
    status
}

fn everything_ipc_hwnd() -> Option<windows::Win32::Foundation::HWND> {
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    unsafe {
        FindWindowW(w!("EVERYTHING_TASKBAR_NOTIFICATION"), None)
            .ok()
            .filter(|handle| !handle.0.is_null())
            .or_else(|| {
                FindWindowW(w!("EVERYTHING_TASKBAR_NOTIFICATION_(1.5a)"), None)
                    .ok()
                    .filter(|handle| !handle.0.is_null())
            })
    }
}

fn everything_ipc_available() -> bool {
    everything_ipc_hwnd().is_some()
}

/// Query Everything IPC for database load state (Everything 1.4+).
/// See: https://www.voidtools.com/support/everything/sdk/everything_isdbloaded/
fn everything_index_state() -> EverythingIndexState {
    // EVERYTHING_IPC_IS_DB_LOADED = 401
    const IPC_IS_DB_LOADED: usize = 401;

    let Some(hwnd) = everything_ipc_hwnd() else {
        return EverythingIndexState { loaded: None };
    };
    EverythingIndexState {
        loaded: unsafe { send_everything_ipc(hwnd, IPC_IS_DB_LOADED) },
    }
}

unsafe fn send_everything_ipc(
    hwnd: windows::Win32::Foundation::HWND,
    command: usize,
) -> Option<bool> {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, SMTO_ABORTIFHUNG, WM_USER};

    let mut result = windows::Win32::Foundation::LRESULT(0);
    let status = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_USER,
            WPARAM(command),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            200,
            Some(&mut result as *mut _ as *mut usize),
        )
    };
    if status.0 == 0 {
        return None;
    }
    Some(result.0 != 0)
}

fn build_everything_search(query: &str, category: &str) -> String {
    let query = query.trim();
    if query.is_empty() {
        // Empty query: browse by category, or `*` for all files.
        return match category {
            "all" => "*".to_string(),
            "folder" => "folder:".to_string(),
            other => category_extensions(other)
                .map(|exts| format!("ext:{}", exts.join(";")))
                .unwrap_or_else(|| "*".to_string()),
        };
    }

    let escaped = escape_everything_literal(query);
    match category {
        "all" => escaped,
        // `folder:term` (no space) — `folder: term` is treated differently.
        "folder" => format!("folder:{escaped}"),
        other => {
            if let Some(exts) = category_extensions(other) {
                // Typing an extension name in its category (e.g. 图片 + "svg")
                // should hit that extension only.
                let query_lower = query.to_ascii_lowercase();
                if exts
                    .iter()
                    .any(|ext| ext.eq_ignore_ascii_case(query_lower.as_str()))
                {
                    return format!("ext:{query_lower}");
                }
                // Prefer `ext:a;b keyword` (~20ms via WM IPC). Mid-string OR
                // wildcards like `*kw*.png|*kw*.jpg|...` take ~1s+ and often
                // time out — that made category search feel broken/slow while
                // "全部" stayed fast.
                format!("ext:{} {escaped}", exts.join(";"))
            } else {
                escaped
            }
        }
    }
}

/// Escape Everything special characters inside a literal search fragment.
fn escape_everything_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\"\""),
            '|' | '*' | '?' => {
                out.push('"');
                out.push(ch);
                out.push('"');
            }
            _ => out.push(ch),
        }
    }
    out
}

fn everything_sort_flag(sort: &str) -> &'static str {
    match sort {
        "mtime_asc" => "date-modified-ascending",
        "name_asc" => "name-ascending",
        "name_desc" => "name-descending",
        "size_asc" => "size-ascending",
        "size_desc" => "size-descending",
        _ => "date-modified-descending",
    }
}

fn everything_ipc_sort(sort: &str) -> everything_ipc::wm::Sort {
    use everything_ipc::wm::Sort;
    match sort {
        "mtime_asc" => Sort::DateModifiedAscending,
        "name_asc" => Sort::NameAscending,
        "name_desc" => Sort::NameDescending,
        "size_asc" => Sort::SizeAscending,
        "size_desc" => Sort::SizeDescending,
        _ => Sort::DateModifiedDescending,
    }
}

/// Direct Everything WM IPC — no process spawn, no CSV temp file.
fn query_via_ipc(
    sort: &str,
    search: &str,
    limit: u32,
    offset: u32,
) -> Result<FileSearchQueryResult, String> {
    use everything_ipc::wm::{EverythingClient, RequestFlags};
    use std::time::Duration;

    let client = EverythingClient::shared().map_err(|error| error.to_string())?;
    // Fetch one extra row so has_more is accurate without a separate count query.
    let fetch_limit = limit.saturating_add(1).min(201);
    let flags = RequestFlags::FullPathAndFileName
        | RequestFlags::Size
        | RequestFlags::DateModified
        | RequestFlags::Attributes;

    let list = client
        .query_wait(search)
        .request_flags(flags)
        .sort(everything_ipc_sort(sort))
        .offset(offset)
        .max_results(fetch_limit)
        .timeout(Duration::from_millis(1500))
        .call()
        .map_err(|error| error.to_string())?;

    let mut items = Vec::with_capacity(list.len());
    for item in list.iter() {
        let Some(path) = item.get_string(RequestFlags::FullPathAndFileName) else {
            continue;
        };
        let path_buf = PathBuf::from(&path);
        let name = path_buf
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(path.as_str())
            .to_string();
        if name.is_empty() {
            continue;
        }
        // FILE_ATTRIBUTE_DIRECTORY = 0x10 — avoid per-row disk metadata.
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        let attrs = item.get_u32(RequestFlags::Attributes).unwrap_or(0);
        let is_dir = (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0;
        let size = if is_dir {
            None
        } else {
            item.get_size(RequestFlags::Size)
        };
        let modified_at = item.get_time(RequestFlags::DateModified).and_then(|ft| {
            filetime_parts_to_system_time(ft.dwHighDateTime, ft.dwLowDateTime)
                .and_then(super::commands::system_time_to_rfc3339)
        });
        let extension = if is_dir {
            None
        } else {
            path_buf
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
        };
        items.push(FileSearchItem {
            name,
            path,
            is_dir,
            size,
            modified_at,
            extension,
        });
    }

    let has_more = items.len() > limit as usize;
    if has_more {
        items.truncate(limit as usize);
    }
    let total = {
        let reported = list.total_len() as u32;
        if reported > 0 {
            reported
        } else if has_more {
            offset.saturating_add(items.len() as u32).saturating_add(1)
        } else {
            offset.saturating_add(items.len() as u32)
        }
    };
    Ok(FileSearchQueryResult {
        items,
        total,
        has_more,
    })
}

fn filetime_parts_to_system_time(high: u32, low: u32) -> Option<std::time::SystemTime> {
    use std::time::{Duration, UNIX_EPOCH};
    let ticks = ((high as u64) << 32) | (low as u64);
    // FILETIME epochs at 1601-01-01; Unix at 1970-01-01.
    const EPOCH_DIFF: u64 = 116_444_736_000_000_000;
    if ticks < EPOCH_DIFF {
        return None;
    }
    let nanos = (ticks - EPOCH_DIFF).checked_mul(100)?;
    Some(UNIX_EPOCH + Duration::from_nanos(nanos))
}

fn query_via_es(
    es_path: &Path,
    sort: &str,
    search: &str,
    limit: u32,
    offset: u32,
) -> Result<FileSearchQueryResult, String> {
    let sort_flag = everything_sort_flag(sort);
    let offset_str = offset.to_string();
    let fetch_limit = limit.saturating_add(1).min(201);
    let fetch_limit_str = fetch_limit.to_string();

    let output = run_es(
        es_path,
        &[
            "-offset",
            &offset_str,
            "-n",
            &fetch_limit_str,
            "-size",
            "-date-modified",
            "-sort",
            sort_flag,
            "-timeout",
            "500",
            search,
        ],
    )?;

    let mut items = parse_es_output(&output);
    // Only touch disk when CSV omitted size/date/extension — never for the IPC path.
    for item in &mut items {
        if item.size.is_some() && item.modified_at.is_some() && item.extension.is_some() {
            continue;
        }
        if let Some(fresh) = item_from_path(Path::new(&item.path)) {
            if item.size.is_none() {
                item.size = fresh.size;
            }
            if item.modified_at.is_none() {
                item.modified_at = fresh.modified_at;
            }
            item.is_dir = fresh.is_dir;
            if item.extension.is_none() {
                item.extension = fresh.extension;
            }
        } else if item.extension.is_none() {
            item.extension = Path::new(&item.path)
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase());
        }
    }

    let has_more = items.len() > limit as usize;
    if has_more {
        items.truncate(limit as usize);
    }
    let total = if has_more {
        offset.saturating_add(items.len() as u32).saturating_add(1)
    } else {
        offset.saturating_add(items.len() as u32)
    };
    Ok(FileSearchQueryResult {
        items,
        total,
        has_more,
    })
}

fn run_es(es_path: &Path, args: &[&str]) -> Result<String, String> {
    if !es_path.exists() {
        return Err("未找到 ES.exe，请重新下载搜索引擎".into());
    }

    // Prefer UTF-8 CSV export: console stdout on Chinese Windows is typically ACP
    // (CP936), and `from_utf8_lossy` would garble CJK paths.
    let export_path = std::env::temp_dir().join(format!(
        "tempo-es-export-{}-{}.csv",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    let export_path_arg = export_path.to_string_lossy().into_owned();

    let mut cmd_args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    cmd_args.extend([
        "-no-digit-grouping".into(),
        "-date-format".into(),
        "1".into(), // ISO-8601
        "-no-header".into(),
        "-utf8-bom".into(),
        "-export-csv".into(),
        export_path_arg,
    ]);

    let mut child = Command::new(es_path)
        .args(&cmd_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("启动 ES 失败: {error}"))?;

    let timeout = Duration::from_secs(QUERY_TIMEOUT_SECS);
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&export_path);
                if everything_index_state().indexing() {
                    return Err("Everything 正在建立文件索引，请稍候再搜".into());
                }
                return Err("搜索超时，请缩小关键词或稍后再试".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(error) => {
                let _ = std::fs::remove_file(&export_path);
                return Err(format!("等待 ES 失败: {error}"));
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("读取 ES 输出失败: {error}"))?;

    let export_text = read_utf8_export_file(&export_path);
    let _ = std::fs::remove_file(&export_path);

    let stderr = decode_windows_bytes(&output.stderr);
    let stdout = decode_windows_bytes(&output.stdout);

    if !output.status.success() {
        let detail = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            String::new()
        };
        if detail.to_ascii_lowercase().contains("everything")
            || detail.to_ascii_lowercase().contains("ipc")
            || detail.contains("无法")
        {
            return Err(format!(
                "无法连接 Everything IPC。请确认 Everything 正在运行。{detail}"
            ));
        }
        // Prefer export content when present; some ES builds exit non-zero on
        // empty results.
        if let Some(text) = export_text {
            return Ok(text);
        }
        if detail.is_empty() {
            return Ok(String::new());
        }
    }

    if let Some(text) = export_text {
        return Ok(text);
    }

    // Fallback: older ES / export failure — decode console stdout via ACP.
    Ok(stdout)
}

fn read_utf8_export_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let content = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes.as_slice()
    };
    match std::str::from_utf8(content) {
        Ok(text) => Some(text.to_string()),
        Err(_) => Some(decode_windows_bytes(content)),
    }
}

fn decode_windows_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let encoding = match unsafe { windows::Win32::Globalization::GetACP() } {
        932 => encoding_rs::SHIFT_JIS,
        936 => encoding_rs::GBK,
        949 => encoding_rs::EUC_KR,
        950 => encoding_rs::BIG5,
        1251 => encoding_rs::WINDOWS_1251,
        1252 => encoding_rs::WINDOWS_1252,
        _ => encoding_rs::GBK,
    };
    let (cow, _, _) = encoding.decode(bytes);
    cow.into_owned()
}

fn parse_es_output(output: &str) -> Vec<FileSearchItem> {
    let mut items = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(item) = parse_es_line(line) {
            items.push(item);
        }
    }
    items
}

/// Prefer CSV rows from `-export-csv` (`size,date-modified,path` or similar).
/// Fallback: whitespace console lines `YYYY-MM-DD HH:MM:SS  <size>  <full path>`.
fn parse_es_line(line: &str) -> Option<FileSearchItem> {
    if line.contains(',') {
        if let Some(item) = parse_es_csv_line(line) {
            return Some(item);
        }
    }
    parse_es_whitespace_line(line)
}

fn parse_es_csv_line(line: &str) -> Option<FileSearchItem> {
    let fields = split_csv_fields(line);
    if fields.is_empty() {
        return None;
    }

    let mut size = None;
    let mut modified_at = None;
    let mut path: Option<String> = None;

    for field in &fields {
        let value = field.trim();
        if value.is_empty() {
            continue;
        }
        if looks_like_windows_path(value) {
            path = Some(value.to_string());
            continue;
        }
        if let Some(date) = normalize_es_date(value) {
            modified_at = Some(date);
            continue;
        }
        if let Ok(n) = value.replace(',', "").parse::<u64>() {
            size = Some(n);
        }
    }

    let path = path.or_else(|| {
        fields
            .last()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })?;
    if !looks_like_windows_path(&path) && PathBuf::from(&path).components().count() < 2 {
        return None;
    }

    let path_buf = PathBuf::from(&path);
    let name = path_buf
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path.as_str())
        .to_string();
    // Prefer CSV fields; avoid disk metadata for every row (was a major latency source).
    let extension = path_buf
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    Some(FileSearchItem {
        name,
        path,
        is_dir: false,
        size,
        modified_at,
        extension,
    })
}

fn parse_es_whitespace_line(line: &str) -> Option<FileSearchItem> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        let path = PathBuf::from(line);
        return item_from_path(&path);
    }

    // date time size path...
    let maybe_date = parts[0];
    let maybe_time = parts[1];
    let maybe_size = parts[2];
    let looks_like_date = maybe_date.len() == 10 && maybe_date.contains('-');
    let looks_like_time = maybe_time.contains(':');
    if looks_like_date && looks_like_time {
        let size = maybe_size.replace(',', "").parse::<u64>().ok();
        let path = parts[3..].join(" ");
        let path_buf = PathBuf::from(&path);
        let mut item = item_from_path(&path_buf).unwrap_or(FileSearchItem {
            name: path_buf
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(path.as_str())
                .to_string(),
            path: path.clone(),
            is_dir: path_buf.is_dir(),
            size: None,
            modified_at: None,
            extension: path_buf
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase()),
        });
        if size.is_some() {
            item.size = size;
        }
        item.modified_at = Some(format!("{maybe_date}T{maybe_time}"));
        return Some(item);
    }

    let path = PathBuf::from(line);
    item_from_path(&path)
}

fn split_csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    fields.push(current);
    fields
}

fn looks_like_windows_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return true;
    }
    value.starts_with("\\\\") || value.starts_with("//")
}

fn normalize_es_date(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    // ISO-8601 from `-date-format 1`
    if trimmed.len() >= 19 && trimmed.as_bytes().get(4) == Some(&b'-') && trimmed.contains('T') {
        return Some(trimmed.to_string());
    }
    // `YYYY-MM-DD HH:MM:SS`
    if trimmed.len() >= 19
        && trimmed.as_bytes().get(4) == Some(&b'-')
        && trimmed.as_bytes().get(10) == Some(&b' ')
        && trimmed.contains(':')
    {
        return Some(trimmed.replacen(' ', "T", 1));
    }
    // Date-only
    if trimmed.len() == 10 && trimmed.as_bytes().get(4) == Some(&b'-') {
        return Some(trimmed.to_string());
    }
    None
}
