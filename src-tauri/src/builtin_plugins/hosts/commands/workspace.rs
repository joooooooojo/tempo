use crate::db::AppState;
use chrono::Local;
use std::fs;
use tauri::AppHandle;

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

