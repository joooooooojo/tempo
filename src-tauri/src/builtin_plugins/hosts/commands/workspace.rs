use crate::db::AppState;
use chrono::Local;
use std::fs;
use tauri::AppHandle;

use super::remote::refresh_hosts_remote_profile_inner;
use super::support::*;
use super::types::*;

#[tauri::command]
pub fn get_hosts_workspace(
    app: AppHandle,
    _state: tauri::State<AppState>,
) -> Result<HostsWorkspace, String> {
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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveHostsProfileArgs {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub remote_url: Option<String>,
    #[serde(default)]
    pub refresh_interval_secs: Option<u64>,
    /// One-time import path for local profiles.
    #[serde(default)]
    pub import_path: Option<String>,
}

fn parse_kind(raw: Option<&str>) -> Result<HostsProfileKind, String> {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("local") => Ok(HostsProfileKind::Local),
        Some("remote") => Ok(HostsProfileKind::Remote),
        Some(other) => Err(format!("未知配置类型: {other}")),
    }
}

#[tauri::command]
pub fn save_hosts_profile(app: AppHandle, args: SaveHostsProfileArgs) -> Result<HostsProfile, String> {
    let name = args.name.trim().to_string();
    if name.is_empty() {
        return Err("配置名称不能为空".into());
    }

    let dir = tools_hosts_dir(&app)?;
    ensure_initialized(&app)?;
    let mut meta = load_profiles_meta(&dir)?;
    let now = Local::now().to_rfc3339();
    let profile_id = args
        .id
        .clone()
        .unwrap_or_else(new_profile_id);

    let existing = meta.profiles.iter().find(|p| p.id == profile_id).cloned();
    let kind = if let Some(ref k) = args.kind {
        parse_kind(Some(k))?
    } else if let Some(ref existing) = existing {
        existing.kind
    } else {
        HostsProfileKind::Local
    };

    let mut content = if let Some(path) = args.import_path.as_deref().map(str::trim).filter(|s| !s.is_empty())
    {
        fs::read_to_string(path).map_err(|e| format!("读取本地文件失败: {e}"))?
    } else if let Some(body) = args.content.clone() {
        body
    } else if existing.is_some() {
        read_profile_file(&dir, &profile_id).unwrap_or_default()
    } else if kind == HostsProfileKind::Remote {
        String::new()
    } else {
        "# 自定义 hosts\n".to_string()
    };

    let remote_url = if kind == HostsProfileKind::Remote {
        let url = args
            .remote_url
            .clone()
            .or_else(|| existing.as_ref().and_then(|p| p.remote_url.clone()))
            .unwrap_or_default()
            .trim()
            .to_string();
        if url.is_empty() {
            return Err("远程配置需要填写 URL".into());
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("远程 URL 必须以 http:// 或 https:// 开头".into());
        }
        Some(url)
    } else {
        None
    };

    let refresh_interval_secs = if kind == HostsProfileKind::Remote {
        args.refresh_interval_secs
            .or_else(|| existing.as_ref().map(|p| p.refresh_interval_secs))
            .unwrap_or(0)
    } else {
        0
    };

    if kind == HostsProfileKind::Local {
        validate_hosts_content(&content)?;
    } else if !content.trim().is_empty() {
        validate_hosts_content(&content)?;
    }

    // Creating a brand-new remote with empty cache: leave empty until refresh.
    if kind == HostsProfileKind::Remote && content.trim().is_empty() && existing.is_none() {
        content = String::new();
    }

    write_profile_file(&dir, &profile_id, &content)?;

    let last_fetched_at = existing.as_ref().and_then(|p| p.last_fetched_at.clone());
    let last_fetch_error = existing.as_ref().and_then(|p| p.last_fetch_error.clone());

    if let Some(slot) = meta.profiles.iter_mut().find(|p| p.id == profile_id) {
        slot.name = name.clone();
        slot.updated_at = now.clone();
        slot.kind = kind;
        slot.remote_url = remote_url.clone();
        slot.refresh_interval_secs = refresh_interval_secs;
        if kind == HostsProfileKind::Local {
            slot.last_fetched_at = None;
            slot.last_fetch_error = None;
        }
    } else {
        meta.profiles.push(ProfileMeta {
            id: profile_id.clone(),
            name: name.clone(),
            updated_at: now.clone(),
            kind,
            remote_url: remote_url.clone(),
            refresh_interval_secs,
            last_fetched_at: last_fetched_at.clone(),
            last_fetch_error: last_fetch_error.clone(),
        });
    }
    save_profiles_meta(&dir, &meta)?;

    let state = load_state(&dir);
    let active = state.active_profile_ids.iter().any(|id| id == &profile_id);
    if active && kind == HostsProfileKind::Local {
        let _ = apply_composed(&app, "save_active_profile");
    }

    Ok(HostsProfile {
        id: profile_id,
        name,
        updated_at: now,
        active,
        kind,
        remote_url,
        refresh_interval_secs,
        last_fetched_at,
        last_fetch_error,
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
    let was_active = state.active_profile_ids.iter().any(|x| x == &id);
    if was_active {
        state.active_profile_ids.retain(|x| x != &id);
        save_state(&dir, &state)?;
        return apply_composed(&app, "delete_active_profile");
    }
    build_workspace(&app)
}

#[tauri::command]
pub fn set_hosts_profile_active(
    app: AppHandle,
    id: String,
    active: bool,
) -> Result<HostsWorkspace, String> {
    let dir = tools_hosts_dir(&app)?;
    let meta = load_profiles_meta(&dir)?;
    if !meta.profiles.iter().any(|p| p.id == id) {
        return Err("配置不存在".into());
    }
    let _ = read_profile_file(&dir, &id)?;

    let mut state = load_state(&dir);
    if active {
        if !state.active_profile_ids.iter().any(|x| x == &id) {
            state.active_profile_ids.push(id);
        }
    } else {
        state.active_profile_ids.retain(|x| x != &id);
    }
    save_state(&dir, &state)?;
    apply_composed(&app, "set_profile_active")
}

#[tauri::command]
pub fn get_hosts_profile_content(app: AppHandle, id: String) -> Result<String, String> {
    let dir = tools_hosts_dir(&app)?;
    read_profile_file(&dir, &id)
}

#[tauri::command]
pub fn open_hosts_file_location(app: AppHandle) -> Result<(), String> {
    let path = hosts_path();
    let dir = path
        .parent()
        .ok_or_else(|| "hosts 路径无效".to_string())?;
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(dir.display().to_string(), None::<String>)
        .map_err(|e| format!("打开文件位置失败: {e}"))
}

#[tauri::command]
pub async fn refresh_hosts_remote_profile(
    app: AppHandle,
    id: String,
) -> Result<HostsWorkspace, String> {
    refresh_hosts_remote_profile_inner(&app, &id).await
}
