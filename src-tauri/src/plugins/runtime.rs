//! On-demand plugin Node runtime (not bundled with Tempo; unrelated to system Node).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use super::paths::{ensure_dir, node_runtime_dir, plugin_runtime_root, runtime_manifest_path};

/// Locked Node major line for Tempo plugins. Patch is chosen by the download manifest.
pub const LOCKED_NODE_MAJOR: &str = "24";

/// Official nodejs.org build this Tempo release is locked to (design §3.3.1).
pub const OFFICIAL_NODE_VERSION: &str = "24.18.0";

/// Test-only escape hatch: point directly at a pre-installed Node binary and skip the
/// on-demand download/verify flow entirely. Never read outside of local development/tests.
pub const NODE_PATH_OVERRIDE_ENV: &str = "TEMPO_PLUGIN_NODE_PATH";
/// Override the official runtime manifest URL (mirrors / air-gapped installs / tests).
pub const MANIFEST_URL_OVERRIDE_ENV: &str = "TEMPO_PLUGIN_RUNTIME_MANIFEST_URL";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRuntimeStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub node_path: Option<String>,
    pub install_dir: Option<String>,
    pub locked_major: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalRuntimeManifest {
    version: String,
    node_path: String,
    package_hash: String,
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

/// Hardcoded official Node 24.18.0 artifacts (nodejs.org dist). No third-party CDN is used;
/// `TEMPO_PLUGIN_RUNTIME_MANIFEST_URL` may still override this for mirrors/tests.
fn official_manifest() -> RemoteRuntimeManifest {
    let mut artifacts = std::collections::HashMap::new();
    artifacts.insert(
        "aarch64-apple-darwin".to_string(),
        RemoteArtifact {
            url: format!(
                "https://nodejs.org/dist/v{OFFICIAL_NODE_VERSION}/node-v{OFFICIAL_NODE_VERSION}-darwin-arm64.tar.gz"
            ),
            sha256: "e1a97e14c99c803e96c7339403282ea05a499c32f8d83defe9ef5ec66f979ed1".into(),
        },
    );
    artifacts.insert(
        "x86_64-apple-darwin".to_string(),
        RemoteArtifact {
            url: format!(
                "https://nodejs.org/dist/v{OFFICIAL_NODE_VERSION}/node-v{OFFICIAL_NODE_VERSION}-darwin-x64.tar.gz"
            ),
            sha256: "dfd0dbd3e721503434df7b7205e719f61b3a3a31b2bcf9729b8b91fea240f080".into(),
        },
    );
    artifacts.insert(
        "x86_64-unknown-linux-gnu".to_string(),
        RemoteArtifact {
            url: format!(
                "https://nodejs.org/dist/v{OFFICIAL_NODE_VERSION}/node-v{OFFICIAL_NODE_VERSION}-linux-x64.tar.gz"
            ),
            sha256: "783130984963db7ba9cbd01089eaf2c2efb055c6770827f33aac9d8f26e821".into(),
        },
    );
    artifacts.insert(
        "x86_64-pc-windows-msvc".to_string(),
        RemoteArtifact {
            url: format!(
                "https://nodejs.org/dist/v{OFFICIAL_NODE_VERSION}/node-v{OFFICIAL_NODE_VERSION}-win-x64.zip"
            ),
            sha256: "0ae68406b42d7725661da979b1403ec9926da205c6770827f33aac9d8f26e821".into(),
        },
    );
    RemoteRuntimeManifest {
        version: OFFICIAL_NODE_VERSION.to_string(),
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

fn node_exe_name() -> &'static str {
    if cfg!(windows) {
        "node.exe"
    } else {
        "node"
    }
}

fn resolve_node_binary(install_dir: &std::path::Path) -> PathBuf {
    // Official Node zip/tar layouts differ slightly by platform.
    let candidates = [
        install_dir.join(node_exe_name()),
        install_dir.join("bin").join(node_exe_name()),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            return candidate;
        }
    }
    // Nested folder e.g. node-v24.x.y-win-x64/node.exe
    if let Ok(entries) = fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let nested = [
                    path.join(node_exe_name()),
                    path.join("bin").join(node_exe_name()),
                ];
                for candidate in nested {
                    if candidate.is_file() {
                        return candidate;
                    }
                }
            }
        }
    }
    install_dir.join(node_exe_name())
}

/// Test-only direct override, bypassing install state entirely.
fn test_node_path_override() -> Option<PathBuf> {
    std::env::var(NODE_PATH_OVERRIDE_ENV)
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

pub fn get_plugin_runtime_status(app: &AppHandle) -> Result<PluginRuntimeStatus, String> {
    if let Some(path) = test_node_path_override() {
        return Ok(PluginRuntimeStatus {
            installed: true,
            version: Some(format!("{LOCKED_NODE_MAJOR}.x (test override)")),
            node_path: Some(path.display().to_string()),
            install_dir: path.parent().map(|p| p.display().to_string()),
            locked_major: LOCKED_NODE_MAJOR.into(),
            message: "使用 TEMPO_PLUGIN_NODE_PATH 指定的测试 Node。".into(),
        });
    }

    let manifest_path = runtime_manifest_path(app)?;
    if !manifest_path.is_file() {
        return Ok(PluginRuntimeStatus {
            installed: false,
            version: None,
            node_path: None,
            install_dir: None,
            locked_major: LOCKED_NODE_MAJOR.into(),
            message: "插件运行时未安装。启用含 main 的第三方插件前需要先安装。".into(),
        });
    }

    let raw = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read runtime manifest: {e}"))?;
    let local: LocalRuntimeManifest =
        serde_json::from_str(&raw).map_err(|e| format!("parse runtime manifest: {e}"))?;
    let node_path = PathBuf::from(&local.node_path);
    if !node_path.is_file() {
        return Ok(PluginRuntimeStatus {
            installed: false,
            version: Some(local.version),
            node_path: None,
            install_dir: Some(node_path.parent().map(|p| p.display().to_string()).unwrap_or_default()),
            locked_major: LOCKED_NODE_MAJOR.into(),
            message: "插件运行时清单存在，但 Node 可执行文件缺失，请重新安装。".into(),
        });
    }

    Ok(PluginRuntimeStatus {
        installed: true,
        version: Some(local.version),
        node_path: Some(local.node_path),
        install_dir: node_path
            .parent()
            .map(|p| p.display().to_string()),
        locked_major: LOCKED_NODE_MAJOR.into(),
        message: "插件运行时已就绪。".into(),
    })
}

/// Resolve the manifest to use: hardcoded official build, or an override URL fetched over HTTP
/// (mirrors / air-gapped installs / tests only — never an arbitrary plugin-supplied endpoint).
async fn resolve_runtime_manifest() -> Result<RemoteRuntimeManifest, String> {
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

pub async fn install_plugin_runtime(app: &AppHandle) -> Result<PluginRuntimeStatus, String> {
    let target = current_target_triple();
    if target == "unsupported" {
        return Err("当前平台暂不支持插件运行时".into());
    }

    let remote = resolve_runtime_manifest().await?;

    if !remote.version.starts_with(&format!("{LOCKED_NODE_MAJOR}.")) {
        return Err(format!(
            "运行时版本 {} 与锁定的 Node {LOCKED_NODE_MAJOR} 不匹配",
            remote.version
        ));
    }

    let artifact = remote
        .artifacts
        .get(target)
        .ok_or_else(|| format!("清单中缺少目标 {target} 的构建"))?;

    let client = reqwest::Client::new();
    let bytes = client
        .get(&artifact.url)
        .send()
        .await
        .map_err(|e| format!("下载 Node 失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载 Node HTTP 错误: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("读取 Node 下载内容失败: {e}"))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = hex::encode(hasher.finalize());
    if !actual.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(format!(
            "Node 包校验失败（期望 {}, 实际 {actual}）",
            artifact.sha256
        ));
    }

    let root = plugin_runtime_root(app)?;
    ensure_dir(&root)?;
    let version_dir = node_runtime_dir(app, &remote.version)?;
    if version_dir.exists() {
        fs::remove_dir_all(&version_dir)
            .map_err(|e| format!("清理旧运行时目录失败: {e}"))?;
    }
    ensure_dir(&version_dir)?;

    let archive_path = root.join(format!(
        "node-{}-{target}.{}",
        remote.version,
        if cfg!(windows) { "zip" } else { "tar.gz" }
    ));
    fs::write(&archive_path, &bytes).map_err(|e| format!("写入下载文件失败: {e}"))?;

    extract_node_archive(&archive_path, &version_dir)?;
    let _ = fs::remove_file(&archive_path);

    let node_path = resolve_node_binary(&version_dir);
    if !node_path.is_file() {
        return Err(format!(
            "解压后未找到 Node 可执行文件（目录 {}）",
            version_dir.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&node_path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&node_path, perms);
        }
    }

    let local = LocalRuntimeManifest {
        version: remote.version.clone(),
        node_path: node_path.display().to_string(),
        package_hash: actual,
        installed_at: chrono::Local::now().to_rfc3339(),
        target: target.into(),
    };
    let manifest_path = runtime_manifest_path(app)?;
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&local).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入运行时清单失败: {e}"))?;

    get_plugin_runtime_status(app)
}

pub fn uninstall_plugin_runtime(app: &AppHandle) -> Result<PluginRuntimeStatus, String> {
    let root = plugin_runtime_root(app)?;
    if root.exists() {
        fs::remove_dir_all(&root).map_err(|e| format!("卸载插件运行时失败: {e}"))?;
    }
    get_plugin_runtime_status(app)
}

fn extract_node_archive(archive: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let name = archive
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if name.ends_with(".zip") || cfg!(windows) {
        // Prefer external tar/Expand-Archive via std::process for MVP without a zip dependency
        // for extraction (zip crate is used for plugin-package imports instead).
        #[cfg(windows)]
        {
            let status = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                        archive.display(),
                        dest.display()
                    ),
                ])
                .status()
                .map_err(|e| format!("Expand-Archive 启动失败: {e}"))?;
            if !status.success() {
                return Err("Expand-Archive 解压失败".into());
            }
            return Ok(());
        }
        #[cfg(not(windows))]
        {
            let status = std::process::Command::new("unzip")
                .args(["-o", &archive.display().to_string(), "-d", &dest.display().to_string()])
                .status();
            if let Ok(status) = status {
                if status.success() {
                    return Ok(());
                }
            }
        }
    }

    // .tar.gz / .tar.xz
    let status = std::process::Command::new("tar")
        .args([
            "-xf",
            &archive.display().to_string(),
            "-C",
            &dest.display().to_string(),
        ])
        .status()
        .map_err(|e| format!("tar 解压启动失败: {e}"))?;
    if !status.success() {
        return Err("tar 解压失败".into());
    }
    Ok(())
}

/// Returns the managed Node executable if installed and present.
pub fn resolved_node_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = test_node_path_override() {
        return Ok(path);
    }
    let status = get_plugin_runtime_status(app)?;
    status
        .node_path
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .ok_or_else(|| "插件运行时未安装".into())
}
