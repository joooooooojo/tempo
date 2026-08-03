//! Detect installed browsers and open http(s) URLs with a chosen browser.

use serde::Serialize;
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Stdio;
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserKind {
    Chrome,
    Edge,
    Firefox,
}

impl BrowserKind {
    fn id(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Firefox => "firefox",
        }
    }

    fn action_name(self) -> &'static str {
        match self {
            Self::Chrome => "用 Chrome 打开",
            Self::Edge => "用 Edge 打开",
            Self::Firefox => "用 Firefox 打开",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Chrome => "Google Chrome",
            Self::Edge => "Microsoft Edge",
            Self::Firefox => "Firefox",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledUrlBrowser {
    pub id: String,
    pub name: String,
    pub action_name: String,
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultUrlBrowser {
    pub name: String,
    pub icon_data_url: Option<String>,
}

/// Browsers available for “open link with …” quick actions (Chrome / Edge / Firefox only).
#[tauri::command]
pub fn list_installed_url_browsers() -> Vec<InstalledUrlBrowser> {
    let mut found = Vec::new();
    for kind in [BrowserKind::Chrome, BrowserKind::Edge, BrowserKind::Firefox] {
        let Some(path) = resolve_browser_target(kind) else {
            continue;
        };
        found.push(InstalledUrlBrowser {
            id: kind.id().into(),
            name: kind.display_name().into(),
            action_name: kind.action_name().into(),
            icon_data_url: icon_data_url_for_path(kind.display_name(), &path),
        });
    }
    found
}

/// System default http(s) browser for the “打开链接” quick action icon.
#[tauri::command]
pub fn get_default_url_browser() -> Option<DefaultUrlBrowser> {
    let path = resolve_default_browser_target()?;
    let name = display_name_for_browser_path(&path)?;
    Some(DefaultUrlBrowser {
        icon_data_url: icon_data_url_for_path(&name, &path),
        name,
    })
}

/// Open an http(s) URL with the system default browser, or a specific installed browser.
#[tauri::command]
pub fn open_url_in_browser(
    app: AppHandle,
    url: String,
    browser_id: Option<String>,
) -> Result<(), String> {
    let url = normalize_http_url(&url)?;
    let browser_id = browser_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match browser_id {
        None => {
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_url(&url, None::<&str>)
                .map_err(|error| format!("无法打开链接：{error}"))
        }
        Some(id) => {
            let kind = parse_browser_id(id)?;
            let target = resolve_browser_target(kind)
                .ok_or_else(|| format!("未找到已安装的 {}", kind.display_name()))?;
            open_url_with_target(&target, &url)
        }
    }
}

fn parse_browser_id(id: &str) -> Result<BrowserKind, String> {
    match id.to_ascii_lowercase().as_str() {
        "chrome" => Ok(BrowserKind::Chrome),
        "edge" => Ok(BrowserKind::Edge),
        "firefox" => Ok(BrowserKind::Firefox),
        _ => Err(format!("不支持的浏览器：{id}")),
    }
}

fn normalize_http_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("链接为空".into());
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err("无效的链接".into());
    }
    let candidate = if trimmed.to_ascii_lowercase().starts_with("www.") {
        format!("https://{trimmed}")
    } else {
        trimmed.to_string()
    };
    let lower = candidate.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("仅支持 http / https 链接".into());
    }
    Ok(candidate)
}

fn icon_data_url_for_path(app_name: &str, path: &Path) -> Option<String> {
    let source = path.to_string_lossy();
    if source.trim().is_empty() {
        return None;
    }
    crate::app_icons::AppIconService::global().icon_url(app_name, source.as_ref())
}

fn resolve_default_browser_target() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return windows_default_browser_target();
    }
    #[cfg(target_os = "macos")]
    {
        return macos_default_browser_target();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        None
    }
}

#[cfg(windows)]
fn windows_default_browser_target() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ, REG_VALUE_TYPE,
    };

    let mut prog_id = [0u16; 256];
    let mut prog_id_size = (prog_id.len() * 2) as u32;
    let mut data_type = REG_VALUE_TYPE::default();
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\http\\UserChoice"),
            w!("ProgId"),
            RRF_RT_REG_SZ,
            Some(&mut data_type),
            Some(prog_id.as_mut_ptr().cast()),
            Some(&mut prog_id_size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (prog_id_size as usize / 2).saturating_sub(1);
    let prog_id = String::from_utf16_lossy(&prog_id[..len]);
    let prog_id_l = prog_id.to_ascii_lowercase();

    let kind = if prog_id_l.contains("chromehtml") || prog_id_l.contains("chrome") {
        BrowserKind::Chrome
    } else if prog_id_l.contains("msedge") || prog_id_l.contains("edge") {
        BrowserKind::Edge
    } else if prog_id_l.contains("firefox") {
        BrowserKind::Firefox
    } else {
        return None;
    };
    resolve_browser_target(kind)
}

#[cfg(target_os = "macos")]
fn macos_default_browser_target() -> Option<PathBuf> {
    use std::process::Command;
    let output = Command::new("swift")
        .args([
            "-e",
            r#"import AppKit
let url = URL(string: "https://example.com")!
if let appURL = NSWorkspace.shared.urlForApplication(toOpen: url) {
    print(appURL.path)
}"#,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let path = PathBuf::from(path);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn display_name_for_browser_path(path: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        return match name.as_str() {
            "chrome.exe" => Some(BrowserKind::Chrome.display_name().into()),
            "msedge.exe" => Some(BrowserKind::Edge.display_name().into()),
            "firefox.exe" => Some(BrowserKind::Firefox.display_name().into()),
            _ => path
                .parent()
                .and_then(|parent| parent.parent())
                .and_then(|parent| parent.file_name())
                .and_then(|value| value.to_str())
                .map(str::to_string),
        };
    }
    #[cfg(target_os = "macos")]
    {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .trim_end_matches(".app")
            .to_ascii_lowercase();
        return match name.as_str() {
            "google chrome" => Some(BrowserKind::Chrome.display_name().into()),
            "microsoft edge" => Some(BrowserKind::Edge.display_name().into()),
            "firefox" => Some(BrowserKind::Firefox.display_name().into()),
            _ if !name.is_empty() => Some(
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or(&name)
                    .to_string(),
            ),
            _ => None,
        };
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = path;
        None
    }
}

fn resolve_browser_target(kind: BrowserKind) -> Option<PathBuf> {
    if let Some(path) = known_browser_paths(kind).into_iter().find(|path| path.exists()) {
        return Some(path);
    }
    browser_target_from_launcher(kind)
}

fn browser_target_from_launcher(kind: BrowserKind) -> Option<PathBuf> {
    let cache = crate::commands::launcher::launcher_cache_records();
    let mut best: Option<(i32, PathBuf)> = None;
    for record in cache {
        let score = browser_match_score(kind, &record.name, &record.target, &record.keywords);
        if score <= 0 {
            continue;
        }
        let path = PathBuf::from(&record.target);
        if path.exists() && is_launchable_browser_binary(kind, &path) {
            best = Some(pick_better(best, score, path));
            continue;
        }
        // Shortcut targets often store the .exe path in keywords.
        if let Some(exe) = record
            .keywords
            .iter()
            .map(PathBuf::from)
            .find(|candidate| candidate.exists() && is_launchable_browser_binary(kind, candidate))
        {
            best = Some(pick_better(best, score + 10, exe));
        }
    }
    best.map(|(_, path)| path)
}

fn pick_better(current: Option<(i32, PathBuf)>, score: i32, path: PathBuf) -> (i32, PathBuf) {
    match current {
        Some((prev, prev_path)) if prev >= score => (prev, prev_path),
        _ => (score, path),
    }
}

fn is_launchable_browser_binary(kind: BrowserKind, path: &Path) -> bool {
    #[cfg(windows)]
    {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match kind {
            BrowserKind::Chrome => name == "chrome.exe",
            BrowserKind::Edge => name == "msedge.exe",
            BrowserKind::Firefox => name == "firefox.exe",
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = kind;
        path.extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
            || path.is_dir()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = (kind, path);
        false
    }
}

fn browser_match_score(
    kind: BrowserKind,
    name: &str,
    target: &str,
    keywords: &[String],
) -> i32 {
    let name_l = name.to_ascii_lowercase();
    let target_l = target.to_ascii_lowercase();
    let joined = format!(
        "{name_l} {target_l} {}",
        keywords.join(" ").to_ascii_lowercase()
    );

    const BLOCKED: &[&str] = &[
        "remote",
        "canary",
        "beta",
        "dev",
        "nightly",
        "chromium",
        "webdriver",
        "uninstall",
        "卸载",
    ];
    if BLOCKED.iter().any(|blocked| name_l.contains(blocked)) {
        return 0;
    }

    match kind {
        BrowserKind::Chrome => {
            if target_l.ends_with("chrome.exe")
                || target_l.contains(r"\google\chrome\application\chrome.exe")
                || target_l.contains("google chrome.app")
            {
                return 100;
            }
            if name_l == "google chrome" || name_l == "chrome" {
                return 80;
            }
            if joined.contains("google chrome") && !joined.contains("chromium") {
                return 40;
            }
            0
        }
        BrowserKind::Edge => {
            if target_l.ends_with("msedge.exe")
                || target_l.contains(r"\microsoft\edge\application\msedge.exe")
                || target_l.contains("microsoft edge.app")
            {
                return 100;
            }
            if name_l == "microsoft edge" || name_l == "edge" {
                return 80;
            }
            if joined.contains("microsoft edge") || joined.contains("msedge") {
                return 40;
            }
            0
        }
        BrowserKind::Firefox => {
            if target_l.ends_with("firefox.exe")
                || target_l.contains(r"\mozilla firefox\firefox.exe")
                || target_l.contains("firefox.app")
            {
                return 100;
            }
            if name_l == "firefox" || name_l == "mozilla firefox" {
                return 80;
            }
            if joined.contains("firefox") {
                return 40;
            }
            0
        }
    }
}

fn known_browser_paths(kind: BrowserKind) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
        let program_files_x86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from);
        let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let mut paths = Vec::new();

        match kind {
            BrowserKind::Chrome => {
                for root in [&program_files, &program_files_x86, &local_app_data]
                    .into_iter()
                    .flatten()
                {
                    paths.push(root.join(r"Google\Chrome\Application\chrome.exe"));
                }
            }
            BrowserKind::Edge => {
                for root in [&program_files, &program_files_x86, &local_app_data]
                    .into_iter()
                    .flatten()
                {
                    paths.push(root.join(r"Microsoft\Edge\Application\msedge.exe"));
                }
            }
            BrowserKind::Firefox => {
                for root in [&program_files, &program_files_x86].into_iter().flatten() {
                    paths.push(root.join(r"Mozilla Firefox\firefox.exe"));
                }
            }
        }
        paths
    }
    #[cfg(target_os = "macos")]
    {
        match kind {
            BrowserKind::Chrome => vec![PathBuf::from("/Applications/Google Chrome.app")],
            BrowserKind::Edge => vec![PathBuf::from("/Applications/Microsoft Edge.app")],
            BrowserKind::Firefox => vec![PathBuf::from("/Applications/Firefox.app")],
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = kind;
        Vec::new()
    }
}

fn open_url_with_target(target: &Path, url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows::Win32::System::Threading::CREATE_NO_WINDOW;

        std::process::Command::new(target)
            .arg(url)
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法用浏览器打开：{error}"))
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-a")
            .arg(target)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法用浏览器打开：{error}"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = (target, url);
        Err("当前平台暂不支持指定浏览器打开".into())
    }
}
