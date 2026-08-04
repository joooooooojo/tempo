use super::commands::{
    category_extensions, item_from_path, sort_items, tools_root, FileSearchItem, FileSearchQuery,
    FileSearchQueryResult, FileSearchStatus,
};
use super::tools::{
    download_file, emit_engine_progress, find_file_named, github_latest_asset_url, which_on_path,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tauri::AppHandle;

const QUERY_TIMEOUT_SECS: u64 = 20;
const PAGE_LIMIT_MAX: u32 = 200;

const FD_EXCLUDES: &[&str] = &[
    ".git",
    "node_modules",
    "Library/Caches",
    "Library/Logs",
    ".Trash",
    "System",
    "private",
    "dev",
    "Volumes",
    "cores",
    ".Spotlight-V100",
    ".fseventsd",
    "Containers",
    "Group Containers",
];

pub(crate) fn status(app: &AppHandle) -> Result<FileSearchStatus, String> {
    match resolve_fd(app)? {
        Some(fd) => Ok(FileSearchStatus {
            ready: true,
            engine: Some("fd".into()),
            version: Some(fd_version(&fd).unwrap_or_else(|| "installed".into())),
            message: Some("已就绪（fd 实时扫描全盘，无持久索引）".into()),
        }),
        None => Ok(FileSearchStatus {
            ready: false,
            engine: Some("fd".into()),
            version: None,
            message: Some("未检测到 fd，可下载最新版以启用全盘搜索".into()),
        }),
    }
}

pub(crate) fn ensure_engine(app: &AppHandle) -> Result<FileSearchStatus, String> {
    if let Some(fd) = resolve_fd(app)? {
        emit_engine_progress(app, "done", 0, None, Some(100.0), Some("完成"));
        return Ok(FileSearchStatus {
            ready: true,
            engine: Some("fd".into()),
            version: Some(fd_version(&fd).unwrap_or_else(|| "installed".into())),
            message: Some("fd 已就绪（实时扫描）".into()),
        });
    }
    install_fd(app)?;
    let fd = resolve_fd(app)?.ok_or_else(|| "fd 安装后仍无法定位可执行文件".to_string())?;
    emit_engine_progress(app, "done", 0, None, Some(100.0), Some("完成"));
    Ok(FileSearchStatus {
        ready: true,
        engine: Some("fd".into()),
        version: Some(fd_version(&fd).unwrap_or_else(|| "installed".into())),
        message: Some("fd 已下载并就绪（实时扫描全盘）".into()),
    })
}

pub(crate) fn query(app: &AppHandle, request: FileSearchQuery) -> Result<FileSearchQueryResult, String> {
    let fd = resolve_fd(app)?.ok_or_else(|| "搜索引擎未就绪，请先下载并启用 fd".to_string())?;
    let query = request.query.trim();
    let category = request.category.as_str();
    // Empty query browses via --extension / --type d, or `^` + `/` for all (paged by max-results).

    let limit = request.limit.clamp(1, PAGE_LIMIT_MAX);
    let offset = request.offset;
    // fd has no offset; fetch a window of offset+limit then skip in Rust.
    let fetch = offset.saturating_add(limit);

    let mut args = vec![
        "--color".to_string(),
        "never".to_string(),
        "--max-results".to_string(),
        fetch.to_string(),
        "--hidden".to_string(),
        "--no-ignore".to_string(),
    ];

    for pattern in FD_EXCLUDES {
        args.push("--exclude".into());
        args.push((*pattern).into());
    }

    match category {
        "folder" => {
            args.push("--type".into());
            args.push("d".into());
        }
        other => {
            if let Some(exts) = category_extensions(other) {
                for ext in exts {
                    args.push("--extension".into());
                    args.push((*ext).into());
                }
            }
        }
    }

    if query.is_empty() {
        // Match all names; category flags narrow the set.
        args.push("^".into());
    } else {
        args.push("--fixed-strings".into());
        args.push(query.to_string());
    }
    // Critical: full-disk root (not $HOME).
    args.push("/".into());

    let output = run_fd(&fd, &args)?;
    let lines: Vec<&str> = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let fetched_line_count = lines.len();

    let mut items: Vec<FileSearchItem> = lines
        .into_iter()
        .filter_map(|line| item_from_path(Path::new(line)))
        .collect();

    sort_items(&mut items, &request.sort);

    let start = (offset as usize).min(items.len());
    let page: Vec<FileSearchItem> = items.into_iter().skip(start).take(limit as usize).collect();
    // Full window means more may exist beyond this page.
    let has_more = fetched_line_count > start.saturating_add(page.len());
    let total = if has_more {
        // Lower bound: at least one more than the end of this page.
        (start.saturating_add(page.len()).saturating_add(1)) as u32
    } else {
        start.saturating_add(page.len()) as u32
    };

    Ok(FileSearchQueryResult {
        items: page,
        total,
        has_more,
    })
}

fn resolve_fd(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    if let Some(path) = which_on_path("fd") {
        return Ok(Some(path));
    }
    let tools = tools_root(app)?.join("fd");
    if let Some(path) = find_file_named(&tools, &["fd"]) {
        return Ok(Some(path));
    }
    Ok(None)
}

fn fd_version(fd: &Path) -> Option<String> {
    let output = Command::new(fd)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().next().map(|line| line.trim().to_string())
}

fn install_fd(app: &AppHandle) -> Result<(), String> {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64-apple-darwin",
        "x86_64" => "x86_64-apple-darwin",
        other => return Err(format!("暂不支持的 macOS 架构: {other}")),
    };
    let (tag, url) = github_latest_asset_url("sharkdp", "fd", &[arch, ".tar.gz"])?;
    let dir = tools_root(app)?.join("fd");
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建 fd 目录失败: {error}"))?;
    let archive = dir.join(format!("fd-{tag}.tar.gz"));
    download_file(app, &url, &archive, "download_fd")?;
    emit_engine_progress(app, "extract", 0, None, None, Some("正在解压…"));
    extract_tar_gz(&archive, &dir)?;
    let _ = std::fs::remove_file(&archive);
    if find_file_named(&dir, &["fd"]).is_none() {
        return Err("fd 压缩包中未找到可执行文件".into());
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    // Prefer system tar for .tar.gz (zip crate does not handle gzip tar).
    let status = Command::new("tar")
        .args([
            "-xzf",
            &archive.to_string_lossy(),
            "-C",
            &dest.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|error| format!("解压 fd 失败（无法启动 tar）: {error}"))?;
    if !status.success() {
        return Err("解压 fd 失败，请检查网络下载的压缩包是否完整".into());
    }
    Ok(())
}

fn run_fd(fd: &Path, args: &[String]) -> Result<String, String> {
    let mut child = Command::new(fd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("启动 fd 失败: {error}"))?;

    let timeout = Duration::from_secs(QUERY_TIMEOUT_SECS);
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("全盘搜索超时（fd 为实时扫描）。请缩小关键词或更换分类后重试".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(format!("等待 fd 失败: {error}")),
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("读取 fd 输出失败: {error}"))?;
    // fd may return non-zero on permission errors while still printing results.
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
