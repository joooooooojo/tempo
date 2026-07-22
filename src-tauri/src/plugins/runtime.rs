//! On-demand plugin Node runtime (not bundled with Tempo; unrelated to system Node).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use super::paths::{
    ensure_dir, node_runtime_dir, plugin_runtime_root, runtime_manifest_path,
};

/// Locked Node major line for Tempo plugins. Patch is chosen by the download manifest.
pub const LOCKED_NODE_MAJOR: &str = "24";

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

pub fn get_plugin_runtime_status(app: &AppHandle) -> Result<PluginRuntimeStatus, String> {
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

/// Official endpoint for the locked Node build. Overridable later via settings.
fn default_runtime_manifest_url() -> String {
    // Placeholder CDN path — replace with Tempo-hosted manifest before shipping.
    // Format: { version, artifacts: { "<triple>": { url, sha256 } } }
    std::env::var("TEMPO_PLUGIN_RUNTIME_MANIFEST_URL").unwrap_or_else(|_| {
        "https://cdn.example.invalid/tempo/plugin-runtime/node-24.json".into()
    })
}

pub async fn install_plugin_runtime(app: &AppHandle) -> Result<PluginRuntimeStatus, String> {
    let target = current_target_triple();
    if target == "unsupported" {
        return Err("当前平台暂不支持插件运行时".into());
    }

    let manifest_url = default_runtime_manifest_url();
    let client = reqwest::Client::new();
    let remote: RemoteRuntimeManifest = client
        .get(&manifest_url)
        .send()
        .await
        .map_err(|e| format!("下载运行时清单失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("运行时清单 HTTP 错误: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析运行时清单失败: {e}"))?;

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
        // Prefer external tar/Expand-Archive via std::process for MVP without zip crate.
        // On Windows use PowerShell Expand-Archive; elsewhere try `unzip` then `tar`.
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
    let status = get_plugin_runtime_status(app)?;
    status
        .node_path
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .ok_or_else(|| "插件运行时未安装".into())
}
