use chrono::{DateTime, Local, Utc};
use std::time::Duration;
use tauri::AppHandle;

use super::support::*;
use super::types::*;

pub(super) async fn fetch_remote_hosts(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("远程 URL 不能为空".into());
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("远程 URL 必须以 http:// 或 https:// 开头".into());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(format!("Tempo/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("拉取远程 hosts 失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("拉取远程 hosts 失败: HTTP {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("读取远程 hosts 内容失败: {e}"))?;
    validate_hosts_content(&text)?;
    Ok(normalize_section(&text))
}

/// Fetch remote content, update cache, and re-apply when the profile is active.
pub async fn refresh_hosts_remote_profile_inner(
    app: &AppHandle,
    id: &str,
) -> Result<HostsWorkspace, String> {
    let dir = tools_hosts_dir(app)?;
    let mut meta = load_profiles_meta(&dir)?;
    let Some(profile) = meta.profiles.iter_mut().find(|p| p.id == id) else {
        return Err("配置不存在".into());
    };
    if profile.kind != HostsProfileKind::Remote {
        return Err("仅远程配置支持同步".into());
    }
    let url = profile
        .remote_url
        .clone()
        .ok_or_else(|| "远程 URL 未设置".to_string())?;

    match fetch_remote_hosts(&url).await {
        Ok(content) => {
            let previous = read_profile_file(&dir, id).unwrap_or_default();
            write_profile_file(&dir, id, &content)?;
            profile.last_fetched_at = Some(Local::now().to_rfc3339());
            profile.last_fetch_error = None;
            profile.updated_at = Local::now().to_rfc3339();
            save_profiles_meta(&dir, &meta)?;

            let state = load_state(&dir);
            let active = state.active_profile_ids.iter().any(|x| x == id);
            if active && previous != content {
                return apply_composed(app, "refresh_remote");
            }
            if active && previous == content {
                // Still touch workspace so UI sees lastFetchedAt.
                return build_workspace(app);
            }
            build_workspace(app)
        }
        Err(error) => {
            if let Some(profile) = meta.profiles.iter_mut().find(|p| p.id == id) {
                profile.last_fetch_error = Some(error.clone());
                profile.updated_at = Local::now().to_rfc3339();
                let _ = save_profiles_meta(&dir, &meta);
            }
            Err(error)
        }
    }
}

fn due_for_refresh(profile: &ProfileMeta, now: DateTime<Utc>) -> bool {
    if profile.kind != HostsProfileKind::Remote {
        return false;
    }
    if profile.refresh_interval_secs == 0 {
        return false;
    }
    if profile.remote_url.as_deref().unwrap_or("").trim().is_empty() {
        return false;
    }
    let Some(last) = profile.last_fetched_at.as_deref() else {
        return true;
    };
    let Ok(parsed) = DateTime::parse_from_rfc3339(last) else {
        return true;
    };
    let elapsed = now.signed_duration_since(parsed.with_timezone(&Utc));
    elapsed.num_seconds() >= profile.refresh_interval_secs as i64
}

/// Background ticker: every minute, refresh remote profiles whose interval elapsed.
pub fn start_remote_refresh_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(error) = tick_remote_refreshes(&app).await {
                tracing::debug!(error = %error, "hosts remote refresh tick skipped");
            }
        }
    });
}

async fn tick_remote_refreshes(app: &AppHandle) -> Result<(), String> {
    let dir = tools_hosts_dir(app)?;
    let meta = load_profiles_meta(&dir)?;
    let now = Utc::now();
    let due: Vec<String> = meta
        .profiles
        .iter()
        .filter(|p| due_for_refresh(p, now))
        .map(|p| p.id.clone())
        .collect();

    for id in due {
        match refresh_hosts_remote_profile_inner(app, &id).await {
            Ok(_) => {
                tracing::info!(profile_id = %id, "hosts remote profile refreshed");
            }
            Err(error) => {
                tracing::warn!(profile_id = %id, error = %error, "hosts remote refresh failed");
            }
        }
    }
    Ok(())
}
