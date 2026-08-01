use crate::db::{current_storage_dir, default_storage_dir};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter};

use super::types::*;

pub(super) const HOSTS_CHANGED_EVENT: &str = "hosts-changed";

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
        // Inline comments are common in hosts files (and used to disable entries).
        let line = line.split('#').next().unwrap_or(line).trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(ip) = parts.next() else {
            continue;
        };
        if !looks_like_ip(ip) {
            return Err(format!("第 {} 行：无效 IP「{}」", index + 1, ip));
        }
        // Hostnames are intentionally not format-checked: users often leave broken /
        // commented-style names to keep a mapping from taking effect.
        if parts.next().is_none() {
            return Err(format!("第 {} 行：缺少主机名", index + 1));
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

pub(super) fn new_profile_id() -> String {
    format!("p-{}", Local::now().format("%Y%m%d%H%M%S%3f"))
}

/// Parse Tempo-managed sections from a system hosts file.
///
/// New format:
/// ```text
/// <preamble>
/// # >>> TEMPO:PROFILE:BEGIN id=<id>
/// ...profile...
/// # <<< TEMPO:PROFILE:END
/// ```
///
/// Legacy PUBLIC markers are still recognized for migration.
pub(super) fn parse_system_hosts(content: &str) -> ParsedSystemHosts {
    let lines: Vec<&str> = content.lines().collect();
    let mut public_start: Option<usize> = None;
    let mut public_end: Option<usize> = None;
    let mut profile_ranges: Vec<(usize, usize, String)> = Vec::new();
    let mut open_profile: Option<(usize, String)> = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == MARK_PUBLIC_BEGIN {
            public_start = Some(idx);
        } else if trimmed == MARK_PUBLIC_END {
            public_end = Some(idx);
        } else if let Some(rest) = trimmed.strip_prefix(MARK_PROFILE_BEGIN_PREFIX) {
            let id = rest
                .split_whitespace()
                .find_map(|part| part.strip_prefix("id="))
                .unwrap_or("")
                .to_string();
            open_profile = Some((idx, id));
        } else if trimmed == MARK_PROFILE_END {
            if let Some((start, id)) = open_profile.take() {
                if !id.is_empty() {
                    profile_ranges.push((start, idx, id));
                }
            }
        }
    }

    let has_legacy_public = public_start.is_some() && public_end.is_some();
    let has_profiles = !profile_ranges.is_empty();
    let managed = has_legacy_public || has_profiles;

    if !managed {
        return ParsedSystemHosts {
            managed: false,
            preamble: normalize_section(content),
            profiles: Vec::new(),
            legacy_public: String::new(),
        };
    }

    let legacy_public = if has_legacy_public {
        let ps = public_start.unwrap();
        let pe = public_end.unwrap();
        if pe > ps + 1 {
            normalize_section(&lines[ps + 1..pe].join("\n"))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let profiles: Vec<(String, String)> = profile_ranges
        .iter()
        .map(|(start, end, id)| {
            let body = if *end > *start + 1 {
                normalize_section(&lines[*start + 1..*end].join("\n"))
            } else {
                String::new()
            };
            (id.clone(), body)
        })
        .collect();

    let mut skipped = vec![false; lines.len()];
    if has_legacy_public {
        let ps = public_start.unwrap();
        let pe = public_end.unwrap();
        for i in ps..=pe {
            skipped[i] = true;
        }
    }
    for (start, end, _) in &profile_ranges {
        for i in *start..=*end {
            skipped[i] = true;
        }
    }

    let mut outside: Vec<&str> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if skipped[i] {
            continue;
        }
        let trimmed = line.trim();
        let is_tempo_banner = trimmed.starts_with("# Managed by Tempo")
            || trimmed.starts_with("# >>> TEMPO:")
            || trimmed.starts_with("# <<< TEMPO:");
        if !trimmed.is_empty() && !is_tempo_banner {
            outside.push(*line);
        }
    }

    let outside_text = normalize_section(&outside.join("\n"));
    // Legacy PUBLIC body is folded into preamble so it is not lost before migration apply.
    let preamble = if outside_text.is_empty() {
        legacy_public.clone()
    } else if legacy_public.is_empty() {
        outside_text
    } else {
        normalize_section(&format!("{outside_text}\n\n{legacy_public}"))
    };

    ParsedSystemHosts {
        managed: true,
        preamble,
        profiles,
        legacy_public,
    }
}

pub(super) fn compose_system_hosts(preamble: &str, profiles: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push_str(
        "# Managed by Tempo. Keep the marker lines so profile sections can be parsed.\n",
    );
    let preamble = normalize_section(preamble);
    if !preamble.is_empty() {
        out.push_str(&preamble);
        out.push('\n');
    }

    for (id, body) in profiles {
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

pub(super) fn read_profile_file(dir: &Path, id: &str) -> Result<String, String> {
    let path = dir.join("profiles").join(format!("{id}.hosts"));
    fs::read_to_string(path).map_err(|e| format!("读取配置失败: {e}"))
}

pub(super) fn write_profile_file(dir: &Path, id: &str, content: &str) -> Result<(), String> {
    let path = dir.join("profiles").join(format!("{id}.hosts"));
    fs::write(path, normalize_section(content)).map_err(|e| format!("保存配置失败: {e}"))
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

fn normalize_active_ids(meta: &ProfilesFile, ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        if meta.profiles.iter().any(|p| &p.id == id) && !out.iter().any(|x| x == id) {
            out.push(id.clone());
        }
    }
    out
}

/// Migrate v1 (public + single active) → v2 (multi-active, no public).
pub(super) fn ensure_migrated_v2(dir: &Path, state: &mut HostsState) -> Result<(), String> {
    if state.migrated_v2 {
        // Still fold legacy single id if somehow present.
        if let Some(id) = state.active_profile_id.take() {
            if !state.active_profile_ids.iter().any(|x| x == &id) {
                state.active_profile_ids.push(id);
                save_state(dir, state)?;
            }
        }
        return Ok(());
    }

    let mut meta = load_profiles_meta(dir)?;
    let now = Local::now().to_rfc3339();

    if let Some(id) = state.active_profile_id.take() {
        if !state.active_profile_ids.iter().any(|x| x == &id) {
            state.active_profile_ids.push(id);
        }
    }

    let public_path = dir.join(PUBLIC_FILE);
    if public_path.exists() {
        let public = fs::read_to_string(&public_path).unwrap_or_default();
        let public = normalize_section(&public);
        if !public.is_empty() {
            let already = meta.profiles.iter().any(|p| p.name == "原公共配置");
            if !already {
                let id = new_profile_id();
                write_profile_file(dir, &id, &public)?;
                meta.profiles.insert(
                    0,
                    ProfileMeta {
                        id: id.clone(),
                        name: "原公共配置".into(),
                        updated_at: now.clone(),
                        kind: HostsProfileKind::Local,
                        remote_url: None,
                        refresh_interval_secs: 0,
                        last_fetched_at: None,
                        last_fetch_error: None,
                    },
                );
                if !state.active_profile_ids.iter().any(|x| x == &id) {
                    state.active_profile_ids.insert(0, id);
                }
                save_profiles_meta(dir, &meta)?;
            }
        }
        let _ = fs::remove_file(&public_path);
    }

    // Ensure every profile has a kind (serde default covers this on load).
    for profile in &mut meta.profiles {
        if profile.kind == HostsProfileKind::Remote && profile.remote_url.is_none() {
            profile.kind = HostsProfileKind::Local;
        }
    }
    save_profiles_meta(dir, &meta)?;

    state.active_profile_ids = normalize_active_ids(&meta, &state.active_profile_ids);
    state.migrated_v2 = true;
    state.defaults_seeded = true; // never seed env-* going forward
    state.initialized = true;
    save_state(dir, state)?;
    Ok(())
}

pub(super) fn ensure_initialized(app: &AppHandle) -> Result<(), String> {
    let dir = tools_hosts_dir(app)?;
    let mut state = load_state(&dir);

    if !state.initialized {
        let path = hosts_path();
        let system = if path.exists() {
            read_hosts_content(&path).unwrap_or_default()
        } else {
            String::new()
        };
        let parsed = parse_system_hosts(&system);

        // Sync any profile sections already on disk into storage.
        if !parsed.profiles.is_empty() {
            let mut meta = load_profiles_meta(&dir)?;
            let now = Local::now().to_rfc3339();
            for (id, body) in &parsed.profiles {
                let profile_path = dir.join("profiles").join(format!("{id}.hosts"));
                if !profile_path.exists() {
                    write_profile_file(&dir, id, body)?;
                }
                if !meta.profiles.iter().any(|p| &p.id == id) {
                    meta.profiles.push(ProfileMeta {
                        id: id.clone(),
                        name: format!("配置 {id}"),
                        updated_at: now.clone(),
                        kind: HostsProfileKind::Local,
                        remote_url: None,
                        refresh_interval_secs: 0,
                        last_fetched_at: None,
                        last_fetch_error: None,
                    });
                }
                if !state.active_profile_ids.iter().any(|x| x == id) {
                    state.active_profile_ids.push(id.clone());
                }
            }
            save_profiles_meta(&dir, &meta)?;
        }

        // Bootstrap public.hosts only so v2 migration can pick it up for existing installs.
        let public_path = dir.join(PUBLIC_FILE);
        if !public_path.exists() && !parsed.legacy_public.is_empty() {
            fs::write(&public_path, &parsed.legacy_public)
                .map_err(|e| format!("写入临时公共配置失败: {e}"))?;
        } else if !public_path.exists()
            && !state.migrated_v2
            && !parsed.managed
            && !parsed.preamble.is_empty()
        {
            // Unmanaged system hosts: leave as preamble on first apply; no public file.
        }

        state.initialized = true;
        save_state(&dir, &state)?;
    }

    ensure_migrated_v2(&dir, &mut state)?;
    Ok(())
}

pub(super) fn reconcile_active_profiles(
    dir: &Path,
    state: &mut HostsState,
    meta: &mut ProfilesFile,
    parsed: &ParsedSystemHosts,
) -> Result<Vec<String>, String> {
    // Prefer ids embedded in the system file when present; otherwise keep state.
    let from_system: Vec<String> = parsed
        .profiles
        .iter()
        .map(|(id, _)| id.clone())
        .filter(|id| {
            dir.join("profiles").join(format!("{id}.hosts")).exists()
                || meta.profiles.iter().any(|p| &p.id == id)
                || parsed
                    .profiles
                    .iter()
                    .any(|(pid, body)| pid == id && !body.is_empty())
        })
        .collect();

    for (id, body) in &parsed.profiles {
        let profile_path = dir.join("profiles").join(format!("{id}.hosts"));
        if !profile_path.exists() {
            write_profile_file(dir, id, body)?;
        }
        if !meta.profiles.iter().any(|p| &p.id == id) {
            meta.profiles.push(ProfileMeta {
                id: id.clone(),
                name: format!("配置 {id}"),
                updated_at: Local::now().to_rfc3339(),
                kind: HostsProfileKind::Local,
                remote_url: None,
                refresh_interval_secs: 0,
                last_fetched_at: None,
                last_fetch_error: None,
            });
            save_profiles_meta(dir, meta)?;
        }
    }

    let resolved = if parsed.managed && !from_system.is_empty() {
        from_system
    } else {
        normalize_active_ids(meta, &state.active_profile_ids)
    };

    if state.active_profile_ids != resolved {
        state.active_profile_ids = resolved.clone();
        state.initialized = true;
        save_state(dir, state)?;
    }

    Ok(resolved)
}

pub(super) fn build_workspace(app: &AppHandle) -> Result<HostsWorkspace, String> {
    ensure_initialized(app)?;
    let dir = tools_hosts_dir(app)?;
    let mut state = load_state(&dir);
    let mut meta = load_profiles_meta(&dir)?;
    let path = hosts_path();
    let system_content = if path.exists() {
        read_hosts_content(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let parsed = parse_system_hosts(&system_content);
    let writable = is_writable(&path);
    let active_profile_ids = reconcile_active_profiles(&dir, &mut state, &mut meta, &parsed)?;

    let profiles = meta
        .profiles
        .iter()
        .map(|p| p.to_public(&active_profile_ids))
        .collect();

    Ok(HostsWorkspace {
        path: path.to_string_lossy().into_owned(),
        writable,
        authorized: writable,
        managed: parsed.managed,
        active_profile_ids,
        profiles,
        system_content,
    })
}

pub(super) fn apply_composed(app: &AppHandle, source: &str) -> Result<HostsWorkspace, String> {
    let dir = tools_hosts_dir(app)?;
    let state = load_state(&dir);
    let meta = load_profiles_meta(&dir)?;
    let path = hosts_path();
    let previous = if path.exists() {
        read_hosts_content(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let parsed = parse_system_hosts(&previous);
    let preamble = parsed.preamble;

    let active_ids = normalize_active_ids(&meta, &state.active_profile_ids);
    let mut sections: Vec<(String, String)> = Vec::new();
    for id in &active_ids {
        let body = read_profile_file(&dir, id)?;
        validate_hosts_content(&body)?;
        sections.push((id.clone(), body));
    }

    let composed = compose_system_hosts(&preamble, &sections);
    create_backup(app, source, &previous)?;
    write_system_hosts_raw(&path, &composed)?;
    let _ = flush_dns_cache();
    let workspace = build_workspace(app)?;
    let _ = app.emit(HOSTS_CHANGED_EVENT, ());
    Ok(workspace)
}
