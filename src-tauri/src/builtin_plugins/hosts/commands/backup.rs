use chrono::Local;
use std::fs;
use tauri::AppHandle;

use super::support::*;
use super::types::*;

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
