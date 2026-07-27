use crate::db::AppState;
use crate::launcher_search::SearchTextIndex;
use chrono::Local;
use parking_lot::RwLock;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

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
    search_index: SearchTextIndex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedLauncherRecord {
    id: String,
    name: String,
    subtitle: String,
    keywords: Vec<String>,
    target: String,
    icon_source: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MainPanelSearchContributionSource {
    Builtin,
    Plugin,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainPanelSearchContribution {
    id: String,
    name: String,
    #[serde(default)]
    keywords: Vec<String>,
    source: MainPanelSearchContributionSource,
}

#[derive(Debug, Clone)]
struct IndexedSearchContribution {
    id: String,
    name: String,
    usage_id: String,
    priority: u8,
    search_index: SearchTextIndex,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MainPanelSearchMatchSource {
    Launcher,
    Contribution,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainPanelSearchMatch {
    source: MainPanelSearchMatchSource,
    id: String,
    score: u32,
    app: Option<LauncherApp>,
}

#[derive(Clone, Default)]
struct LauncherUsage {
    pinned: bool,
    last_used_at: Option<String>,
    use_count: i64,
}

#[derive(Default)]
struct LauncherUsageCache {
    loaded_at: Option<Instant>,
    items: HashMap<String, LauncherUsage>,
}

static INDEXING: AtomicBool = AtomicBool::new(false);

fn launcher_cache() -> &'static RwLock<Vec<LauncherRecord>> {
    static CACHE: OnceLock<RwLock<Vec<LauncherRecord>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(Vec::new()))
}

fn search_contribution_cache() -> &'static RwLock<Vec<IndexedSearchContribution>> {
    static CACHE: OnceLock<RwLock<Vec<IndexedSearchContribution>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(Vec::new()))
}

fn launcher_usage_cache() -> &'static RwLock<LauncherUsageCache> {
    static CACHE: OnceLock<RwLock<LauncherUsageCache>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(LauncherUsageCache::default()))
}

const MAX_LAUNCHER_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_LAUNCHER_SNAPSHOT_RECORDS: usize = 20_000;

fn launcher_platform_key() -> &'static str {
    std::env::consts::OS
}

pub fn restore_launcher_index_snapshot(state: &AppState) {
    let restored = {
        let conn = state.db.lock();
        load_launcher_index_snapshot(&conn, launcher_platform_key())
    };
    match restored {
        Ok(records) if !records.is_empty() => {
            let count = records.len();
            *launcher_cache().write() = records.clone();
            tracing::info!(
                platform = launcher_platform_key(),
                count,
                "restored launcher index snapshot"
            );
            prewarm_launcher_icons(&records);
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(
                platform = launcher_platform_key(),
                error = %error,
                "failed to restore launcher index snapshot"
            );
        }
    }
}

fn load_launcher_index_snapshot(
    conn: &Connection,
    platform: &str,
) -> Result<Vec<LauncherRecord>, String> {
    let payload = conn
        .query_row(
            "SELECT payload FROM launcher_index_snapshots WHERE platform = ?1",
            [platform],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(payload) = payload else {
        return Ok(Vec::new());
    };
    if payload.len() > MAX_LAUNCHER_SNAPSHOT_BYTES {
        return Err("launcher index snapshot is too large".into());
    }

    let stored = serde_json::from_str::<Vec<PersistedLauncherRecord>>(&payload)
        .map_err(|error| format!("decode launcher index snapshot: {error}"))?;
    if stored.len() > MAX_LAUNCHER_SNAPSHOT_RECORDS {
        return Err("launcher index snapshot has too many records".into());
    }

    stored
        .into_iter()
        .map(PersistedLauncherRecord::into_launcher_record)
        .collect()
}

fn persist_launcher_index_snapshot(
    conn: &Connection,
    platform: &str,
    records: &[LauncherRecord],
) -> Result<(), String> {
    let stored = records
        .iter()
        .map(PersistedLauncherRecord::from)
        .collect::<Vec<_>>();
    let payload = serde_json::to_string(&stored)
        .map_err(|error| format!("encode launcher index snapshot: {error}"))?;
    if payload.len() > MAX_LAUNCHER_SNAPSHOT_BYTES {
        return Err("launcher index snapshot is too large".into());
    }
    conn.execute(
        "INSERT INTO launcher_index_snapshots (platform, payload, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(platform) DO UPDATE SET
           payload = excluded.payload,
           updated_at = excluded.updated_at",
        params![platform, payload, Local::now().to_rfc3339()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

impl From<&LauncherRecord> for PersistedLauncherRecord {
    fn from(record: &LauncherRecord) -> Self {
        Self {
            id: record.id.clone(),
            name: record.name.clone(),
            subtitle: record.subtitle.clone(),
            keywords: record.keywords.clone(),
            target: record.target.clone(),
            icon_source: record.icon_source.clone(),
        }
    }
}

impl PersistedLauncherRecord {
    fn into_launcher_record(self) -> Result<LauncherRecord, String> {
        if self.name.trim().is_empty() || self.target.trim().is_empty() {
            return Err("launcher index snapshot contains an invalid record".into());
        }
        if self.id != launcher_id(&self.target) {
            return Err("launcher index snapshot contains a mismatched id".into());
        }
        let search_index = SearchTextIndex::new(&self.name, &self.keywords);
        Ok(LauncherRecord {
            id: self.id,
            name: self.name,
            subtitle: self.subtitle,
            keywords: self.keywords,
            target: self.target,
            icon_source: self.icon_source,
            search_index,
        })
    }
}

fn launcher_index_ready_signal() -> &'static (StdMutex<()>, Condvar) {
    static SIGNAL: OnceLock<(StdMutex<()>, Condvar)> = OnceLock::new();
    SIGNAL.get_or_init(|| (StdMutex::new(()), Condvar::new()))
}

fn finish_launcher_indexing() {
    let (lock, ready) = launcher_index_ready_signal();
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    INDEXING.store(false, Ordering::Release);
    ready.notify_all();
}

fn wait_for_launcher_index() {
    if !launcher_cache().read().is_empty() || !INDEXING.load(Ordering::Acquire) {
        return;
    }

    let (lock, ready) = launcher_index_ready_signal();
    let guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    drop(
        ready.wait_timeout_while(guard, Duration::from_secs(2), |_| {
            launcher_cache().read().is_empty() && INDEXING.load(Ordering::Acquire)
        }),
    );
}

pub fn warm_launcher_index(app: AppHandle) {
    if INDEXING.swap(true, Ordering::AcqRel) {
        return;
    }

    crate::logging::spawn_named("tempo-launcher-index", move || {
        let records = enumerate_launcher_apps();
        publish_launcher_records(&app, &records);
        finish_launcher_indexing();
    });
}

/// Background re-index when the main panel opens so newly installed apps appear without restart.
pub fn request_launcher_index_refresh(app: &AppHandle) {
    warm_launcher_index(app.clone());
}

#[tauri::command]
pub fn get_launcher_apps(state: tauri::State<AppState>) -> Vec<LauncherApp> {
    hydrate_launcher_apps(&state, launcher_cache().read().clone())
}

#[tauri::command]
pub fn refresh_launcher_apps(app: AppHandle, state: tauri::State<AppState>) -> Vec<LauncherApp> {
    if INDEXING.swap(true, Ordering::AcqRel) {
        return hydrate_launcher_apps(&state, launcher_cache().read().clone());
    }

    let records = enumerate_launcher_apps();
    publish_launcher_records(&app, &records);
    finish_launcher_indexing();
    hydrate_launcher_apps(&state, records)
}

#[tauri::command]
pub fn sync_main_panel_search_contributions(
    contributions: Vec<MainPanelSearchContribution>,
) -> Result<(), String> {
    *search_contribution_cache().write() = index_search_contributions(contributions)?;
    Ok(())
}

#[tauri::command]
pub async fn search_main_panel_apps(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<MainPanelSearchMatch>, String> {
    if launcher_cache().read().is_empty() && !INDEXING.load(Ordering::Acquire) {
        warm_launcher_index(app);
    }
    let state = state.inner().clone();
    let query_chars = query.chars().count();
    let started_at = Instant::now();
    let results = tauri::async_runtime::spawn_blocking(move || {
        search_main_panel_apps_inner(&state, &query, limit)
    })
    .await
    .map_err(|error| format!("搜索任务失败：{error}"))?;
    tracing::debug!(
        query_chars,
        result_count = results.len(),
        elapsed_ms = started_at.elapsed().as_millis(),
        "Rust launcher search completed"
    );
    Ok(results)
}

fn search_main_panel_apps_inner(
    state: &AppState,
    query: &str,
    limit: Option<u32>,
) -> Vec<MainPanelSearchMatch> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    wait_for_launcher_index();
    let usage = launcher_usage_snapshot(state);
    let mut matches = Vec::new();

    for record in launcher_cache().read().iter() {
        let Some(semantic_score) = record.search_index.score(query) else {
            continue;
        };
        let usage = usage.get(&record.id);
        let pinned = usage.is_some_and(|item| item.pinned);
        let use_count = usage.map_or(0, |item| item.use_count);
        matches.push(RankedSearchMatch {
            result: MainPanelSearchMatch {
                source: MainPanelSearchMatchSource::Launcher,
                id: record.id.clone(),
                score: semantic_score + ranking_bonus(pinned, use_count),
                app: Some(launcher_app_from_record(record, usage)),
            },
            name: record.name.clone(),
            priority: 0,
            pinned,
            use_count,
        });
    }

    for contribution in search_contribution_cache().read().iter() {
        let Some(semantic_score) = contribution.search_index.score(query) else {
            continue;
        };
        let use_count = usage
            .get(&contribution.usage_id)
            .map_or(0, |item| item.use_count);
        matches.push(RankedSearchMatch {
            result: MainPanelSearchMatch {
                source: MainPanelSearchMatchSource::Contribution,
                id: contribution.id.clone(),
                score: semantic_score + ranking_bonus(false, use_count),
                app: None,
            },
            name: contribution.name.clone(),
            priority: contribution.priority,
            pinned: false,
            use_count,
        });
    }

    matches.sort_by(|left, right| {
        right
            .result
            .score
            .cmp(&left.result.score)
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| right.pinned.cmp(&left.pinned))
            .then_with(|| right.use_count.cmp(&left.use_count))
            .then_with(|| normalize_name(&left.name).cmp(&normalize_name(&right.name)))
            .then_with(|| left.result.id.cmp(&right.result.id))
    });

    matches
        .into_iter()
        .take(limit.unwrap_or(36).clamp(1, 100) as usize)
        .map(|item| item.result)
        .collect()
}

struct RankedSearchMatch {
    result: MainPanelSearchMatch,
    name: String,
    priority: u8,
    pinned: bool,
    use_count: i64,
}

fn index_search_contributions(
    contributions: Vec<MainPanelSearchContribution>,
) -> Result<Vec<IndexedSearchContribution>, String> {
    const MAX_CONTRIBUTIONS: usize = 4_096;
    const MAX_ID_CHARS: usize = 256;
    const MAX_NAME_CHARS: usize = 256;
    const MAX_KEYWORDS: usize = 64;
    const MAX_KEYWORD_CHARS: usize = 512;

    if contributions.len() > MAX_CONTRIBUTIONS {
        return Err("搜索索引条目过多".into());
    }

    let mut seen = HashSet::new();
    let mut indexed = Vec::with_capacity(contributions.len());
    for contribution in contributions {
        let id = contribution.id.trim();
        let name = contribution.name.trim();
        if id.is_empty()
            || id.chars().count() > MAX_ID_CHARS
            || name.is_empty()
            || name.chars().count() > MAX_NAME_CHARS
            || contribution.keywords.len() > MAX_KEYWORDS
            || contribution
                .keywords
                .iter()
                .any(|keyword| keyword.chars().count() > MAX_KEYWORD_CHARS)
        {
            return Err("搜索索引条目无效".into());
        }

        let source = match contribution.source {
            MainPanelSearchContributionSource::Builtin => "builtin",
            MainPanelSearchContributionSource::Plugin => "plugin",
        };
        if !seen.insert(format!("{source}:{id}")) {
            continue;
        }

        let keywords = contribution
            .keywords
            .into_iter()
            .map(|keyword| keyword.trim().to_string())
            .filter(|keyword| !keyword.is_empty())
            .collect::<Vec<_>>();
        indexed.push(IndexedSearchContribution {
            id: id.to_string(),
            name: name.to_string(),
            usage_id: format!("{source}:{id}"),
            priority: u8::from(matches!(
                contribution.source,
                MainPanelSearchContributionSource::Builtin
            )),
            search_index: SearchTextIndex::new(name, &keywords),
        });
    }

    Ok(indexed)
}

fn ranking_bonus(pinned: bool, use_count: i64) -> u32 {
    let use_count = use_count.max(0) as f64;
    let usage_bonus = ((use_count + 1.0).log2() * 18.0).min(120.0) as u32;
    usage_bonus + if pinned { 60 } else { 0 }
}

#[tauri::command]
pub fn launch_indexed_app(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    let record = launcher_cache()
        .read()
        .iter()
        .find(|record| record.id == id)
        .cloned()
        .ok_or_else(|| "应用索引已失效，请刷新后重试".to_string())?;

    // Fire-and-forget: do not inherit our console, keep a process handle, or wait on the app.
    launch_target_detached(&record.target)
        .map_err(|error| format!("无法启动 {}：{error}", record.name))?;

    touch_launcher_usage(&state, &id)?;
    Ok(())
}

/// Launch a filesystem target without attaching it to Tempo's process tree, console, or job.
fn launch_target_detached(target: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        launch_windows_detached(target)
    }
    #[cfg(target_os = "macos")]
    {
        // `open` returns immediately and the launched app is not our child.
        std::process::Command::new("open")
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

#[cfg(windows)]
fn launch_windows_detached(target: &str) -> Result<(), String> {
    launch_windows_from_explorer(target)
}

#[cfg(windows)]
fn launch_windows_from_explorer(target: &str) -> Result<(), String> {
    use windows::core::{Interface, BSTR, VARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, IDispatch, IServiceProvider, CLSCTX_LOCAL_SERVER,
    };
    use windows::Win32::UI::Shell::{
        IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellWindows, SID_STopLevelBrowser,
        ShellWindows, CSIDL_DESKTOP, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let _com = crate::platform::icon_extraction_thread_context();

    // Obtain Shell.Application from Explorer's desktop view. Calling ShellExecute on this
    // proxy makes Explorer own the launch, so the app does not inherit Tempo's job or I/O.
    let shell_windows: IShellWindows = explorer_stage("create-shell-windows", unsafe {
        CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)
    })?;
    let desktop = VARIANT::from(CSIDL_DESKTOP as i32);
    let empty = VARIANT::default();
    let mut desktop_hwnd = 0;
    let desktop_dispatch = explorer_stage("find-desktop-shell", unsafe {
        shell_windows.FindWindowSW(
            &desktop,
            &empty,
            SWC_DESKTOP,
            &mut desktop_hwnd,
            SWFO_NEEDDISPATCH,
        )
    })?;
    let services: IServiceProvider =
        explorer_stage("query-shell-services", desktop_dispatch.cast())?;
    let browser: IShellBrowser = explorer_stage("query-shell-browser", unsafe {
        services.QueryService(&SID_STopLevelBrowser)
    })?;
    let view = explorer_stage("query-shell-view", unsafe {
        browser.QueryActiveShellView()
    })?;
    let background: IDispatch = explorer_stage("get-shell-background", unsafe {
        view.GetItemObject(SVGIO_BACKGROUND)
    })?;
    let folder_view: IShellFolderViewDual =
        explorer_stage("get-shell-background", background.cast())?;
    let application = explorer_stage("get-shell-application", unsafe {
        folder_view.Application()
    })?;
    let shell: IShellDispatch2 = explorer_stage("query-shell-dispatch", application.cast())?;

    let file = BSTR::from(target);
    let empty_string = VARIANT::from("");
    let show = VARIANT::from(SW_SHOWNORMAL.0);
    explorer_stage("shell-execute", unsafe {
        shell.ShellExecute(&file, &empty_string, &empty_string, &empty_string, &show)
    })
}

#[cfg(windows)]
fn explorer_stage<T>(stage: &str, result: windows::core::Result<T>) -> Result<T, String> {
    result.map_err(|error| format!("{stage}: {error}"))
}

#[tauri::command]
pub fn record_launcher_usage(state: tauri::State<AppState>, id: String) -> Result<(), String> {
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
    let mut items = launcher_usage_snapshot(&state)
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
    drop(conn);
    invalidate_launcher_usage_cache();
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
    drop(conn);
    invalidate_launcher_usage_cache();
    Ok(())
}

fn hydrate_launcher_apps(
    state: &tauri::State<AppState>,
    records: Vec<LauncherRecord>,
) -> Vec<LauncherApp> {
    let usage = launcher_usage_snapshot(state);
    let mut apps = records
        .into_iter()
        .map(|record| {
            let record_usage = usage.get(&record.id);
            launcher_app_from_record(&record, record_usage)
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

fn launcher_app_from_record(record: &LauncherRecord, usage: Option<&LauncherUsage>) -> LauncherApp {
    let icon_data_url = record.icon_source.as_ref().and_then(|source| {
        crate::app_icons::AppIconService::global().icon_url(&record.name, source)
    });
    LauncherApp {
        id: record.id.clone(),
        name: record.name.clone(),
        subtitle: record.subtitle.clone(),
        keywords: record.keywords.clone(),
        icon_data_url,
        pinned: usage.is_some_and(|item| item.pinned),
        last_used_at: usage.and_then(|item| item.last_used_at.clone()),
        use_count: usage.map_or(0, |item| item.use_count),
    }
}

fn launcher_usage_snapshot(state: &AppState) -> HashMap<String, LauncherUsage> {
    const CACHE_TTL: Duration = Duration::from_secs(2);

    {
        let cache = launcher_usage_cache().read();
        if cache
            .loaded_at
            .is_some_and(|loaded_at| loaded_at.elapsed() < CACHE_TTL)
        {
            return cache.items.clone();
        }
    }

    let items = load_launcher_usage_from_db(state);
    let mut cache = launcher_usage_cache().write();
    cache.loaded_at = Some(Instant::now());
    cache.items = items.clone();
    items
}

fn invalidate_launcher_usage_cache() {
    launcher_usage_cache().write().loaded_at = None;
}

fn load_launcher_usage_from_db(state: &AppState) -> HashMap<String, LauncherUsage> {
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
    if records.is_empty() && !launcher_cache().read().is_empty() {
        tracing::warn!(
            platform = launcher_platform_key(),
            "launcher scan returned no records; keeping the previous snapshot"
        );
        return;
    }

    let changed = {
        let cache = launcher_cache().read();
        cache.len() != records.len()
            || cache.iter().zip(records.iter()).any(|(cached, next)| {
                cached.id != next.id
                    || cached.name != next.name
                    || cached.subtitle != next.subtitle
                    || cached.keywords != next.keywords
                    || cached.target != next.target
                    || cached.icon_source != next.icon_source
            })
    };
    if !changed {
        // Snapshot unchanged, but memory icon cache is cold after restart — warm from disk.
        prewarm_launcher_icons(records);
        return;
    }

    if let Some(state) = app.try_state::<AppState>() {
        let conn = state.db.lock();
        if let Err(error) = persist_launcher_index_snapshot(&conn, launcher_platform_key(), records)
        {
            tracing::warn!(
                platform = launcher_platform_key(),
                error = %error,
                "failed to persist launcher index snapshot"
            );
        }
    }

    *launcher_cache().write() = records.to_vec();
    crate::logging::debug_if_err(
        app.emit_to("main-panel", "launcher:index-ready", ()),
        "emit launcher index ready",
    );
    prewarm_launcher_icons(records);
}

fn prewarm_launcher_icons(records: &[LauncherRecord]) {
    let items: Vec<(String, String)> = records
        .iter()
        .filter_map(|record| {
            record
                .icon_source
                .as_ref()
                .map(|source| (record.name.clone(), source.clone()))
        })
        .collect();
    if items.is_empty() {
        return;
    }
    crate::logging::spawn_named("tempo-app-icon-prewarm", move || {
        let started = std::time::Instant::now();
        crate::app_icons::AppIconService::global().prewarm(&items);
        tracing::debug!(
            count = items.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "prewarmed launcher app icons"
        );
    });
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
    match enumerate_windows_apps_folder() {
        Ok(apps) => apps,
        Err(error) => {
            tracing::debug!(error = %error, "failed to enumerate shell:AppsFolder");
            Vec::new()
        }
    }
}

/// Enumerate Start Menu / Store apps via `shell:AppsFolder` (same source as Get-StartApps).
#[cfg(target_os = "windows")]
fn enumerate_windows_apps_folder() -> windows::core::Result<Vec<(String, String)>> {
    use windows::core::{Interface, GUID, HSTRING, PCWSTR};
    use windows::Win32::System::Com::IBindCtx;
    use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
    use windows::Win32::UI::Shell::{
        BHID_EnumItems, IEnumShellItems, IShellItem, IShellItem2, SHCreateItemFromParsingName,
        SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
    };

    // PKEY_AppUserModel_ID — System.AppUserModel.ID
    const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3),
        pid: 5,
    };

    let folder_path = HSTRING::from("shell:AppsFolder");
    let apps_folder: IShellItem =
        unsafe { SHCreateItemFromParsingName(PCWSTR(folder_path.as_ptr()), None::<&IBindCtx>)? };
    let enumerator: IEnumShellItems =
        unsafe { apps_folder.BindToHandler(None::<&IBindCtx>, &BHID_EnumItems)? };

    let mut apps = Vec::with_capacity(256);
    loop {
        let mut slot: [Option<IShellItem>; 1] = [None];
        let mut fetched = 0u32;
        let _ = unsafe { enumerator.Next(&mut slot, Some(&mut fetched as *mut u32)) };
        if fetched == 0 {
            break;
        }
        let Some(item) = slot[0].take() else {
            break;
        };

        let name = match unsafe { item.GetDisplayName(SIGDN_NORMALDISPLAY) } {
            Ok(pwstr) => shell_pwstr_to_string(pwstr),
            Err(_) => continue,
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            continue;
        }

        let app_id = item
            .cast::<IShellItem2>()
            .ok()
            .and_then(|item2| unsafe { item2.GetString(&PKEY_APP_USER_MODEL_ID).ok() })
            .map(shell_pwstr_to_string)
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                unsafe { item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING).ok() }
                    .map(shell_pwstr_to_string)
                    .filter(|value| !value.trim().is_empty())
            });
        let Some(app_id) = app_id.map(|value| value.trim().to_string()) else {
            continue;
        };

        apps.push((name, app_id));
    }

    Ok(apps)
}

#[cfg(target_os = "windows")]
fn shell_pwstr_to_string(p: windows::core::PWSTR) -> String {
    use windows::Win32::System::Com::CoTaskMemFree;

    if p.is_null() {
        return String::new();
    }
    let value = unsafe { p.to_string().unwrap_or_default() };
    unsafe { CoTaskMemFree(Some(p.0 as *const std::ffi::c_void)) };
    value
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
            let fs_name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if !is_launchable_name(&fs_name) {
                continue;
            }
            let target = path.to_string_lossy().into_owned();
            let (display_name, extra_keywords) =
                crate::platform::macos_app_launcher_identity(&path)
                    .unwrap_or_else(|| (fs_name.clone(), Vec::new()));
            if !is_launchable_name(&display_name) {
                continue;
            }
            // Dedup by bundle path so localized renames never collide with another app.
            records.entry(target.clone()).or_insert_with(|| {
                launcher_record(
                    display_name,
                    "macOS 应用",
                    target.clone(),
                    Some(target),
                    extra_keywords,
                )
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
    let search_index = SearchTextIndex::new(&name, &extra_keywords);
    LauncherRecord {
        id: launcher_id(&target),
        name,
        subtitle: subtitle.into(),
        keywords: extra_keywords,
        target,
        icon_source,
        search_index,
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
    use super::{
        finish_launcher_indexing, index_search_contributions, is_launchable_name,
        launcher_app_from_record, launcher_cache, launcher_id, launcher_record,
        load_launcher_index_snapshot, persist_launcher_index_snapshot, ranking_bonus,
        wait_for_launcher_index, MainPanelSearchContribution, MainPanelSearchContributionSource,
        MainPanelSearchMatch, MainPanelSearchMatchSource, INDEXING,
    };
    use std::sync::atomic::Ordering;

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

    #[test]
    fn rust_indexes_main_panel_contributions() {
        let indexed = index_search_contributions(vec![MainPanelSearchContribution {
            id: "translate".into(),
            name: "聚合翻译".into(),
            keywords: vec!["translate".into()],
            source: MainPanelSearchContributionSource::Builtin,
        }])
        .unwrap();

        assert_eq!(indexed.len(), 1);
        assert_eq!(indexed[0].usage_id, "builtin:translate");
        assert!(indexed[0].search_index.score("jhfy").is_some());
        assert!(indexed[0].search_index.score("juhefanyi").is_some());
    }

    #[test]
    fn usage_and_pin_bonus_cannot_cross_semantic_tiers() {
        assert!(ranking_bonus(true, i64::MAX) < 1_000);
        assert!(ranking_bonus(false, 100) > ranking_bonus(false, 1));
    }

    #[test]
    fn launcher_search_match_contains_a_renderable_app_snapshot() {
        let record = launcher_record(
            "Google Chrome".into(),
            "Windows 应用",
            "chrome.exe".into(),
            None,
            vec!["chrome".into()],
        );
        let app = launcher_app_from_record(&record, None);
        let result = MainPanelSearchMatch {
            source: MainPanelSearchMatchSource::Launcher,
            id: record.id,
            score: 100_000,
            app: Some(app),
        };

        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["source"], "launcher");
        assert_eq!(value["app"]["name"], "Google Chrome");
        assert_eq!(value["app"]["keywords"][0], "chrome");
    }

    #[test]
    fn cold_search_waits_for_the_in_flight_launcher_index() {
        launcher_cache().write().clear();
        INDEXING.store(true, Ordering::Release);
        let publisher = std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            launcher_cache().write().push(launcher_record(
                "Microsoft Edge".into(),
                "Windows 应用",
                "edge.exe".into(),
                None,
                vec!["edge".into()],
            ));
            finish_launcher_indexing();
        });

        wait_for_launcher_index();
        assert_eq!(launcher_cache().read().len(), 1);
        publisher.join().unwrap();
        launcher_cache().write().clear();
        INDEXING.store(false, Ordering::Release);
    }

    #[test]
    fn launcher_snapshots_are_persisted_and_isolated_by_platform() {
        let root = std::env::temp_dir().join(format!(
            "tempo-launcher-snapshot-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("tempo.db");
        let conn = crate::db::init_db(&path).unwrap();
        let windows = vec![launcher_record(
            "Google Chrome".into(),
            "Windows 应用",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe".into(),
            None,
            vec!["chrome".into()],
        )];
        let macos = vec![launcher_record(
            "Safari".into(),
            "macOS 应用",
            "/Applications/Safari.app".into(),
            None,
            Vec::new(),
        )];

        persist_launcher_index_snapshot(&conn, "windows", &windows).unwrap();
        persist_launcher_index_snapshot(&conn, "macos", &macos).unwrap();
        let restored_windows = load_launcher_index_snapshot(&conn, "windows").unwrap();
        let restored_macos = load_launcher_index_snapshot(&conn, "macos").unwrap();

        assert_eq!(restored_windows[0].name, "Google Chrome");
        assert!(restored_windows[0].search_index.score("chrome").is_some());
        assert_eq!(restored_macos[0].name, "Safari");
        assert!(restored_macos[0].search_index.score("safari").is_some());
        drop(conn);
        drop(std::fs::remove_dir_all(root));
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
