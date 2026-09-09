//! On-demand plugin Deno runtime (not bundled with Tempo; unrelated to system Deno).

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as AsyncMutex;

use super::paths::{deno_runtime_dir, ensure_dir, plugin_runtime_root, runtime_manifest_path};

/// Locked Deno major line for Tempo plugins. Patch is chosen by the download manifest.
pub const LOCKED_DENO_MAJOR: &str = "2";

/// Official denoland/deno build this Tempo release is locked to (design §3.3.1).
pub const OFFICIAL_DENO_VERSION: &str = "2.9.6";

/// Test-only escape hatch: point directly at a pre-installed Deno binary and skip the
/// on-demand download/verify flow entirely. Never read outside of local development/tests.
pub const DENO_PATH_OVERRIDE_ENV: &str = "TEMPO_PLUGIN_DENO_PATH";
/// Override the official runtime manifest URL (mirrors / air-gapped installs / tests).
pub const MANIFEST_URL_OVERRIDE_ENV: &str = "TEMPO_PLUGIN_RUNTIME_MANIFEST_URL";

pub const INSTALL_PROGRESS_EVENT: &str = "plugin-runtime-install-progress";

/// Serializes install attempts so closing/reopening the main panel cannot start a second download.
static INSTALL_LOCK: AsyncMutex<()> = AsyncMutex::const_new(());

/// Survives WebView remounts: settings can re-query and keep showing progress.
static INSTALL_PROGRESS: Mutex<Option<RuntimeInstallProgress>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInstallProgress {
    /// downloading | verifying | extracting | finalizing | failed
    pub phase: String,
    pub message: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    /// 0..=100 when total is known; otherwise None.
    pub percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRuntimeStatus {
    pub installed: bool,
    /// True while a download/extract is in flight (persists across main panel remounts).
    pub installing: bool,
    pub version: Option<String>,
    pub deno_path: Option<String>,
    pub install_dir: Option<String>,
    pub locked_major: String,
    pub message: String,
    pub progress: Option<RuntimeInstallProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalRuntimeManifest {
    version: String,
    deno_path: String,
    package_hash: String,
    binary_sha256: String,
    installed_at: String,
    target: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteRuntimeManifest {
    version: String,
    /// Map of rustc target triple -> artifact.
    artifacts: std::collections::HashMap<String, RemoteArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteArtifact {
    url: String,
    sha256: String,
}

/// Hardcoded official Deno 2.9.6 artifacts (GitHub releases). No third-party CDN is used;
/// `TEMPO_PLUGIN_RUNTIME_MANIFEST_URL` may still override this for mirrors/tests.
fn official_manifest() -> RemoteRuntimeManifest {
    let artifacts = [
        ("aarch64-apple-darwin", "213a2f304f04d3c9cb5220669afad138f60a5aab1fe80962abdeb8f35807a472"),
        ("x86_64-apple-darwin", "7d4524b82bcc557fe020a1a5b56956ed42b992ae5b28026e8ad5d17329533f5f"),
        ("x86_64-unknown-linux-gnu", "394f07f4da2bebe6ce6f1e7ce0fa16429b29b08c35e3fac3fe25972676dff4b2"),
        ("x86_64-pc-windows-msvc", "15e5300b0ba3c3695a7621d90160a746ec9e710228cee639afa9d580f6e3cd11"),
    ].into_iter().map(|(target, hash)| (target.to_string(), RemoteArtifact {
        url: format!("https://github.com/denoland/deno/releases/download/v{OFFICIAL_DENO_VERSION}/deno-{target}.zip"),
        sha256: hash.into(),
    })).collect();
    RemoteRuntimeManifest {
        version: OFFICIAL_DENO_VERSION.into(),
        artifacts,
    }
}

fn current_target_triple() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )))]
    {
        "unsupported"
    }
}

fn deno_exe_name() -> &'static str {
    if cfg!(windows) {
        "deno.exe"
    } else {
        "deno"
    }
}

fn resolve_deno_binary(install_dir: &std::path::Path) -> PathBuf {
    // Official Deno zip/tar layouts differ slightly by platform.
    let candidates = [
        install_dir.join(deno_exe_name()),
        install_dir.join("bin").join(deno_exe_name()),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            return candidate;
        }
    }
    // Accept a single nested archive directory.
    if let Ok(entries) = fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let nested = [
                    path.join(deno_exe_name()),
                    path.join("bin").join(deno_exe_name()),
                ];
                for candidate in nested {
                    if candidate.is_file() {
                        return candidate;
                    }
                }
            }
        }
    }
    install_dir.join(deno_exe_name())
}

/// Test-only direct override, bypassing install state entirely.
fn test_deno_path_override() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var(DENO_PATH_OVERRIDE_ENV)
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

fn current_progress() -> Option<RuntimeInstallProgress> {
    INSTALL_PROGRESS.lock().ok().and_then(|guard| guard.clone())
}

fn set_progress(app: &AppHandle, progress: RuntimeInstallProgress) {
    if let Ok(mut guard) = INSTALL_PROGRESS.lock() {
        *guard = Some(progress.clone());
    }
    let _ = app.emit(INSTALL_PROGRESS_EVENT, &progress);
}

fn clear_progress() {
    if let Ok(mut guard) = INSTALL_PROGRESS.lock() {
        *guard = None;
    }
}

fn status_base(
    installed: bool,
    version: Option<String>,
    deno_path: Option<String>,
    install_dir: Option<String>,
    message: String,
) -> PluginRuntimeStatus {
    let progress = current_progress();
    let installing = progress
        .as_ref()
        .is_some_and(|p| p.phase != "failed" && p.phase != "done");
    PluginRuntimeStatus {
        installed,
        installing,
        version,
        deno_path,
        install_dir,
        locked_major: LOCKED_DENO_MAJOR.into(),
        message,
        progress,
    }
}

pub fn get_plugin_runtime_status(app: &AppHandle) -> Result<PluginRuntimeStatus, String> {
    if let Some(path) = test_deno_path_override() {
        return Ok(status_base(
            true,
            Some(format!("{LOCKED_DENO_MAJOR}.x (test override)")),
            Some(path.display().to_string()),
            path.parent().map(|p| p.display().to_string()),
            "使用 TEMPO_PLUGIN_DENO_PATH 指定的测试 Deno。".into(),
        ));
    }

    let progress = current_progress();
    let installing = progress
        .as_ref()
        .is_some_and(|p| p.phase != "failed" && p.phase != "done");

    let manifest_path = runtime_manifest_path(app)?;
    if !manifest_path.is_file() {
        let message = if installing {
            progress
                .as_ref()
                .map(|p| p.message.clone())
                .unwrap_or_else(|| "正在安装插件运行时…".into())
        } else {
            "插件运行时未安装。启用含 main 的第三方插件前需要先安装。".into()
        };
        return Ok(status_base(false, None, None, None, message));
    }

    let raw =
        fs::read_to_string(&manifest_path).map_err(|e| format!("read runtime manifest: {e}"))?;
    let local: LocalRuntimeManifest =
        serde_json::from_str(&raw).map_err(|e| format!("parse runtime manifest: {e}"))?;
    let deno_path = PathBuf::from(&local.deno_path);
    let expected_root = deno_runtime_dir(app, OFFICIAL_DENO_VERSION)?;
    if local.version != OFFICIAL_DENO_VERSION
        || local.target != current_target_triple()
        || !deno_path.is_file()
        || !deno_path.starts_with(&expected_root)
    {
        return Ok(status_base(
            false,
            Some(local.version),
            None,
            Some(
                deno_path
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            ),
            "插件运行时清单存在，但 Deno 可执行文件缺失，请重新安装。".into(),
        ));
    }

    Ok(status_base(
        true,
        Some(local.version),
        Some(local.deno_path),
        deno_path.parent().map(|p| p.display().to_string()),
        if installing {
            "插件运行时已就绪（另有安装任务进行中）。".into()
        } else {
            "插件运行时已就绪。".into()
        },
    ))
}

/// Resolve the manifest to use: hardcoded official build, or an override URL fetched over HTTP
/// (mirrors / air-gapped installs / tests only — never an arbitrary plugin-supplied endpoint).
async fn resolve_runtime_manifest() -> Result<RemoteRuntimeManifest, String> {
    if !cfg!(debug_assertions) {
        return Ok(official_manifest());
    }
    if let Ok(url) = std::env::var(MANIFEST_URL_OVERRIDE_ENV) {
        let client = reqwest::Client::new();
        let remote: RemoteRuntimeManifest = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("下载运行时清单失败: {e}"))?
            .error_for_status()
            .map_err(|e| format!("运行时清单 HTTP 错误: {e}"))?
            .json()
            .await
            .map_err(|e| format!("解析运行时清单失败: {e}"))?;
        return Ok(remote);
    }
    Ok(official_manifest())
}

async fn download_with_progress(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    dest: &std::path::Path,
) -> Result<u64, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载 Deno 失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载 Deno HTTP 错误: {e}"))?;

    let total = response.content_length();
    let mut file = fs::File::create(dest).map_err(|e| format!("创建下载文件失败: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut last_emit_pct: u8 = 255;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("读取下载流失败: {e}"))?
    {
        file.write_all(&chunk)
            .map_err(|e| format!("写入下载文件失败: {e}"))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);

        let percent = total.map(|t| {
            if t == 0 {
                0
            } else {
                ((downloaded.saturating_mul(100)) / t).min(100) as u8
            }
        });

        // Throttle UI events to whole-percent changes (or every ~512 KiB when size unknown).
        let should_emit = match percent {
            Some(pct) if pct != last_emit_pct => {
                last_emit_pct = pct;
                true
            }
            None if downloaded == 0 || downloaded % (512 * 1024) < chunk.len() as u64 => true,
            _ => false,
        };
        if should_emit {
            let mb = downloaded as f64 / (1024.0 * 1024.0);
            let message = match total {
                Some(t) => {
                    let total_mb = t as f64 / (1024.0 * 1024.0);
                    format!("正在下载 Deno… {mb:.1}/{total_mb:.1} MB")
                }
                None => format!("正在下载 Deno… {mb:.1} MB"),
            };
            set_progress(
                app,
                RuntimeInstallProgress {
                    phase: "downloading".into(),
                    message,
                    downloaded_bytes: downloaded,
                    total_bytes: total,
                    percent,
                },
            );
        }
    }

    file.flush().map_err(|e| format!("刷新下载文件失败: {e}"))?;
    Ok(downloaded)
}

/// Starts (or joins) a background install that outlives the settings WebView. Returns the
/// current status immediately — callers should poll `plugin_runtime_status` / listen to
/// `plugin-runtime-install-progress` until `installing` becomes false.
pub fn start_plugin_runtime_install(app: AppHandle) -> Result<PluginRuntimeStatus, String> {
    let status = get_plugin_runtime_status(&app)?;
    if status.installed {
        clear_progress();
        return Ok(status);
    }
    if status.installing {
        return Ok(status);
    }

    let target = current_target_triple();
    if target == "unsupported" {
        return Err("当前平台暂不支持插件运行时".into());
    }

    // Mark installing BEFORE spawn so a second click / remount sees `installing: true`.
    {
        let mut guard = INSTALL_PROGRESS
            .lock()
            .map_err(|_| "runtime progress lock poisoned".to_string())?;
        if guard
            .as_ref()
            .is_some_and(|p| p.phase != "failed" && p.phase != "done")
        {
            drop(guard);
            return get_plugin_runtime_status(&app);
        }
        *guard = Some(RuntimeInstallProgress {
            phase: "downloading".into(),
            message: "正在准备下载插件运行时…".into(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(0),
        });
    }
    let _ = app.emit(
        INSTALL_PROGRESS_EVENT,
        RuntimeInstallProgress {
            phase: "downloading".into(),
            message: "正在准备下载插件运行时…".into(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(0),
        },
    );

    let app_bg = app.clone();
    tauri::async_runtime::spawn(async move {
        let _lock = INSTALL_LOCK.lock().await;
        if get_plugin_runtime_status(&app_bg)
            .map(|s| s.installed)
            .unwrap_or(false)
        {
            clear_progress();
            return;
        }

        let result = install_plugin_runtime_inner(&app_bg, current_target_triple()).await;
        match result {
            Ok(_) => {
                clear_progress();
                let _ = app_bg.emit(
                    INSTALL_PROGRESS_EVENT,
                    RuntimeInstallProgress {
                        phase: "done".into(),
                        message: "插件运行时已安装".into(),
                        downloaded_bytes: 0,
                        total_bytes: None,
                        percent: Some(100),
                    },
                );
            }
            Err(error) => {
                set_progress(
                    &app_bg,
                    RuntimeInstallProgress {
                        phase: "failed".into(),
                        message: error,
                        downloaded_bytes: 0,
                        total_bytes: None,
                        percent: None,
                    },
                );
            }
        }
    });

    get_plugin_runtime_status(&app)
}

async fn install_plugin_runtime_inner(
    app: &AppHandle,
    target: &str,
) -> Result<PluginRuntimeStatus, String> {
    let remote = resolve_runtime_manifest().await?;

    if remote.version != OFFICIAL_DENO_VERSION {
        return Err(format!(
            "运行时版本 {} 与锁定的 Deno {LOCKED_DENO_MAJOR} 不匹配",
            remote.version
        ));
    }

    let artifact = remote
        .artifacts
        .get(target)
        .ok_or_else(|| format!("清单中缺少目标 {target} 的构建"))?;

    let root = plugin_runtime_root(app)?;
    ensure_dir(&root)?;
    let archive_path = root.join(format!("deno-{}-{target}.{}", remote.version, "zip"));

    let client = reqwest::Client::new();
    download_with_progress(app, &client, &artifact.url, &archive_path).await?;

    set_progress(
        app,
        RuntimeInstallProgress {
            phase: "verifying".into(),
            message: "正在校验下载文件…".into(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(100),
        },
    );

    let file_bytes = fs::read(&archive_path).map_err(|e| format!("读取下载文件失败: {e}"))?;
    let mut hasher = Sha256::new();
    hasher.update(&file_bytes);
    let actual = hex::encode(hasher.finalize());
    if !actual.eq_ignore_ascii_case(&artifact.sha256) {
        let _ = fs::remove_file(&archive_path);
        return Err(format!(
            "Deno 包校验失败（期望 {}, 实际 {actual}）",
            artifact.sha256
        ));
    }
    drop(file_bytes);

    set_progress(
        app,
        RuntimeInstallProgress {
            phase: "extracting".into(),
            message: "正在解压 Deno 运行时…".into(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(100),
        },
    );

    // Unique staging directory; only publishing the manifest makes this install visible.
    let version_dir = deno_runtime_dir(app, &remote.version)?.join(super::host::generate_id());
    ensure_dir(&version_dir)?;

    extract_deno_archive(&archive_path, &version_dir)?;
    let _ = fs::remove_file(&archive_path);

    let deno_path = resolve_deno_binary(&version_dir);
    if !deno_path.is_file() {
        return Err(format!(
            "解压后未找到 Deno 可执行文件（目录 {}）",
            version_dir.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&deno_path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&deno_path, perms);
        }
    }

    set_progress(
        app,
        RuntimeInstallProgress {
            phase: "finalizing".into(),
            message: "正在写入运行时清单…".into(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: Some(100),
        },
    );

    let local = LocalRuntimeManifest {
        version: remote.version.clone(),
        deno_path: deno_path.display().to_string(),
        package_hash: actual,
        binary_sha256: hex::encode(Sha256::digest(
            fs::read(&deno_path).map_err(|e| e.to_string())?,
        )),
        installed_at: chrono::Local::now().to_rfc3339(),
        target: target.into(),
    };
    let manifest_path = runtime_manifest_path(app)?;
    // Write to a temp file then rename so a crash mid-write never leaves a half manifest
    // that would look "installed" after remount.
    let tmp_path = manifest_path.with_extension("json.tmp");
    fs::write(
        &tmp_path,
        serde_json::to_string_pretty(&local).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入运行时清单失败: {e}"))?;
    fs::rename(&tmp_path, &manifest_path).map_err(|e| format!("发布运行时清单失败: {e}"))?;

    get_plugin_runtime_status(app)
}

pub fn uninstall_plugin_runtime(app: &AppHandle) -> Result<PluginRuntimeStatus, String> {
    let _lock = INSTALL_LOCK
        .try_lock()
        .map_err(|_| "安装进行中，请稍后再卸载")?;
    if current_progress().is_some_and(|p| p.phase != "failed") {
        return Err("安装进行中，请稍后再卸载".into());
    }
    clear_progress();
    let manifest_path = runtime_manifest_path(app)?;
    if manifest_path.exists() {
        fs::remove_file(manifest_path).map_err(|e| e.to_string())?;
    }
    let root = plugin_runtime_root(app)?.join("deno");
    if root.exists() {
        fs::remove_dir_all(&root).map_err(|e| format!("卸载插件运行时失败: {e}"))?;
    }
    get_plugin_runtime_status(app)
}

fn extract_deno_archive(archive: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    extract_zip_archive(archive, dest)
}

fn extract_zip_archive(archive: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("打开压缩包失败: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("读取压缩包失败: {e}"))?;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| format!("读取压缩条目失败: {e}"))?;
        let enclosed = entry.enclosed_name().ok_or("invalid archive path")?;
        let outpath = dest.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&outpath).map_err(|e| format!("创建目录失败: {e}"))?;
            continue;
        }
        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let mut outfile = fs::File::create(&outpath).map_err(|e| format!("写入文件失败: {e}"))?;
        std::io::copy(&mut entry, &mut outfile).map_err(|e| format!("解压文件失败: {e}"))?;
    }
    Ok(())
}

/// Returns the managed Deno executable if installed and present.
pub fn resolved_deno_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = test_deno_path_override() {
        return Ok(path);
    }
    let status = get_plugin_runtime_status(app)?;
    if !status.installed {
        return Err("插件 Deno 运行时缺失或版本不匹配，请重新安装".into());
    }
    let local: LocalRuntimeManifest = serde_json::from_str(
        &fs::read_to_string(runtime_manifest_path(app)?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let actual = hex::encode(Sha256::digest(
        fs::read(&local.deno_path).map_err(|e| e.to_string())?,
    ));
    if actual != local.binary_sha256 {
        return Err("Deno 可执行文件校验失败，请重新安装".into());
    }
    status
        .deno_path
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .ok_or_else(|| "插件运行时未安装".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_manifest_contains_only_pinned_official_archives() {
        let manifest = official_manifest();
        assert_eq!(manifest.version, "2.9.6");
        assert_eq!(manifest.artifacts.len(), 4);
        for (target, artifact) in manifest.artifacts {
            assert_eq!(
                artifact.url,
                format!(
                    "https://github.com/denoland/deno/releases/download/v2.9.6/deno-{target}.zip"
                )
            );
            assert_eq!(artifact.sha256.len(), 64);
            assert!(artifact.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
        }
    }
    #[test]
    fn archive_rejects_traversal() {
        let root = std::env::temp_dir().join(format!(
            "tempo-deno-zip-{}",
            super::super::host::generate_id()
        ));
        fs::create_dir_all(&root).unwrap();
        let archive = root.join("probe.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        zip.start_file("../escaped.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"not executable").unwrap();
        zip.finish().unwrap();
        assert!(extract_zip_archive(&archive, &root.join("out")).is_err());
        assert!(!root.join("escaped.exe").exists());
        fs::remove_dir_all(&root).unwrap();
    }
}
