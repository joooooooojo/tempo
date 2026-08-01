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
    let mut meta = load_profiles_meta(&dir)?;
    let mut state = load_state(&dir);
    let now = Local::now().to_rfc3339();
    let mut active_ids = Vec::new();

    // Legacy PUBLIC → local profile so restore does not drop it.
    if !parsed.legacy_public.is_empty() {
        let id = new_profile_id();
        write_profile_file(&dir, &id, &parsed.legacy_public)?;
        meta.profiles.push(ProfileMeta {
            id: id.clone(),
            name: "恢复-原公共".into(),
            updated_at: now.clone(),
            kind: HostsProfileKind::Local,
            remote_url: None,
            refresh_interval_secs: 0,
            last_fetched_at: None,
            last_fetch_error: None,
        });
        active_ids.push(id);
    }

    for (profile_id, body) in &parsed.profiles {
        write_profile_file(&dir, profile_id, body)?;
        if !meta.profiles.iter().any(|p| &p.id == profile_id) {
            meta.profiles.push(ProfileMeta {
                id: profile_id.clone(),
                name: format!("恢复 {profile_id}"),
                updated_at: now.clone(),
                kind: HostsProfileKind::Local,
                remote_url: None,
                refresh_interval_secs: 0,
                last_fetched_at: None,
                last_fetch_error: None,
            });
        }
        if !active_ids.iter().any(|x| x == profile_id) {
            active_ids.push(profile_id.clone());
        }
    }

    save_profiles_meta(&dir, &meta)?;
    state.active_profile_ids = active_ids;
    state.active_profile_id = None;
    state.initialized = true;
    state.migrated_v2 = true;
    save_state(&dir, &state)?;
    apply_composed(&app, "restore_backup")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unmanaged_whole_file_as_preamble() {
        let raw = "127.0.0.1 localhost\n# comment\n";
        let parsed = parse_system_hosts(raw);
        assert!(!parsed.managed);
        assert!(parsed.preamble.contains("127.0.0.1 localhost"));
        assert!(parsed.profiles.is_empty());
    }

    #[test]
    fn parse_and_compose_multi_profile_roundtrip() {
        let composed = compose_system_hosts(
            "127.0.0.1 localhost",
            &[
                ("p-1".into(), "192.168.1.1 api.dev".into()),
                ("p-2".into(), "10.0.0.2 staging".into()),
            ],
        );
        let parsed = parse_system_hosts(&composed);
        assert!(parsed.managed);
        assert_eq!(parsed.preamble, "127.0.0.1 localhost");
        assert_eq!(parsed.profiles.len(), 2);
        assert_eq!(parsed.profiles[0].0, "p-1");
        assert_eq!(parsed.profiles[0].1, "192.168.1.1 api.dev");
        assert_eq!(parsed.profiles[1].0, "p-2");
        assert_eq!(parsed.profiles[1].1, "10.0.0.2 staging");
    }

    #[test]
    fn legacy_public_folded_into_preamble() {
        let raw = "10.0.0.1 orphan\n# >>> TEMPO:PUBLIC:BEGIN\n127.0.0.1 localhost\n# <<< TEMPO:PUBLIC:END\n";
        let parsed = parse_system_hosts(raw);
        assert!(parsed.managed);
        assert!(parsed.preamble.contains("10.0.0.1 orphan"));
        assert!(parsed.preamble.contains("127.0.0.1 localhost"));
        assert!(!parsed.legacy_public.is_empty());
    }

    #[test]
    fn validate_allows_inline_comments_and_odd_hostnames() {
        let raw = "\
127.0.0.1 localhost # inline comment
0.0.0.0 broken_host!
140.82.112.4 github.com
";
        assert!(validate_hosts_content(raw).is_ok());
    }
}
