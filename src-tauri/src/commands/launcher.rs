use crate::db::AppState;
use chrono::Local;
use parking_lot::RwLock;
use rusqlite::params;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Clone, Serialize)]
pub struct LauncherApp {
    pub id: String,
    pub name: String,
    pub subtitle: String,
    pub keywords: Vec<String>,
    pub icon_data_url: Option<String>,
    pub pinned: bool,
    pub last_used_at: Option<String>,
    pub use_count: i64,
}

#[derive(Debug, Clone)]
struct LauncherRecord {
    id: String,
    name: String,
    subtitle: String,
    keywords: Vec<String>,
    target: String,
    icon_source: Option<String>,
}

#[derive(Default)]
struct LauncherUsage {
    pinned: bool,
    last_used_at: Option<String>,
    use_count: i64,
}

static INDEXING: AtomicBool = AtomicBool::new(false);

fn launcher_cache() -> &'static RwLock<Vec<LauncherRecord>> {
    static CACHE: OnceLock<RwLock<Vec<LauncherRecord>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(Vec::new()))
}

pub fn warm_launcher_index(app: AppHandle) {
    if INDEXING.swap(true, Ordering::AcqRel) {
        return;
    }

    crate::logging::spawn_named("tempo-launcher-index", move || {
        let records = enumerate_launcher_apps();
        publish_launcher_records(&app, &records);
        INDEXING.store(false, Ordering::Release);
    });
}

#[tauri::command]
pub fn get_launcher_apps(app: AppHandle, state: tauri::State<AppState>) -> Vec<LauncherApp> {
    if launcher_cache().read().is_empty() && !INDEXING.load(Ordering::Acquire) {
        warm_launcher_index(app);
    }
    hydrate_launcher_apps(&state, launcher_cache().read().clone())
}

#[tauri::command]
pub fn refresh_launcher_apps(app: AppHandle, state: tauri::State<AppState>) -> Vec<LauncherApp> {
    if INDEXING.swap(true, Ordering::AcqRel) {
        return hydrate_launcher_apps(&state, launcher_cache().read().clone());
    }

    let records = enumerate_launcher_apps();
    publish_launcher_records(&app, &records);
    INDEXING.store(false, Ordering::Release);
    hydrate_launcher_apps(&state, records)
}

#[tauri::command]
pub fn launch_indexed_app(
    app: AppHandle,
    state: tauri::State<AppState>,
    id: String,
) -> Result<(), String> {
    let record = launcher_cache()
        .read()
        .iter()
        .find(|record| record.id == id)
        .cloned()
        .ok_or_else(|| "应用索引已失效，请刷新后重试".to_string())?;

    app.opener()
        .open_path(record.target, None::<String>)
        .map_err(|error| format!("无法启动 {}：{error}", record.name))?;

    touch_launcher_usage(&state, &id)?;
    Ok(())
}

#[tauri::command]
pub fn record_launcher_usage(
    state: tauri::State<AppState>,
    id: String,
) -> Result<(), String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("无效的应用标识".into());
    }
    touch_launcher_usage(&state, id)
}

#[derive(Debug, Clone, Serialize)]
pub struct LauncherUsageItem {
    pub id: String,
    pub pinned: bool,
    pub last_used_at: Option<String>,
    pub use_count: i64,
}

#[tauri::command]
pub fn get_launcher_usage(state: tauri::State<AppState>) -> Vec<LauncherUsageItem> {
    let mut items = load_launcher_usage(&state)
        .into_iter()
        .map(|(id, usage)| LauncherUsageItem {
            id,
            pinned: usage.pinned,
            last_used_at: usage.last_used_at,
            use_count: usage.use_count,
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        parse_usage_timestamp(right.last_used_at.as_deref())
            .cmp(&parse_usage_timestamp(left.last_used_at.as_deref()))
            .then_with(|| right.use_count.cmp(&left.use_count))
            .then_with(|| left.id.cmp(&right.id))
    });
    items
}

fn touch_launcher_usage(state: &tauri::State<AppState>, id: &str) -> Result<(), String> {
    let now = Local::now().to_rfc3339();
    let conn = state.db.lock();
    conn.execute(
        "INSERT INTO launcher_usage (item_id, last_used_at, use_count)
         VALUES (?1, ?2, 1)
         ON CONFLICT(item_id) DO UPDATE SET
           last_used_at = excluded.last_used_at,
           use_count = launcher_usage.use_count + 1",
        params![id, now],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn parse_usage_timestamp(value: Option<&str>) -> Option<chrono::DateTime<chrono::Utc>> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&chrono::Utc))
}

#[tauri::command]
pub fn set_launcher_app_pinned(
    state: tauri::State<AppState>,
    id: String,
    pinned: bool,
) -> Result<(), String> {
    if !launcher_cache().read().iter().any(|record| record.id == id) {
        return Err("应用索引已失效，请刷新后重试".into());
    }

    let pinned_at = pinned.then(|| Local::now().to_rfc3339());
    let conn = state.db.lock();
    conn.execute(
        "INSERT INTO launcher_usage (item_id, pinned_at)
         VALUES (?1, ?2)
         ON CONFLICT(item_id) DO UPDATE SET pinned_at = excluded.pinned_at",
        params![id, pinned_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn hydrate_launcher_apps(
    state: &tauri::State<AppState>,
    records: Vec<LauncherRecord>,
) -> Vec<LauncherApp> {
    let usage = load_launcher_usage(state);
    let mut apps = records
        .into_iter()
        .map(|record| {
            let usage = usage.get(&record.id);
            let icon_data_url = record.icon_source.as_ref().and_then(|source| {
                crate::app_icons::AppIconService::global().icon_url(&record.name, source)
            });
            LauncherApp {
                id: record.id,
                name: record.name,
                subtitle: record.subtitle,
                keywords: record.keywords,
                icon_data_url,
                pinned: usage.is_some_and(|usage| usage.pinned),
                last_used_at: usage.and_then(|usage| usage.last_used_at.clone()),
                use_count: usage.map_or(0, |usage| usage.use_count),
            }
        })
        .collect::<Vec<_>>();

    apps.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.last_used_at.cmp(&left.last_used_at))
            .then_with(|| right.use_count.cmp(&left.use_count))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    apps
}

fn load_launcher_usage(state: &tauri::State<AppState>) -> HashMap<String, LauncherUsage> {
    let conn = state.db.lock();
    let mut statement = match conn
        .prepare("SELECT item_id, pinned_at, last_used_at, use_count FROM launcher_usage")
    {
        Ok(statement) => statement,
        Err(error) => {
            tracing::warn!(error = %error, "failed to prepare launcher usage query");
            return HashMap::new();
        }
    };
    let rows = match statement.query_map([], |row| {
        let pinned_at: Option<String> = row.get(1)?;
        Ok((
            row.get::<_, String>(0)?,
            LauncherUsage {
                pinned: pinned_at.is_some(),
                last_used_at: row.get(2)?,
                use_count: row.get(3)?,
            },
        ))
    }) {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "failed to query launcher usage");
            return HashMap::new();
        }
    };
    rows.filter_map(Result::ok).collect()
}

fn enumerate_launcher_apps() -> Vec<LauncherRecord> {
    let _shell_context = crate::platform::icon_extraction_thread_context();
    let mut records = platform_launcher_apps();
    let mut seen = HashSet::new();
    records.retain(|record| seen.insert(record.id.clone()));
    records.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    records
}

fn publish_launcher_records(app: &AppHandle, records: &[LauncherRecord]) {
    *launcher_cache().write() = records.to_vec();
    crate::logging::debug_if_err(
        app.emit_to("command-palette", "launcher:index-ready", ()),
        "emit launcher index ready",
    );
}

#[cfg(target_os = "windows")]
fn platform_launcher_apps() -> Vec<LauncherRecord> {
    let _shell_context = crate::platform::icon_extraction_thread_context();
    let mut by_name = HashMap::<String, LauncherRecord>::new();
    for (root, max_depth) in windows_launcher_roots() {
        collect_windows_shortcuts(&root, 0, max_depth, &mut by_name);
    }

    for (name, app_id) in windows_start_apps() {
        if !is_launchable_name(&name) {
            continue;
        }
        let key = normalize_name(&name);
        by_name.entry(key).or_insert_with(|| {
            let target = format!("shell:AppsFolder\\{app_id}");
            launcher_record(
                name,
                "Windows 应用",
                target.clone(),
                Some(target),
                vec![app_id],
            )
        });
    }

    by_name.into_values().collect()
}

#[cfg(target_os = "windows")]
fn windows_launcher_roots() -> Vec<(PathBuf, usize)> {
    let mut roots = Vec::new();
    if let Ok(value) = std::env::var("APPDATA") {
        roots.push((
            PathBuf::from(value).join("Microsoft/Windows/Start Menu/Programs"),
            8,
        ));
    }
    if let Ok(value) = std::env::var("PROGRAMDATA") {
        roots.push((
            PathBuf::from(value).join("Microsoft/Windows/Start Menu/Programs"),
            8,
        ));
    }
    if let Ok(value) = std::env::var("USERPROFILE") {
        roots.push((PathBuf::from(value).join("Desktop"), 0));
    }
    if let Ok(value) = std::env::var("PUBLIC") {
        roots.push((PathBuf::from(value).join("Desktop"), 0));
    }
    roots
}

#[cfg(target_os = "windows")]
fn collect_windows_shortcuts(
    root: &Path,
    depth: usize,
    max_depth: usize,
    records: &mut HashMap<String, LauncherRecord>,
) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if depth < max_depth {
                collect_windows_shortcuts(&path, depth + 1, max_depth, records);
            }
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !matches!(
            extension.to_ascii_lowercase().as_str(),
            "lnk" | "exe" | "url"
        ) {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if !is_launchable_name(&name) {
            continue;
        }
        let key = normalize_name(&name);
        let target = path.to_string_lossy().into_owned();
        let (icon_source, keywords) = if extension.eq_ignore_ascii_case("lnk") {
            let shortcut = resolve_windows_shortcut(&path);
            let mut keywords = Vec::new();
            if let Some(target_path) = shortcut
                .as_ref()
                .and_then(|shortcut| shortcut.target_path.clone())
            {
                keywords.push(target_path);
            }
            let icon_source = shortcut
                .and_then(|shortcut| shortcut.icon_source)
                .or_else(|| Some(target.clone()));
            (icon_source, keywords)
        } else {
            (Some(target.clone()), Vec::new())
        };
        records.entry(key).or_insert_with(|| {
            launcher_record(name, "Windows 应用", target, icon_source, keywords)
        });
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct WindowsShortcutMetadata {
    target_path: Option<String>,
    icon_source: Option<String>,
}

#[cfg(target_os = "windows")]
fn resolve_windows_shortcut(path: &Path) -> Option<WindowsShortcutMetadata> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
    use windows::Win32::System::Com::{
        CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};

    let shortcut_path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let persist_file: IPersistFile = shell_link.cast().ok()?;
    unsafe { persist_file.Load(PCWSTR(shortcut_path.as_ptr()), STGM_READ) }.ok()?;

    let mut target_buffer = vec![0u16; 32_768];
    let mut find_data = WIN32_FIND_DATAW::default();
    let target_path =
        unsafe { shell_link.GetPath(&mut target_buffer, &mut find_data, SLGP_RAWPATH.0 as u32) }
            .ok()
            .and_then(|_| windows_wide_buffer_to_string(&target_buffer));

    let mut icon_buffer = vec![0u16; 32_768];
    let mut icon_index = 0;
    let explicit_icon = unsafe { shell_link.GetIconLocation(&mut icon_buffer, &mut icon_index) }
        .ok()
        .and_then(|_| windows_wide_buffer_to_string(&icon_buffer))
        .and_then(|value| normalize_windows_icon_source(&value, target_path.as_deref()));
    let icon_source = explicit_icon
        .or_else(|| {
            target_path
                .clone()
                .filter(|target| Path::new(target).exists())
        })
        .or_else(|| Some(path.to_string_lossy().into_owned()));

    Some(WindowsShortcutMetadata {
        target_path,
        icon_source,
    })
}

#[cfg(target_os = "windows")]
fn windows_wide_buffer_to_string(buffer: &[u16]) -> Option<String> {
    let length = buffer.iter().position(|value| *value == 0)?;
    let value = String::from_utf16_lossy(&buffer[..length]);
    let value = value.trim().trim_matches('"').trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(target_os = "windows")]
fn normalize_windows_icon_source(value: &str, target_path: Option<&str>) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::System::Environment::ExpandEnvironmentStringsW;

    let wide = std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let required = unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), None) };
    let expanded = if required > 1 {
        let mut output = vec![0u16; required as usize];
        let written = unsafe {
            ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), Some(output.as_mut_slice()))
        };
        (written > 0)
            .then(|| String::from_utf16_lossy(&output[..written.saturating_sub(1) as usize]))
    } else {
        None
    }
    .unwrap_or_else(|| value.to_string());

    let path = PathBuf::from(expanded.trim().trim_matches('"'));
    if path.is_absolute() {
        return path.exists().then(|| path.to_string_lossy().into_owned());
    }
    let parent = target_path
        .and_then(|target| Path::new(target).parent())
        .map(Path::to_path_buf)?;
    let resolved = parent.join(path);
    resolved
        .exists()
        .then(|| resolved.to_string_lossy().into_owned())
}

#[cfg(target_os = "windows")]
fn windows_start_apps() -> Vec<(String, String)> {
    let script = "$OutputEncoding=[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; ConvertTo-Json -Compress -InputObject @(Get-StartApps | Select-Object Name,AppID)";
    let output = match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            tracing::debug!(status = ?output.status.code(), "Get-StartApps returned a failure");
            return Vec::new();
        }
        Err(error) => {
            tracing::debug!(error = %error, "failed to run Get-StartApps");
            return Vec::new();
        }
    };
    let value: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(error) => {
            tracing::debug!(error = %error, "failed to decode Get-StartApps output");
            return Vec::new();
        }
    };
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some((
                entry.get("Name")?.as_str()?.trim().to_string(),
                entry.get("AppID")?.as_str()?.trim().to_string(),
            ))
        })
        .filter(|(name, app_id)| !name.is_empty() && !app_id.is_empty())
        .collect()
}

#[cfg(target_os = "macos")]
fn platform_launcher_apps() -> Vec<LauncherRecord> {
    let mut records = HashMap::<String, LauncherRecord>::new();
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    for root in roots {
        collect_macos_apps(&root, 0, &mut records);
    }
    records.into_values().collect()
}

#[cfg(target_os = "macos")]
fn collect_macos_apps(root: &Path, depth: usize, records: &mut HashMap<String, LauncherRecord>) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) == Some("app") {
            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if !is_launchable_name(&name) {
                continue;
            }
            let target = path.to_string_lossy().into_owned();
            records.entry(normalize_name(&name)).or_insert_with(|| {
                launcher_record(name, "macOS 应用", target.clone(), Some(target), Vec::new())
            });
        } else {
            collect_macos_apps(&path, depth + 1, records);
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_launcher_apps() -> Vec<LauncherRecord> {
    Vec::new()
}

fn launcher_record(
    name: String,
    subtitle: &str,
    target: String,
    icon_source: Option<String>,
    mut extra_keywords: Vec<String>,
) -> LauncherRecord {
    extra_keywords.push(name.to_lowercase());
    LauncherRecord {
        id: launcher_id(&target),
        name,
        subtitle: subtitle.into(),
        keywords: extra_keywords,
        target,
        icon_source,
    }
}

fn launcher_id(target: &str) -> String {
    let digest = Sha256::digest(target.as_bytes());
    format!("app:{}", hex::encode(&digest[..12]))
}

fn normalize_name(value: &str) -> String {
    value.trim().to_lowercase()
}

fn is_launchable_name(value: &str) -> bool {
    let normalized = normalize_name(value);
    !normalized.is_empty()
        && !["uninstall", "卸载", "readme", "license", "帮助", "help"]
            .iter()
            .any(|blocked| normalized.contains(blocked))
}

#[cfg(test)]
mod tests {
    use super::{is_launchable_name, launcher_id};

    #[cfg(target_os = "windows")]
    use super::collect_windows_shortcuts;

    #[test]
    fn launcher_ids_are_stable_and_do_not_expose_targets() {
        let id = launcher_id("C:/Program Files/Example/example.exe");
        assert_eq!(id, launcher_id("C:/Program Files/Example/example.exe"));
        assert!(id.starts_with("app:"));
        assert!(!id.contains("Program Files"));
    }

    #[test]
    fn launcher_name_filter_removes_non_app_shortcuts() {
        assert!(is_launchable_name("Visual Studio Code"));
        assert!(!is_launchable_name("Uninstall Visual Studio Code"));
        assert!(!is_launchable_name("卸载应用"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_desktop_scan_does_not_descend_into_folders() {
        let root = std::env::temp_dir().join(format!(
            "tempo-launcher-scan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = root.join("project/node_modules/tool");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("Desktop App.lnk"), []).unwrap();
        std::fs::write(nested.join("Internal Tool.exe"), []).unwrap();

        let mut records = std::collections::HashMap::new();
        collect_windows_shortcuts(&root, 0, 0, &mut records);

        assert_eq!(records.len(), 1);
        assert!(records.values().any(|record| record.name == "Desktop App"));
        assert!(!records
            .values()
            .any(|record| record.name == "Internal Tool"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
