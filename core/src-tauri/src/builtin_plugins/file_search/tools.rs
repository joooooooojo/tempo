use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const ENGINE_PROGRESS_EVENT: &str = "file-search:engine-progress";

const PROGRESS_BYTE_STEP: u64 = 64 * 1024;
const PROGRESS_PERCENT_STEP: u8 = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineProgressPayload {
    pub stage: String,
    pub current: u64,
    pub total: Option<u64>,
    pub percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

pub(crate) fn emit_engine_progress(
    app: &AppHandle,
    stage: &str,
    current: u64,
    total: Option<u64>,
    percent: Option<f64>,
    label: Option<&str>,
) {
    let _ = app.emit(
        ENGINE_PROGRESS_EVENT,
        EngineProgressPayload {
            stage: stage.to_string(),
            current,
            total,
            percent,
            label: label.map(|value| value.to_string()),
        },
    );
}

pub(crate) fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("Tempo/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("创建 HTTP 客户端失败: {error}"))
}

pub(crate) fn download_file(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    stage: &str,
) -> Result<u64, String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建目录失败: {error}"))?;
    }
    let client = http_client()?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|error| format!("下载失败（网络）: {error}"))?
        .error_for_status()
        .map_err(|error| format!("下载失败（HTTP）: {error}"))?;
    let total = response.content_length();
    emit_engine_progress(
        app,
        stage,
        0,
        total,
        total.map(|_| 0.0),
        None,
    );

    let mut file = File::create(dest).map_err(|error| format!("创建下载文件失败: {error}"))?;
    let mut downloaded: u64 = 0;
    let mut last_emit_bytes: u64 = 0;
    let mut last_emit_pct: u8 = 0;
    let mut buf = [0u8; PROGRESS_BYTE_STEP as usize];

    loop {
        let n = response
            .read(&mut buf)
            .map_err(|error| format!("读取下载流失败: {error}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|error| format!("写入下载文件失败: {error}"))?;
        downloaded = downloaded.saturating_add(n as u64);

        let percent = total.map(|t| {
            if t == 0 {
                0.0
            } else {
                ((downloaded as f64) * 100.0 / (t as f64)).min(100.0)
            }
        });

        let bytes_due = downloaded.saturating_sub(last_emit_bytes) >= PROGRESS_BYTE_STEP;
        let percent_due = percent
            .map(|pct| {
                let floor = pct.floor() as u8;
                floor >= last_emit_pct.saturating_add(PROGRESS_PERCENT_STEP)
            })
            .unwrap_or(false);

        if bytes_due || percent_due || downloaded == total.unwrap_or(u64::MAX) {
            last_emit_bytes = downloaded;
            if let Some(pct) = percent {
                last_emit_pct = pct.floor() as u8;
            }
            emit_engine_progress(app, stage, downloaded, total, percent, None);
        }
    }

    file.flush()
        .map_err(|error| format!("刷新下载文件失败: {error}"))?;
    emit_engine_progress(
        app,
        stage,
        downloaded,
        total.or(Some(downloaded)),
        Some(100.0),
        None,
    );
    Ok(downloaded)
}

pub(crate) fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|error| format!("创建解压目录失败: {error}"))?;
    let file = File::open(archive).map_err(|error| format!("打开压缩包失败: {error}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| format!("解析压缩包失败: {error}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取压缩条目失败: {error}"))?;
        let name = entry
            .enclosed_name()
            .ok_or_else(|| "压缩包包含非法路径".to_string())?
            .to_path_buf();
        let out_path = dest.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|error| format!("创建目录失败: {error}"))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("创建目录失败: {error}"))?;
        }
        let mut outfile =
            File::create(&out_path).map_err(|error| format!("创建解压文件失败: {error}"))?;
        std::io::copy(&mut entry, &mut outfile)
            .map_err(|error| format!("写入解压文件失败: {error}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(())
}

pub(crate) fn find_file_named(root: &Path, names: &[&str]) -> Option<PathBuf> {
    if !root.exists() {
        return None;
    }
    for name in names {
        let direct = root.join(name);
        if direct.is_file() {
            return Some(direct);
        }
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let file_name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
            if names.iter().any(|name| name.eq_ignore_ascii_case(file_name)) {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn which_on_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub(crate) fn github_latest_asset_url(
    owner: &str,
    repo: &str,
    name_contains: &[&str],
) -> Result<(String, String), String> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let client = http_client()?;
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|error| format!("请求 GitHub Release 失败: {error}"))?
        .error_for_status()
        .map_err(|error| format!("GitHub Release HTTP 错误: {error}"))?;
    let payload: serde_json::Value = response
        .json()
        .map_err(|error| format!("解析 GitHub Release 失败: {error}"))?;
    let tag = payload
        .get("tag_name")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown")
        .to_string();
    let assets = payload
        .get("assets")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "GitHub Release 无资源列表".to_string())?;
    for asset in assets {
        let name = asset
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if name_contains.iter().all(|part| name.contains(part)) {
            let download_url = asset
                .get("browser_download_url")
                .and_then(|value| value.as_str())
                .ok_or_else(|| "GitHub Release 资源缺少下载地址".to_string())?;
            return Ok((tag, download_url.to_string()));
        }
    }
    Err(format!(
        "未找到匹配的 GitHub 资源（需要包含: {}）",
        name_contains.join(", ")
    ))
}
