//! Detect installed browsers (system http handlers) and open URLs with a chosen one.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Stdio;
use tauri::AppHandle;

#[derive(Debug, Clone)]
struct DiscoveredBrowser {
    id: String,
    name: String,
    path: PathBuf,
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

/// All installed http(s) browsers discovered from the OS (StartMenuInternet / LS handlers).
#[tauri::command]
pub fn list_installed_url_browsers() -> Vec<InstalledUrlBrowser> {
    discover_installed_browsers()
        .into_iter()
        .map(|browser| InstalledUrlBrowser {
            action_name: format!("用 {} 打开", short_action_label(&browser.name)),
            icon_data_url: icon_data_url_for_path(&browser.name, &browser.path),
            id: browser.id,
            name: browser.name,
        })
        .collect()
}

/// System default http(s) browser for the “打开链接” quick action icon.
#[tauri::command]
pub fn get_default_url_browser() -> Option<DefaultUrlBrowser> {
    let path = resolve_default_browser_path()?;
    let browsers = discover_installed_browsers();
    let matched = browsers.iter().find(|browser| paths_equivalent(&browser.path, &path));
    let name = matched
        .map(|browser| browser.name.clone())
        .or_else(|| display_name_for_browser_path(&path))?;
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
            let browsers = discover_installed_browsers();
            let browser = browsers
                .into_iter()
                .find(|browser| browser.id == id)
                .ok_or_else(|| format!("未找到已安装的浏览器：{id}"))?;
            open_url_with_target(&browser.path, &url)
        }
    }
}

fn discover_installed_browsers() -> Vec<DiscoveredBrowser> {
    #[cfg(windows)]
    {
        return finalize_browsers(discover_browsers_windows());
    }
    #[cfg(target_os = "macos")]
    {
        return finalize_browsers(discover_browsers_macos());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Vec::new()
    }
}

fn finalize_browsers(mut browsers: Vec<DiscoveredBrowser>) -> Vec<DiscoveredBrowser> {
    // Prefer first occurrence of each real binary path.
    let mut seen_paths = HashSet::new();
    browsers.retain(|browser| {
        let key = normalize_path_key(&browser.path);
        seen_paths.insert(key)
    });

    // Ensure stable unique ids.
    let mut used_ids = HashSet::new();
    for browser in &mut browsers {
        let mut id = browser.id.clone();
        if !used_ids.insert(id.clone()) {
            let mut suffix = 2u32;
            loop {
                let candidate = format!("{}-{suffix}", browser.id);
                if used_ids.insert(candidate.clone()) {
                    id = candidate;
                    break;
                }
                suffix += 1;
            }
            browser.id = id;
        }
    }

    browsers.sort_by(|a, b| {
        browser_sort_rank(&a.id, &a.name)
            .cmp(&browser_sort_rank(&b.id, &b.name))
            .then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))
    });
    browsers
}

fn browser_sort_rank(id: &str, name: &str) -> u8 {
    match id {
        "chrome" => 0,
        "edge" => 1,
        "firefox" => 2,
        "safari" => 3,
        "brave" => 4,
        "opera" => 5,
        "vivaldi" => 6,
        "arc" => 7,
        _ => {
            let lower = name.to_ascii_lowercase();
            if lower.contains("chrome") {
                10
            } else if lower.contains("edge") {
                11
            } else if lower.contains("firefox") {
                12
            } else {
                50
            }
        }
    }
}

fn short_action_label(name: &str) -> String {
    let trimmed = name.trim();
    for prefix in ["Google ", "Microsoft ", "Mozilla "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    trimmed.to_string()
}

fn preferred_browser_id(name: &str, path: &Path) -> String {
    let name_l = name.to_ascii_lowercase();
    let path_l = path.to_string_lossy().to_ascii_lowercase();
    let file = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if file == "chrome.exe"
        || path_l.contains("google\\chrome\\")
        || path_l.contains("google chrome.app")
        || name_l == "google chrome"
    {
        return "chrome".into();
    }
    if file == "msedge.exe"
        || path_l.contains("microsoft\\edge\\")
        || path_l.contains("microsoft edge.app")
        || name_l == "microsoft edge"
        || name_l == "edge"
    {
        return "edge".into();
    }
    if file == "firefox.exe"
        || path_l.contains("mozilla firefox")
        || path_l.contains("firefox.app")
        || name_l == "firefox"
        || name_l == "mozilla firefox"
    {
        return "firefox".into();
    }
    if file == "brave.exe"
        || path_l.contains("brave-browser")
        || path_l.contains("brave browser.app")
        || name_l.contains("brave")
    {
        return "brave".into();
    }
    if file == "opera.exe" || path_l.contains("opera") || name_l.contains("opera") {
        return "opera".into();
    }
    if file == "vivaldi.exe" || path_l.contains("vivaldi") || name_l.contains("vivaldi") {
        return "vivaldi".into();
    }
    if path_l.contains("arc.app") || name_l == "arc" {
        return "arc".into();
    }
    if path_l.contains("safari.app") || name_l == "safari" {
        return "safari".into();
    }

    slugify_id(name)
}

fn slugify_id(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "browser".into()
    } else {
        trimmed
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

fn normalize_path_key(path: &Path) -> String {
    let lossy = path.to_string_lossy();
    #[cfg(windows)]
    {
        return lossy.replace('/', "\\").to_ascii_lowercase();
    }
    #[cfg(not(windows))]
    {
        lossy.to_string()
    }
}

fn paths_equivalent(a: &Path, b: &Path) -> bool {
    normalize_path_key(a) == normalize_path_key(b)
}

fn resolve_default_browser_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        return windows_default_browser_path();
    }
    #[cfg(target_os = "macos")]
    {
        return macos_default_browser_path();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
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
            "chrome.exe" => Some("Google Chrome".into()),
            "msedge.exe" => Some("Microsoft Edge".into()),
            "firefox.exe" => Some("Firefox".into()),
            "brave.exe" => Some("Brave".into()),
            "opera.exe" => Some("Opera".into()),
            "vivaldi.exe" => Some("Vivaldi".into()),
            _ => path
                .parent()
                .and_then(|parent| parent.parent())
                .and_then(|parent| parent.file_name())
                .and_then(|value| value.to_str())
                .map(str::to_string)
                .filter(|value| !value.is_empty()),
        };
    }
    #[cfg(target_os = "macos")]
    {
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        Some(stem.to_string())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = path;
        None
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

// --- Windows: Clients\StartMenuInternet + ProgId ---

#[cfg(windows)]
fn discover_browsers_windows() -> Vec<DiscoveredBrowser> {
    use windows::core::w;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let mut browsers = Vec::new();
    let roots = [
        (
            HKEY_CURRENT_USER,
            w!("Software\\Clients\\StartMenuInternet"),
        ),
        (
            HKEY_LOCAL_MACHINE,
            w!("Software\\Clients\\StartMenuInternet"),
        ),
        (
            HKEY_LOCAL_MACHINE,
            w!("Software\\WOW6432Node\\Clients\\StartMenuInternet"),
        ),
    ];

    for (hive, subkey) in roots {
        collect_start_menu_internet(hive, subkey, &mut browsers);
    }

    browsers
}

#[cfg(windows)]
fn collect_start_menu_internet(
    hive: windows::Win32::System::Registry::HKEY,
    subkey: windows::core::PCWSTR,
    out: &mut Vec<DiscoveredBrowser>,
) {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, KEY_READ,
    };

    let mut key = windows::Win32::System::Registry::HKEY::default();
    let status = unsafe { RegOpenKeyExW(hive, subkey, 0, KEY_READ, &mut key) };
    if status != ERROR_SUCCESS {
        return;
    }

    let mut index = 0u32;
    loop {
        let mut name_buf = [0u16; 512];
        let mut name_len = name_buf.len() as u32;
        let enum_status = unsafe {
            RegEnumKeyExW(
                key,
                index,
                windows::core::PWSTR(name_buf.as_mut_ptr()),
                &mut name_len,
                None,
                windows::core::PWSTR::null(),
                None,
                None,
            )
        };
        if enum_status != ERROR_SUCCESS {
            break;
        }
        index += 1;
        let client_key = String::from_utf16_lossy(&name_buf[..name_len as usize]);
        if client_key.trim().is_empty() {
            continue;
        }
        if let Some(browser) = browser_from_start_menu_client(key, &client_key) {
            out.push(browser);
        }
    }

    unsafe {
        let _ = RegCloseKey(key);
    }
}

#[cfg(windows)]
fn browser_from_start_menu_client(
    parent: windows::Win32::System::Registry::HKEY,
    client_key: &str,
) -> Option<DiscoveredBrowser> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, KEY_READ};

    let sub = HSTRING::from(client_key);
    let mut key = windows::Win32::System::Registry::HKEY::default();
    let status = unsafe { RegOpenKeyExW(parent, PCWSTR(sub.as_ptr()), 0, KEY_READ, &mut key) };
    if status != ERROR_SUCCESS {
        return None;
    }

    let name = reg_get_default_string(key)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| client_key.to_string());
    let command = reg_get_string(key, r"shell\open\command", "");
    unsafe {
        let _ = RegCloseKey(key);
    }

    let path = parse_command_executable(&command?)?;
    if !path.exists() || !is_windows_browser_executable(&path) {
        return None;
    }
    if looks_like_browser_uninstall_or_helper(&name, &path) {
        return None;
    }
    // Internet Explorer is a stub that forwards to Edge on modern Windows.
    if name.eq_ignore_ascii_case("internet explorer")
        || path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|file| file.eq_ignore_ascii_case("iexplore.exe"))
    {
        return None;
    }

    Some(DiscoveredBrowser {
        id: preferred_browser_id(&name, &path),
        name,
        path,
    })
}

#[cfg(windows)]
fn windows_default_browser_path() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let prog_id = reg_read_sz(
        HKEY_CURRENT_USER,
        w!("Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\http\\UserChoice"),
        w!("ProgId"),
    )?;
    if prog_id.trim().is_empty() {
        return None;
    }

    let prog_id_key = format!(r"Software\Classes\{prog_id}\shell\open\command");
    let command = reg_read_sz_hstring(HKEY_CURRENT_USER, &prog_id_key, "")
        .or_else(|| reg_read_sz_hstring(HKEY_LOCAL_MACHINE, &prog_id_key, ""))?;
    let path = parse_command_executable(&command)?;
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[cfg(windows)]
fn reg_read_sz(
    hive: windows::Win32::System::Registry::HKEY,
    subkey: windows::core::PCWSTR,
    value: windows::core::PCWSTR,
) -> Option<String> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ, REG_VALUE_TYPE};

    let mut buf = [0u16; 512];
    let mut size = (buf.len() * 2) as u32;
    let mut data_type = REG_VALUE_TYPE::default();
    let status = unsafe {
        RegGetValueW(
            hive,
            subkey,
            value,
            RRF_RT_REG_SZ,
            Some(&mut data_type),
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(windows)]
fn reg_read_sz_hstring(
    hive: windows::Win32::System::Registry::HKEY,
    subkey: &str,
    value_name: &str,
) -> Option<String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ, REG_VALUE_TYPE};

    let sub = HSTRING::from(subkey);
    let value = HSTRING::from(value_name);
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let mut data_type = REG_VALUE_TYPE::default();
    let status = unsafe {
        RegGetValueW(
            hive,
            PCWSTR(sub.as_ptr()),
            if value_name.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(value.as_ptr())
            },
            RRF_RT_REG_SZ,
            Some(&mut data_type),
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(windows)]
fn reg_get_default_string(key: windows::Win32::System::Registry::HKEY) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ, REG_VALUE_TYPE};

    let mut buf = [0u16; 512];
    let mut size = (buf.len() * 2) as u32;
    let mut data_type = REG_VALUE_TYPE::default();
    let status = unsafe {
        RegGetValueW(
            key,
            PCWSTR::null(),
            PCWSTR::null(),
            RRF_RT_REG_SZ,
            Some(&mut data_type),
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(windows)]
fn reg_get_string(
    key: windows::Win32::System::Registry::HKEY,
    relative_subkey: &str,
    value_name: &str,
) -> Option<String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ, REG_VALUE_TYPE};

    let sub = HSTRING::from(relative_subkey);
    let value = HSTRING::from(value_name);
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let mut data_type = REG_VALUE_TYPE::default();
    let status = unsafe {
        RegGetValueW(
            key,
            PCWSTR(sub.as_ptr()),
            if value_name.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(value.as_ptr())
            },
            RRF_RT_REG_SZ,
            Some(&mut data_type),
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

#[cfg(windows)]
fn parse_command_executable(command: &str) -> Option<PathBuf> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('"') {
        let end = rest.find('"')?;
        let path = PathBuf::from(&rest[..end]);
        return Some(path);
    }
    let token = trimmed.split_whitespace().next()?;
    Some(PathBuf::from(token))
}

#[cfg(windows)]
fn is_windows_browser_executable(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

#[cfg(windows)]
fn looks_like_browser_uninstall_or_helper(name: &str, path: &Path) -> bool {
    let hay = format!(
        "{} {}",
        name.to_ascii_lowercase(),
        path.to_string_lossy().to_ascii_lowercase()
    );
    const BLOCKED: &[&str] = &[
        "uninstall",
        "卸载",
        "webdriver",
        "crashpad",
        "helper",
        "notification_helper",
        "elevation_service",
    ];
    BLOCKED.iter().any(|blocked| hay.contains(blocked))
}

// --- macOS: NSWorkspace URL handlers ---

#[cfg(target_os = "macos")]
fn discover_browsers_macos() -> Vec<DiscoveredBrowser> {
    let mut browsers = discover_browsers_macos_workspace();
    if browsers.is_empty() {
        browsers = discover_browsers_macos_fallback_paths();
    }
    browsers
}

#[cfg(target_os = "macos")]
fn discover_browsers_macos_workspace() -> Vec<DiscoveredBrowser> {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};
    use std::ffi::CStr;

    unsafe {
        let workspace_cls = match Class::get("NSWorkspace") {
            Some(cls) => cls,
            None => return Vec::new(),
        };
        let url_cls = match Class::get("NSURL") {
            Some(cls) => cls,
            None => return Vec::new(),
        };
        let nsstring_cls = match Class::get("NSString") {
            Some(cls) => cls,
            None => return Vec::new(),
        };

        let workspace: *mut Object = msg_send![workspace_cls, sharedWorkspace];
        if workspace.is_null() {
            return Vec::new();
        }

        let probe: *mut Object = msg_send![nsstring_cls, stringWithUTF8String: b"https://example.com\0".as_ptr()];
        if probe.is_null() {
            return Vec::new();
        }
        let url: *mut Object = msg_send![url_cls, URLWithString: probe];
        if url.is_null() {
            return Vec::new();
        }

        // macOS 12+: URLsForApplicationsToOpenURL:
        let apps: *mut Object = msg_send![workspace, URLsForApplicationsToOpenURL: url];
        if apps.is_null() {
            return Vec::new();
        }

        let count: usize = msg_send![apps, count];
        let mut browsers = Vec::with_capacity(count);
        for index in 0..count {
            let item: *mut Object = msg_send![apps, objectAtIndex: index];
            if item.is_null() {
                continue;
            }
            let path_ns: *mut Object = msg_send![item, path];
            if path_ns.is_null() {
                continue;
            }
            let utf8: *const std::os::raw::c_char = msg_send![path_ns, UTF8String];
            if utf8.is_null() {
                continue;
            }
            let path = PathBuf::from(CStr::from_ptr(utf8).to_string_lossy().as_ref());
            if !path.exists() {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Browser")
                .to_string();
            if looks_like_macos_non_browser(&name, &path) {
                continue;
            }
            browsers.push(DiscoveredBrowser {
                id: preferred_browser_id(&name, &path),
                name,
                path,
            });
        }
        browsers
    }
}

#[cfg(target_os = "macos")]
fn discover_browsers_macos_fallback_paths() -> Vec<DiscoveredBrowser> {
    const CANDIDATES: &[(&str, &str)] = &[
        ("Safari", "/Applications/Safari.app"),
        ("Google Chrome", "/Applications/Google Chrome.app"),
        ("Microsoft Edge", "/Applications/Microsoft Edge.app"),
        ("Firefox", "/Applications/Firefox.app"),
        ("Brave Browser", "/Applications/Brave Browser.app"),
        ("Opera", "/Applications/Opera.app"),
        ("Vivaldi", "/Applications/Vivaldi.app"),
        ("Arc", "/Applications/Arc.app"),
    ];
    let mut browsers = Vec::new();
    for (name, path) in CANDIDATES {
        let path = PathBuf::from(path);
        if path.exists() {
            browsers.push(DiscoveredBrowser {
                id: preferred_browser_id(name, &path),
                name: (*name).into(),
                path,
            });
        }
    }
    browsers
}

#[cfg(target_os = "macos")]
fn looks_like_macos_non_browser(name: &str, path: &Path) -> bool {
    let hay = format!(
        "{} {}",
        name.to_ascii_lowercase(),
        path.to_string_lossy().to_ascii_lowercase()
    );
    const BLOCKED: &[&str] = &[
        "helper",
        "webdriver",
        "automator",
        "script editor",
        "terminal",
        "iterm",
        "tempo",
    ];
    BLOCKED.iter().any(|blocked| hay.contains(blocked))
}

#[cfg(target_os = "macos")]
fn macos_default_browser_path() -> Option<PathBuf> {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};
    use std::ffi::CStr;

    unsafe {
        let workspace_cls = Class::get("NSWorkspace")?;
        let url_cls = Class::get("NSURL")?;
        let nsstring_cls = Class::get("NSString")?;
        let workspace: *mut Object = msg_send![workspace_cls, sharedWorkspace];
        if workspace.is_null() {
            return None;
        }
        let probe: *mut Object =
            msg_send![nsstring_cls, stringWithUTF8String: b"https://example.com\0".as_ptr()];
        if probe.is_null() {
            return None;
        }
        let url: *mut Object = msg_send![url_cls, URLWithString: probe];
        if url.is_null() {
            return None;
        }
        let app_url: *mut Object = msg_send![workspace, URLForApplicationToOpenURL: url];
        if app_url.is_null() {
            // Fallback for older selectors via swift helper used previously.
            return macos_default_browser_path_via_swift();
        }
        let path_ns: *mut Object = msg_send![app_url, path];
        if path_ns.is_null() {
            return None;
        }
        let utf8: *const std::os::raw::c_char = msg_send![path_ns, UTF8String];
        if utf8.is_null() {
            return None;
        }
        let path = PathBuf::from(CStr::from_ptr(utf8).to_string_lossy().as_ref());
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_default_browser_path_via_swift() -> Option<PathBuf> {
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
    path.exists().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::{parse_command_executable, preferred_browser_id, short_action_label, slugify_id};
    use std::path::PathBuf;

    #[cfg(windows)]
    #[test]
    fn parses_quoted_windows_command() {
        let path = parse_command_executable(
            r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --single-argument %1"#,
        )
        .expect("path");
        assert!(path.ends_with(r"chrome.exe"));
    }

    #[test]
    fn prefers_well_known_ids() {
        assert_eq!(
            preferred_browser_id(
                "Google Chrome",
                &PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
            ),
            "chrome"
        );
        assert_eq!(
            preferred_browser_id("Brave", &PathBuf::from(r"C:\Brave\brave.exe")),
            "brave"
        );
    }

    #[test]
    fn slugifies_unknown_names() {
        assert_eq!(slugify_id("My Cool Browser"), "my-cool-browser");
        assert_eq!(short_action_label("Google Chrome"), "Chrome");
        assert_eq!(short_action_label("Microsoft Edge"), "Edge");
    }
}
