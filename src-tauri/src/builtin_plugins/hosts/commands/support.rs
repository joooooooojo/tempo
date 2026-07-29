use crate::db::{current_storage_dir, default_storage_dir};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::AppHandle;

use super::types::*;

pub(super) fn hosts_path() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

pub(super) fn tools_hosts_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = current_storage_dir(app).or_else(|_| default_storage_dir(app))?;
    let dir = base.join("tools").join("hosts");
    fs::create_dir_all(dir.join("profiles")).map_err(|e| format!("创建 hosts 目录失败: {e}"))?;
    fs::create_dir_all(dir.join("backups")).map_err(|e| format!("创建备份目录失败: {e}"))?;
    Ok(dir)
}

pub(super) fn is_writable(path: &Path) -> bool {
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

pub(super) fn read_hosts_content(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("读取 hosts 失败: {e}"))
}

pub(super) fn normalize_section(content: &str) -> String {
    content.trim_matches(['\r', '\n']).to_string()
}

pub(super) fn validate_hosts_content(content: &str) -> Result<(), String> {
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

pub(super) fn looks_like_ip(value: &str) -> bool {
    if value.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    value.parse::<std::net::Ipv6Addr>().is_ok()
}

pub(super) fn looks_like_hostname(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 {
        return false;
    }
    value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_')
}

pub(super) fn preview_text(content: &str) -> String {
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
pub(super) fn parse_system_hosts(content: &str) -> ParsedSystemHosts {
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

pub(super) fn compose_system_hosts(
    public: &str,
    active_id: Option<&str>,
    profile_content: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(
        "# Managed by Tempo. Keep the marker lines so public/custom sections can be parsed.\n",
    );
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

pub(super) fn load_state(dir: &Path) -> HostsState {
    let path = dir.join(STATE_FILE);
    if !path.exists() {
        return HostsState::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub(super) fn save_state(dir: &Path, state: &HostsState) -> Result<(), String> {
    let path = dir.join(STATE_FILE);
    let raw = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| format!("保存 hosts 状态失败: {e}"))
}

pub(super) fn load_profiles_meta(dir: &Path) -> Result<ProfilesFile, String> {
    let path = dir.join(PROFILES_META);
    if !path.exists() {
        return Ok(ProfilesFile {
            profiles: Vec::new(),
        });
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("解析方案列表失败: {e}"))
}

pub(super) fn save_profiles_meta(dir: &Path, meta: &ProfilesFile) -> Result<(), String> {
    let path = dir.join(PROFILES_META);
    let raw = serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| format!("保存方案列表失败: {e}"))
}

pub(super) fn read_public_file(dir: &Path) -> Result<String, String> {
    let path = dir.join(PUBLIC_FILE);
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).map_err(|e| format!("读取公共配置失败: {e}"))
}

pub(super) fn write_public_file(dir: &Path, content: &str) -> Result<(), String> {
    let path = dir.join(PUBLIC_FILE);
    fs::write(path, normalize_section(content)).map_err(|e| format!("保存公共配置失败: {e}"))
}

pub(super) fn read_profile_file(dir: &Path, id: &str) -> Result<String, String> {
    let path = dir.join("profiles").join(format!("{id}.hosts"));
    fs::read_to_string(path).map_err(|e| format!("读取自定义配置失败: {e}"))
}

pub(super) fn write_profile_file(dir: &Path, id: &str, content: &str) -> Result<(), String> {
    let path = dir.join("profiles").join(format!("{id}.hosts"));
    fs::write(path, normalize_section(content)).map_err(|e| format!("保存自定义配置失败: {e}"))
}

/// Seed the three default environments once. Deleted profiles are never recreated.
pub(super) fn ensure_default_profiles(dir: &Path, state: &mut HostsState) -> Result<(), String> {
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

pub(super) fn create_backup(app: &AppHandle, source: &str, content: &str) -> Result<String, String> {
    let dir = tools_hosts_dir(app)?;
    let id = Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let path = dir.join("backups").join(format!("{id}.hosts"));
    let header = format!(
        "# tempo-backup source={source} at={}\n",
        Local::now().to_rfc3339()
    );
    fs::write(&path, format!("{header}{content}")).map_err(|e| format!("写入备份失败: {e}"))?;
    prune_backups(&dir.join("backups"))?;
    Ok(id)
}

pub(super) fn prune_backups(dir: &Path) -> Result<(), String> {
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

pub(super) fn write_system_hosts_raw(path: &Path, content: &str) -> Result<(), String> {
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
pub(super) fn grant_write_permission(path: &Path) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let user = std::env::var("USERNAME").unwrap_or_else(|_| "%USERNAME%".into());
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let account = if domain.is_empty() {
        user
    } else {
        format!("{domain}\\{user}")
    };

    let path_str = path.to_string_lossy();
    // cmd.exe + icacls/attrib are always present; elevate via ShellExecute "runas" (no PowerShell).
    let parameters =
        format!("/d /c icacls \"{path_str}\" /grant \"{account}:(M)\" && attrib -R \"{path_str}\"");

    let mut file = to_wide("cmd.exe");
    let mut verb = to_wide("runas");
    let mut params = to_wide(&parameters);

    let mut exec_info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_mut_ptr()),
        lpFile: PCWSTR(file.as_mut_ptr()),
        lpParameters: PCWSTR(params.as_mut_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    unsafe {
        ShellExecuteExW(&mut exec_info).map_err(|e| format!("启动提权失败: {e}"))?;
        if exec_info.hProcess.is_invalid() {
            return Err("启动提权失败：未获得进程句柄".into());
        }
        let wait = WaitForSingleObject(exec_info.hProcess, INFINITE);
        if wait != WAIT_OBJECT_0 {
            let _ = CloseHandle(exec_info.hProcess);
            return Err("等待提权进程结束失败".into());
        }
        let mut exit_code = 1u32;
        GetExitCodeProcess(exec_info.hProcess, &mut exit_code)
            .map_err(|e| format!("读取提权结果失败: {e}"))?;
        let _ = CloseHandle(exec_info.hProcess);
        if exit_code != 0 {
            return Err("授权失败（可能取消了 UAC）。".into());
        }
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn to_wide(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(target_os = "macos")]
pub(super) fn grant_write_permission(path: &Path) -> Result<(), String> {
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
pub(super) fn grant_write_permission(path: &Path) -> Result<(), String> {
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

pub(super) fn flush_dns_cache() -> Result<(), String> {
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
        let _ = Command::new("killall")
            .args(["-HUP", "mDNSResponder"])
            .status();
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
pub(super) fn ensure_initialized(app: &AppHandle) -> Result<(), String> {
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

pub(super) fn build_workspace(app: &AppHandle) -> Result<HostsWorkspace, String> {
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
pub(super) fn reconcile_active_profile(
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

pub(super) fn apply_composed(app: &AppHandle, source: &str) -> Result<HostsWorkspace, String> {
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

    let composed = compose_system_hosts(&public, active_id.as_deref(), profile_body.as_deref());

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

