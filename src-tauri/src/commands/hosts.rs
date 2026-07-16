use crate::db::{current_storage_dir, default_storage_dir, AppState};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::AppHandle;

const MAX_BACKUPS: usize = 20;
const PROFILES_META: &str = "profiles.json";
const STATE_FILE: &str = "state.json";
const PUBLIC_FILE: &str = "public.hosts";

const MARK_PUBLIC_BEGIN: &str = "# >>> TEMPO:PUBLIC:BEGIN";
const MARK_PUBLIC_END: &str = "# <<< TEMPO:PUBLIC:END";
const MARK_PROFILE_BEGIN_PREFIX: &str = "# >>> TEMPO:PROFILE:BEGIN";
const MARK_PROFILE_END: &str = "# <<< TEMPO:PROFILE:END";

/// Built-in custom environments seeded on first use.
const DEFAULT_PROFILES: &[(&str, &str)] = &[
    ("env-dev", "开发环境"),
    ("env-test", "测试环境"),
    ("env-prod", "生产环境"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsWorkspace {
    pub path: String,
    pub writable: bool,
    pub authorized: bool,
    /// Whether the on-disk system hosts contains Tempo section markers.
    pub managed: bool,
    pub public_content: String,
    pub active_profile_id: Option<String>,
    pub profiles: Vec<HostsProfile>,
    /// Exact content currently on the system hosts file.
    pub system_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsProfile {
    pub id: String,
    pub name: String,
    pub updated_at: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsBackup {
    pub id: String,
    pub source: String,
    pub created_at: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct HostsState {
    #[serde(default, alias = "activeProfileId")]
    active_profile_id: Option<String>,
    /// True after we have bootstrapped public.hosts at least once.
    #[serde(default)]
    initialized: bool,
    /// True after the three default environments were seeded once (or skipped for existing installs).
    #[serde(default)]
    defaults_seeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfilesFile {
    profiles: Vec<ProfileMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfileMeta {
    id: String,
    name: String,
    updated_at: String,
}

#[derive(Debug, Clone, Default)]
struct ParsedSystemHosts {
    managed: bool,
    public: String,
    profile_id: Option<String>,
    profile_content: String,
}

fn hosts_path() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

fn tools_hosts_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = current_storage_dir(app).or_else(|_| default_storage_dir(app))?;
    let dir = base.join("tools").join("hosts");
    fs::create_dir_all(dir.join("profiles")).map_err(|e| format!("创建 hosts 目录失败: {e}"))?;
    fs::create_dir_all(dir.join("backups")).map_err(|e| format!("创建备份目录失败: {e}"))?;
    Ok(dir)
}

fn is_writable(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if meta.permissions().readonly() {
        return false;
    }
    fs::OpenOptions::new().append(true).open(path).is_ok()
}

fn read_hosts_content(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("读取 hosts 失败: {e}"))
}

fn normalize_section(content: &str) -> String {
    content.trim_matches(['\r', '\n']).to_string()
}

fn validate_hosts_content(content: &str) -> Result<(), String> {
    for (index, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(ip) = parts.next() else {
            continue;
        };
        if !looks_like_ip(ip) {
            return Err(format!("第 {} 行：无效 IP「{}」", index + 1, ip));
        }
        let hostnames: Vec<_> = parts.collect();
        if hostnames.is_empty() {
            return Err(format!("第 {} 行：缺少主机名", index + 1));
        }
        for host in hostnames {
            if !looks_like_hostname(host) {
                return Err(format!("第 {} 行：无效主机名「{}」", index + 1, host));
            }
        }
    }
    Ok(())
}

fn looks_like_ip(value: &str) -> bool {
    if value.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    value.parse::<std::net::Ipv6Addr>().is_ok()
}

fn looks_like_hostname(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 {
        return false;
    }
    value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_')
}

fn preview_text(content: &str) -> String {
    let flat = content.lines().take(2).collect::<Vec<_>>().join(" · ");
    if flat.chars().count() > 80 {
        format!("{}…", flat.chars().take(80).collect::<String>())
    } else if flat.is_empty() {
        "(空)".into()
    } else {
        flat
    }
}

/// Parse Tempo-managed sections from a system hosts file.
///
/// Format:
/// ```text
/// # >>> TEMPO:PUBLIC:BEGIN
/// ...public...
/// # <<< TEMPO:PUBLIC:END
/// # >>> TEMPO:PROFILE:BEGIN id=<id>
/// ...profile...
/// # <<< TEMPO:PROFILE:END
/// ```
fn parse_system_hosts(content: &str) -> ParsedSystemHosts {
    let lines: Vec<&str> = content.lines().collect();
    let mut public_start: Option<usize> = None;
    let mut public_end: Option<usize> = None;
    let mut profile_start: Option<usize> = None;
    let mut profile_end: Option<usize> = None;
    let mut profile_id: Option<String> = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == MARK_PUBLIC_BEGIN {
            public_start = Some(idx);
        } else if trimmed == MARK_PUBLIC_END {
            public_end = Some(idx);
        } else if let Some(rest) = trimmed.strip_prefix(MARK_PROFILE_BEGIN_PREFIX) {
            profile_start = Some(idx);
            profile_id = rest
                .split_whitespace()
                .find_map(|part| part.strip_prefix("id="))
                .map(|s| s.to_string());
        } else if trimmed == MARK_PROFILE_END {
            profile_end = Some(idx);
        }
    }

    let managed = public_start.is_some() && public_end.is_some();
    if !managed {
        return ParsedSystemHosts {
            managed: false,
            public: normalize_section(content),
            profile_id: None,
            profile_content: String::new(),
        };
    }

    let ps = public_start.unwrap();
    let pe = public_end.unwrap();
    let public = if pe > ps + 1 {
        lines[ps + 1..pe].join("\n")
    } else {
        String::new()
    };

    let mut profile_content = String::new();
    if let (Some(p_start), Some(p_end)) = (profile_start, profile_end) {
        if p_end > p_start + 1 {
            profile_content = lines[p_start + 1..p_end].join("\n");
        }
    } else {
        profile_id = None;
    }

    // Content outside Tempo markers (e.g. manual edits) is merged into public
    // so it is not lost on the next apply. Tempo banner comments are ignored.
    let mut outside: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if Some(i) == public_start {
            i = pe + 1;
            continue;
        }
        if profile_start == Some(i) {
            if let Some(p_end) = profile_end {
                i = p_end + 1;
                continue;
            }
        }
        let trimmed = lines[i].trim();
        let is_tempo_banner = trimmed.starts_with("# Managed by Tempo")
            || trimmed.starts_with("# >>> TEMPO:")
            || trimmed.starts_with("# <<< TEMPO:");
        if !trimmed.is_empty() && !is_tempo_banner {
            outside.push(lines[i]);
        }
        i += 1;
    }
    let outside_text = normalize_section(&outside.join("\n"));
    let public = if outside_text.is_empty() {
        normalize_section(&public)
    } else if public.trim().is_empty() {
        outside_text
    } else {
        normalize_section(&format!("{outside_text}\n\n{}", normalize_section(&public)))
    };

    ParsedSystemHosts {
        managed: true,
        public,
        profile_id,
        profile_content: normalize_section(&profile_content),
    }
}

fn compose_system_hosts(public: &str, active_id: Option<&str>, profile_content: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("# Managed by Tempo. Keep the marker lines so public/custom sections can be parsed.\n");
    out.push_str(MARK_PUBLIC_BEGIN);
    out.push('\n');
    let public = normalize_section(public);
    if !public.is_empty() {
        out.push_str(&public);
        out.push('\n');
    }
    out.push_str(MARK_PUBLIC_END);
    out.push('\n');

    if let (Some(id), Some(body)) = (active_id, profile_content) {
        let body = normalize_section(body);
        out.push('\n');
        out.push_str(&format!("{MARK_PROFILE_BEGIN_PREFIX} id={id}\n"));
        if !body.is_empty() {
            out.push_str(&body);
            out.push('\n');
        }
        out.push_str(MARK_PROFILE_END);
        out.push('\n');
    }
    out
}

fn load_state(dir: &Path) -> HostsState {
    let path = dir.join(STATE_FILE);
    if !path.exists() {
        return HostsState::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_state(dir: &Path, state: &HostsState) -> Result<(), String> {
    let path = dir.join(STATE_FILE);
    let raw = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| format!("保存 hosts 状态失败: {e}"))
}

fn load_profiles_meta(dir: &Path) -> Result<ProfilesFile, String> {
    let path = dir.join(PROFILES_META);
    if !path.exists() {
        return Ok(ProfilesFile {
            profiles: Vec::new(),
        });
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("解析方案列表失败: {e}"))
}

fn save_profiles_meta(dir: &Path, meta: &ProfilesFile) -> Result<(), String> {
    let path = dir.join(PROFILES_META);
    let raw = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| format!("保存方案列表失败: {e}"))
}

fn read_public_file(dir: &Path) -> Result<String, String> {
    let path = dir.join(PUBLIC_FILE);
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).map_err(|e| format!("读取公共配置失败: {e}"))
}

fn write_public_file(dir: &Path, content: &str) -> Result<(), String> {
    let path = dir.join(PUBLIC_FILE);
    fs::write(path, normalize_section(content)).map_err(|e| format!("保存公共配置失败: {e}"))
}

fn read_profile_file(dir: &Path, id: &str) -> Result<String, String> {
    let path = dir.join("profiles").join(format!("{id}.hosts"));
    fs::read_to_string(path).map_err(|e| format!("读取自定义配置失败: {e}"))
}

fn write_profile_file(dir: &Path, id: &str, content: &str) -> Result<(), String> {
    let path = dir.join("profiles").join(format!("{id}.hosts"));
    fs::write(path, normalize_section(content)).map_err(|e| format!("保存自定义配置失败: {e}"))
}

/// Seed the three default environments once. Deleted profiles are never recreated.
fn ensure_default_profiles(dir: &Path, state: &mut HostsState) -> Result<(), String> {
    if state.defaults_seeded {
        return Ok(());
    }

    let mut meta = load_profiles_meta(dir)?;
    if meta.profiles.is_empty() {
        let now = Local::now().to_rfc3339();
        for &(id, name) in DEFAULT_PROFILES {
            write_profile_file(dir, id, &format!("# {name}\n"))?;
            meta.profiles.push(ProfileMeta {
                id: id.to_string(),
                name: name.to_string(),
                updated_at: now.clone(),
            });
        }
        save_profiles_meta(dir, &meta)?;
    }

    state.defaults_seeded = true;
    save_state(dir, state)?;
    Ok(())
}

fn create_backup(app: &AppHandle, source: &str, content: &str) -> Result<String, String> {
    let dir = tools_hosts_dir(app)?;
    let id = Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let path = dir.join("backups").join(format!("{id}.hosts"));
    let header = format!("# tempo-backup source={source} at={}\n", Local::now().to_rfc3339());
    fs::write(&path, format!("{header}{content}")).map_err(|e| format!("写入备份失败: {e}"))?;
    prune_backups(&dir.join("backups"))?;
    Ok(id)
}

fn prune_backups(dir: &Path) -> Result<(), String> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "hosts")
        })
        .collect();
    files.sort_by_key(|entry| std::cmp::Reverse(entry.file_name()));
    for entry in files.into_iter().skip(MAX_BACKUPS) {
        let _ = fs::remove_file(entry.path());
    }
    Ok(())
}

fn write_system_hosts_raw(path: &Path, content: &str) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "hosts 路径无效".to_string())?;
    let tmp = parent.join(format!("hosts.tempo.tmp.{}", std::process::id()));
    fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {e}"))?;
    fs::rename(&tmp, path)
        .or_else(|_| {
            fs::copy(&tmp, path)
                .map(|_| ())
                .and_then(|_| fs::remove_file(&tmp))
                .map_err(|e| e)
        })
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("写入 hosts 失败: {e}。若尚未授权，请先点击「一键授权」。")
        })?;
    Ok(())
}

#[cfg(windows)]
fn grant_write_permission(path: &Path) -> Result<(), String> {
    let path_str = path.to_string_lossy().replace('\'', "''");
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "%USERNAME%".into());
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let account = if domain.is_empty() {
        user
    } else {
        format!("{domain}\\{user}")
    }
    .replace('\'', "''");

    let inner = format!(
        "icacls '{path_str}' /grant '{account}:(M)'; if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}; attrib -R '{path_str}'"
    );
    let outer = format!(
        "Start-Process -FilePath powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-Command','{}'",
        inner.replace('\'', "''")
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &outer])
        .output()
        .map_err(|e| format!("启动提权失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("授权失败（可能取消了 UAC）。{}", stderr.trim()));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn grant_write_permission(path: &Path) -> Result<(), String> {
    let path_str = path.to_string_lossy();
    let user = std::env::var("USER").unwrap_or_else(|_| "whoami".into());
    let script = format!(
        "do shell script \"chmod 644 '{path_str}' && chown {user} '{path_str}'\" with administrator privileges"
    );
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("启动提权失败: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("授权失败（可能取消了密码提示）。{}", stderr.trim()));
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn grant_write_permission(path: &Path) -> Result<(), String> {
    let path_str = path.to_string_lossy();
    let user = std::env::var("USER").unwrap_or_else(|_| "root".into());
    let status = Command::new("pkexec")
        .args(["chown", &user, path_str.as_ref()])
        .status()
        .or_else(|_| {
            Command::new("sudo")
                .args(["chown", &user, path_str.as_ref()])
                .status()
        })
        .map_err(|e| format!("启动提权失败: {e}"))?;
    if !status.success() {
        return Err("授权失败，请确认已安装 pkexec/sudo 并完成授权。".into());
    }
    let _ = Command::new("pkexec")
        .args(["chmod", "644", path_str.as_ref()])
        .status();
    Ok(())
}

fn flush_dns_cache() -> Result<(), String> {
    #[cfg(windows)]
    {
        let output = Command::new("ipconfig")
            .arg("/flushdns")
            .output()
            .map_err(|e| format!("刷新 DNS 失败: {e}"))?;
        if !output.status.success() {
            return Err("刷新 DNS 失败".into());
        }
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("dscacheutil").args(["-flushcache"]).status();
        let _ = Command::new("killall").args(["-HUP", "mDNSResponder"]).status();
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("systemd-resolve")
            .args(["--flush-caches"])
            .status();
        Ok(())
    }
}

/// First launch: treat current system hosts as public config (or parse markers).
fn ensure_initialized(app: &AppHandle) -> Result<(), String> {
    let dir = tools_hosts_dir(app)?;
    let mut state = load_state(&dir);
    let public_path = dir.join(PUBLIC_FILE);
    if state.initialized && public_path.exists() {
        return Ok(());
    }

    let path = hosts_path();
    let system = if path.exists() {
        read_hosts_content(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let parsed = parse_system_hosts(&system);

    if !public_path.exists() {
        write_public_file(&dir, &parsed.public)?;
    }

    // If system already has a Tempo profile section, sync it into storage.
    if let Some(id) = parsed.profile_id.clone() {
        let profile_path = dir.join("profiles").join(format!("{id}.hosts"));
        if !profile_path.exists() {
            write_profile_file(&dir, &id, &parsed.profile_content)?;
            let mut meta = load_profiles_meta(&dir)?;
            if !meta.profiles.iter().any(|p| p.id == id) {
                meta.profiles.push(ProfileMeta {
                    id: id.clone(),
                    name: format!("配置 {id}"),
                    updated_at: Local::now().to_rfc3339(),
                });
                save_profiles_meta(&dir, &meta)?;
            }
        }
        state.active_profile_id = Some(id);
    }

    state.initialized = true;
    save_state(&dir, &state)?;
    Ok(())
}

fn build_workspace(app: &AppHandle) -> Result<HostsWorkspace, String> {
    ensure_initialized(app)?;
    let dir = tools_hosts_dir(app)?;
    let mut state = load_state(&dir);
    ensure_default_profiles(&dir, &mut state)?;
    let mut meta = load_profiles_meta(&dir)?;
    let path = hosts_path();
    let system_content = if path.exists() {
        read_hosts_content(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let parsed = parse_system_hosts(&system_content);
    let writable = is_writable(&path);
    let public_content = read_public_file(&dir)?;

    // System hosts markers are the source of truth for what is currently applied.
    // Reconcile state.json so the active indicator survives restarts / refresh.
    let active_profile_id = reconcile_active_profile(&dir, &mut state, &mut meta, &parsed)?;

    let profiles = meta
        .profiles
        .into_iter()
        .map(|p| HostsProfile {
            active: active_profile_id.as_ref() == Some(&p.id),
            id: p.id,
            name: p.name,
            updated_at: p.updated_at,
        })
        .collect();

    Ok(HostsWorkspace {
        path: path.to_string_lossy().into_owned(),
        writable,
        authorized: writable,
        managed: parsed.managed,
        public_content,
        active_profile_id,
        profiles,
        system_content,
    })
}

/// Prefer the profile id embedded in the system hosts file; fall back to state.json.
/// Always write the resolved id back to state so UI activation persists.
fn reconcile_active_profile(
    dir: &Path,
    state: &mut HostsState,
    meta: &mut ProfilesFile,
    parsed: &ParsedSystemHosts,
) -> Result<Option<String>, String> {
    let from_system = parsed.profile_id.as_ref().and_then(|id| {
        let file_exists = dir.join("profiles").join(format!("{id}.hosts")).exists();
        let in_meta = meta.profiles.iter().any(|p| &p.id == id);
        if file_exists || in_meta || !parsed.profile_content.is_empty() {
            Some(id.clone())
        } else {
            None
        }
    });

    let from_state = state
        .active_profile_id
        .clone()
        .filter(|id| meta.profiles.iter().any(|p| &p.id == id));

    // Applied system content wins; otherwise keep last saved activation.
    let resolved = from_system.or(from_state);

    if let Some(ref id) = resolved {
        // Ensure profile content + meta exist when recovered from system markers.
        let profile_path = dir.join("profiles").join(format!("{id}.hosts"));
        if !profile_path.exists() {
            write_profile_file(dir, id, &parsed.profile_content)?;
        }
        if !meta.profiles.iter().any(|p| &p.id == id) {
            meta.profiles.push(ProfileMeta {
                id: id.clone(),
                name: format!("配置 {id}"),
                updated_at: Local::now().to_rfc3339(),
            });
            save_profiles_meta(dir, meta)?;
        }
    }

    if state.active_profile_id != resolved {
        state.active_profile_id = resolved.clone();
        state.initialized = true;
        save_state(dir, state)?;
    }

    Ok(resolved)
}

fn apply_composed(app: &AppHandle, source: &str) -> Result<HostsWorkspace, String> {
    let dir = tools_hosts_dir(app)?;
    let state = load_state(&dir);
    let public = read_public_file(&dir)?;
    validate_hosts_content(&public)?;

    let active_id = state.active_profile_id.clone();
    let profile_body = if let Some(ref id) = active_id {
        let body = read_profile_file(&dir, id)?;
        validate_hosts_content(&body)?;
        Some(body)
    } else {
        None
    };

    let composed = compose_system_hosts(
        &public,
        active_id.as_deref(),
        profile_body.as_deref(),
    );

    let path = hosts_path();
    let previous = if path.exists() {
        read_hosts_content(&path).unwrap_or_default()
    } else {
        String::new()
    };
    create_backup(app, source, &previous)?;
    write_system_hosts_raw(&path, &composed)?;
    let _ = flush_dns_cache();
    build_workspace(app)
}

#[tauri::command]
pub fn get_hosts_workspace(app: AppHandle, _state: tauri::State<AppState>) -> Result<HostsWorkspace, String> {
    build_workspace(&app)
}

#[tauri::command]
pub fn authorize_hosts_write(app: AppHandle) -> Result<HostsWorkspace, String> {
    let path = hosts_path();
    if !is_writable(&path) {
        grant_write_permission(&path)?;
        if !is_writable(&path) {
            return Err("授权已完成，但仍无法写入。请检查杀毒软件或系统保护是否拦截。".into());
        }
    }
    build_workspace(&app)
}

#[tauri::command]
pub fn save_hosts_public(app: AppHandle, content: String) -> Result<HostsWorkspace, String> {
    validate_hosts_content(&content)?;
    let dir = tools_hosts_dir(&app)?;
    write_public_file(&dir, &content)?;
    apply_composed(&app, "save_public")
}

#[tauri::command]
pub fn save_hosts_profile(
    app: AppHandle,
    id: Option<String>,
    name: String,
    content: String,
) -> Result<HostsProfile, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("配置名称不能为空".into());
    }
    validate_hosts_content(&content)?;
    let dir = tools_hosts_dir(&app)?;
    let mut meta = load_profiles_meta(&dir)?;
    let now = Local::now().to_rfc3339();
    let profile_id = id.unwrap_or_else(|| format!("p-{}", Local::now().format("%Y%m%d%H%M%S%3f")));
    write_profile_file(&dir, &profile_id, &content)?;

    if let Some(existing) = meta.profiles.iter_mut().find(|p| p.id == profile_id) {
        existing.name = name.clone();
        existing.updated_at = now.clone();
    } else {
        meta.profiles.push(ProfileMeta {
            id: profile_id.clone(),
            name: name.clone(),
            updated_at: now.clone(),
        });
    }
    save_profiles_meta(&dir, &meta)?;

    let state = load_state(&dir);
    let active = state.active_profile_id.as_ref() == Some(&profile_id);
    // If this profile is currently active, re-apply so system stays in sync.
    if active {
        let _ = apply_composed(&app, "save_active_profile");
    }

    Ok(HostsProfile {
        id: profile_id,
        name,
        updated_at: now,
        active,
    })
}

#[tauri::command]
pub fn delete_hosts_profile(app: AppHandle, id: String) -> Result<HostsWorkspace, String> {
    let dir = tools_hosts_dir(&app)?;
    let mut meta = load_profiles_meta(&dir)?;
    meta.profiles.retain(|p| p.id != id);
    save_profiles_meta(&dir, &meta)?;
    let _ = fs::remove_file(dir.join("profiles").join(format!("{id}.hosts")));

    let mut state = load_state(&dir);
    let was_active = state.active_profile_id.as_ref() == Some(&id);
    if was_active {
        state.active_profile_id = None;
        save_state(&dir, &state)?;
        return apply_composed(&app, "delete_active_profile");
    }
    build_workspace(&app)
}

#[tauri::command]
pub fn activate_hosts_profile(
    app: AppHandle,
    id: Option<String>,
) -> Result<HostsWorkspace, String> {
    let dir = tools_hosts_dir(&app)?;
    let meta = load_profiles_meta(&dir)?;
    let mut state = load_state(&dir);

    if let Some(ref profile_id) = id {
        if !meta.profiles.iter().any(|p| &p.id == profile_id) {
            return Err("自定义配置不存在".into());
        }
        // Ensure file exists.
        let _ = read_profile_file(&dir, profile_id)?;
        state.active_profile_id = Some(profile_id.clone());
    } else {
        state.active_profile_id = None;
    }
    save_state(&dir, &state)?;
    apply_composed(&app, "activate_profile")
}

#[tauri::command]
pub fn get_hosts_profile_content(app: AppHandle, id: String) -> Result<String, String> {
    let dir = tools_hosts_dir(&app)?;
    read_profile_file(&dir, &id)
}

#[tauri::command]
pub fn apply_hosts(app: AppHandle) -> Result<HostsWorkspace, String> {
    apply_composed(&app, "apply")
}

#[tauri::command]
pub fn flush_dns() -> Result<(), String> {
    flush_dns_cache()
}

#[tauri::command]
pub fn list_hosts_backups(app: AppHandle) -> Result<Vec<HostsBackup>, String> {
    let dir = tools_hosts_dir(&app)?;
    let backup_dir = dir.join("backups");
    let mut items = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&backup_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "hosts")
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

    for entry in entries.into_iter().take(MAX_BACKUPS) {
        let path = entry.path();
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let raw = fs::read_to_string(&path).unwrap_or_default();
        let mut source = "backup".to_string();
        let mut body = raw.as_str();
        if let Some(first) = raw.lines().next() {
            if first.starts_with("# tempo-backup") {
                if let Some(s) = first.split("source=").nth(1) {
                    source = s.split_whitespace().next().unwrap_or("backup").to_string();
                }
                body = raw.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
            }
        }
        items.push(HostsBackup {
            id,
            source,
            created_at: entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    chrono::DateTime::<chrono::Local>::from(t)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default(),
            preview: preview_text(body),
        });
    }
    Ok(items)
}

#[tauri::command]
pub fn restore_hosts_backup(app: AppHandle, id: String) -> Result<HostsWorkspace, String> {
    let dir = tools_hosts_dir(&app)?;
    let path = dir.join("backups").join(format!("{id}.hosts"));
    let raw = fs::read_to_string(path).map_err(|e| format!("读取备份失败: {e}"))?;
    let content = if raw.starts_with("# tempo-backup") {
        raw.split_once('\n')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or(raw)
    } else {
        raw
    };

    let parsed = parse_system_hosts(&content);
    write_public_file(&dir, &parsed.public)?;

    let mut state = load_state(&dir);
    if let Some(profile_id) = parsed.profile_id.clone() {
        write_profile_file(&dir, &profile_id, &parsed.profile_content)?;
        let mut meta = load_profiles_meta(&dir)?;
        if !meta.profiles.iter().any(|p| p.id == profile_id) {
            meta.profiles.push(ProfileMeta {
                id: profile_id.clone(),
                name: format!("恢复 {profile_id}"),
                updated_at: Local::now().to_rfc3339(),
            });
            save_profiles_meta(&dir, &meta)?;
        }
        state.active_profile_id = Some(profile_id);
    } else {
        state.active_profile_id = None;
    }
    state.initialized = true;
    save_state(&dir, &state)?;
    apply_composed(&app, "restore_backup")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unmanaged_whole_file_as_public() {
        let raw = "127.0.0.1 localhost\n# comment\n";
        let parsed = parse_system_hosts(raw);
        assert!(!parsed.managed);
        assert!(parsed.public.contains("127.0.0.1 localhost"));
        assert!(parsed.profile_id.is_none());
    }

    #[test]
    fn parse_and_compose_roundtrip() {
        let composed = compose_system_hosts(
            "127.0.0.1 localhost",
            Some("p-1"),
            Some("192.168.1.1 api.dev"),
        );
        let parsed = parse_system_hosts(&composed);
        assert!(parsed.managed);
        assert_eq!(parsed.public, "127.0.0.1 localhost");
        assert_eq!(parsed.profile_id.as_deref(), Some("p-1"));
        assert_eq!(parsed.profile_content, "192.168.1.1 api.dev");
    }

    #[test]
    fn outside_marker_content_merged_into_public() {
        let raw = "10.0.0.1 orphan\n# >>> TEMPO:PUBLIC:BEGIN\n127.0.0.1 localhost\n# <<< TEMPO:PUBLIC:END\n";
        let parsed = parse_system_hosts(raw);
        assert!(parsed.managed);
        assert!(parsed.public.contains("10.0.0.1 orphan"));
        assert!(parsed.public.contains("127.0.0.1 localhost"));
    }
}
