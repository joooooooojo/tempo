use chrono::Local;
use parking_lot::Mutex;
use rusqlite::{params, Connection, Error as SqliteError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

pub const MAX_HOURLY_SECONDS: i64 = 60 * 60;
pub const MAX_DAILY_SECONDS: i64 = 24 * MAX_HOURLY_SECONDS;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUsage {
    pub app_name: String,
    pub process_name: String,
    pub category: String,
    pub seconds: i64,
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyData {
    pub hour: u32,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub date: String,
    pub total_seconds: i64,
    pub average_seconds: i64,
    pub peak_hour: u32,
    pub peak_seconds: i64,
    pub hourly: Vec<HourlyData>,
    pub top_apps: Vec<AppUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeeklyDay {
    pub date: String,
    pub seconds: i64,
    pub is_over_limit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeeklyReport {
    pub days: Vec<WeeklyDay>,
    pub average_seconds: i64,
    pub daily_limit_seconds: i64,
    pub top_apps: Vec<AppUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoSubtask {
    pub id: i64,
    pub todo_id: i64,
    pub title: String,
    pub completed: bool,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub completed: bool,
    pub due_at: Option<String>,
    pub pinned_at: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    #[serde(default = "default_recurrence")]
    pub recurrence: String,
    #[serde(default)]
    pub remind_1d: bool,
    #[serde(default)]
    pub remind_1h: bool,
    #[serde(default)]
    pub remind_custom_hours: Option<i64>,
    #[serde(default)]
    pub recurrence_root_id: Option<i64>,
    #[serde(default)]
    pub next_recurrence_at: Option<String>,
    #[serde(default)]
    pub images: Vec<TodoImage>,
    #[serde(default)]
    pub notes: Vec<TodoNote>,
    #[serde(default)]
    pub subtasks: Vec<TodoSubtask>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub image_count: u32,
    #[serde(default)]
    pub lightweight: bool,
}

fn default_recurrence() -> String {
    "none".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoImage {
    pub id: i64,
    pub todo_id: i64,
    pub data_url: String,
    pub mime_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoNote {
    pub id: i64,
    pub todo_id: i64,
    pub body: String,
    pub created_at: String,
    pub images: Vec<TodoNoteImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoNoteImage {
    pub id: i64,
    pub note_id: i64,
    pub data_url: String,
    pub mime_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub autostart: bool,
    pub sound_enabled: bool,
    pub theme: String,
    pub clipboard_monitor_enabled: bool,
    pub clipboard_max_entries: u32,
    pub clipboard_paste_mode: String,
    pub clipboard_plain_text_only: bool,
    pub clipboard_history_retention: String,
    pub shortcut_main_panel: String,
    pub shortcut_clipboard_picker: String,
    pub shortcut_snippet_picker: String,
    pub storage_dir: String,
    pub mcp_enabled: bool,
    pub mcp_port: u16,
    pub mcp_token: String,
    /// Builtin app ids hidden from the launcher (settings itself cannot be disabled).
    #[serde(default)]
    pub disabled_builtin_apps: Vec<String>,
}

pub const DEFAULT_MAIN_PANEL_SHORTCUT: &str = "Alt+Space";
pub const DEFAULT_CLIPBOARD_PICKER_SHORTCUT: &str = "Control+Shift+V";
pub const DEFAULT_SNIPPET_PICKER_SHORTCUT: &str = "Control+Shift+S";

impl Default for Settings {
    fn default() -> Self {
        Self {
            autostart: false,
            sound_enabled: false,
            theme: "system".into(),
            clipboard_monitor_enabled: true,
            clipboard_max_entries: 200,
            clipboard_paste_mode: "clipboard".into(),
            clipboard_plain_text_only: true,
            clipboard_history_retention: "days".into(),
            shortcut_main_panel: DEFAULT_MAIN_PANEL_SHORTCUT.into(),
            shortcut_clipboard_picker: DEFAULT_CLIPBOARD_PICKER_SHORTCUT.into(),
            shortcut_snippet_picker: DEFAULT_SNIPPET_PICKER_SHORTCUT.into(),
            storage_dir: String::new(),
            mcp_enabled: true,
            mcp_port: DEFAULT_MCP_PORT,
            mcp_token: String::new(),
            disabled_builtin_apps: Vec::new(),
        }
    }
}

pub const DEFAULT_MCP_PORT: u16 = 17832;

pub fn generate_mcp_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn normalize_mcp_port(value: u64) -> u16 {
    value.clamp(1024, 65535) as u16
}

pub fn normalize_disabled_builtin_apps(ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        let id = id.trim();
        if id.is_empty() || id == "settings" {
            continue;
        }
        if !out.iter().any(|existing| existing == id) {
            out.push(id.to_string());
        }
    }
    out
}

pub fn parse_disabled_builtin_apps(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Vec<String>>(raw) {
        Ok(ids) => normalize_disabled_builtin_apps(&ids),
        Err(error) => {
            tracing::warn!(error = %error, "failed to parse disabled_builtin_apps");
            Vec::new()
        }
    }
}

/// Recent user copy kept for the main panel (see `MAIN_PANEL_CLIPBOARD_SEED_MAX_AGE_MS`).
#[derive(Debug, Clone)]
pub struct RecentClipboardForMainPanel {
    pub captured_at_ms: i64,
    pub kind: String,
    pub text: Option<String>,
    pub entry_id: Option<i64>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
}

pub const MAIN_PANEL_CLIPBOARD_SEED_MAX_AGE_MS: i64 = 10_000;

#[derive(Debug, Default)]
pub struct ClipboardRuntime {
    pub skip_next_capture: bool,
    pub last_source_app: Option<String>,
    pub last_source_process: Option<String>,
    pub decoded_image_cache: HashMap<String, CachedClipboardImage>,
    pub decoded_image_cache_order: VecDeque<String>,
    pub decoded_image_cache_bytes: usize,
    pub recent_for_main_panel: Option<RecentClipboardForMainPanel>,
}

#[derive(Debug, Clone)]
pub struct CachedClipboardImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub clipboard: Arc<Mutex<ClipboardRuntime>>,
}

pub fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// User-facing data folder name. Keep in sync with `productName` in tauri.conf.json.
pub const APP_STORAGE_FOLDER_NAME: &str = "Tempo";

/// Primary SQLite database file under the Tempo storage root.
pub const DB_FILE_NAME: &str = "tempo.db";

#[derive(Debug, Serialize, Deserialize)]
struct StorageConfig {
    storage_dir: String,
}

pub fn default_storage_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app.path().data_dir().map_err(|e| e.to_string())?;
    Ok(data_dir.join(APP_STORAGE_FOLDER_NAME))
}

pub fn prepare_storage_dir(app: &AppHandle) -> Result<(), String> {
    let preferred = if has_custom_storage_config(app)? {
        current_storage_dir(app)?
    } else {
        default_storage_dir(app)?
    };
    std::fs::create_dir_all(&preferred).map_err(|e| e.to_string())?;
    Ok(())
}

fn has_custom_storage_config(app: &AppHandle) -> Result<bool, String> {
    let config_path = storage_config_path(app)?;
    let Ok(data) = std::fs::read_to_string(&config_path) else {
        return Ok(false);
    };
    let Ok(config) = serde_json::from_str::<StorageConfig>(&data) else {
        return Ok(false);
    };
    Ok(!config.storage_dir.trim().is_empty())
}

pub fn storage_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(default_storage_dir(app)?.join("storage.json"))
}

pub fn current_storage_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let config_path = storage_config_path(app)?;
    let default_dir = default_storage_dir(app)?;
    let Ok(data) = std::fs::read_to_string(&config_path) else {
        return Ok(default_dir);
    };
    let Ok(config) = serde_json::from_str::<StorageConfig>(&data) else {
        return Ok(default_dir);
    };
    let configured = config.storage_dir.trim();
    if configured.is_empty() {
        Ok(default_dir)
    } else {
        Ok(PathBuf::from(configured))
    }
}

pub fn save_storage_dir(app: &AppHandle, dir: &Path) -> Result<(), String> {
    let config_path = storage_config_path(app)?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let config = StorageConfig {
        storage_dir: dir.to_string_lossy().into_owned(),
    };
    let data = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(config_path, data).map_err(|e| e.to_string())
}

pub fn db_path(app: &AppHandle) -> PathBuf {
    match current_storage_dir(app).or_else(|_| default_storage_dir(app)) {
        Ok(storage_dir) => storage_dir.join(DB_FILE_NAME),
        Err(error) => {
            tracing::error!(error = %error, "failed to resolve storage directory");
            panic!("storage dir: {error}");
        }
    }
}

pub fn init_db(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let conn = Connection::open(path).map_err(|error| error.to_string())?;
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS tempo_daily (
            date TEXT PRIMARY KEY,
            total_seconds INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS tempo_hourly (
            date TEXT NOT NULL,
            hour INTEGER NOT NULL,
            seconds INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (date, hour)
        );
        CREATE TABLE IF NOT EXISTS app_usage (
            date TEXT NOT NULL,
            app_name TEXT NOT NULL,
            process_name TEXT NOT NULL DEFAULT '',
            category TEXT NOT NULL DEFAULT '系统程序',
            seconds INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (date, app_name)
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            completed INTEGER NOT NULL DEFAULT 0,
            due_at TEXT,
            pinned_at TEXT,
            created_at TEXT NOT NULL,
            completed_at TEXT,
            recurrence TEXT NOT NULL DEFAULT 'none',
            remind_1d INTEGER NOT NULL DEFAULT 0,
            remind_1h INTEGER NOT NULL DEFAULT 0,
            due_reminded_1d INTEGER NOT NULL DEFAULT 0,
            due_reminded_1h INTEGER NOT NULL DEFAULT 0,
            due_reminded_at INTEGER NOT NULL DEFAULT 0,
            remind_custom_hours INTEGER,
            due_reminded_custom INTEGER NOT NULL DEFAULT 0,
            recurrence_root_id INTEGER,
            next_recurrence_at TEXT,
            subtasks_completion_snapshot TEXT
        );
        CREATE TABLE IF NOT EXISTS todo_images (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            todo_id INTEGER NOT NULL,
            data_url TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(todo_id) REFERENCES todos(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS todo_notes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            todo_id INTEGER NOT NULL,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(todo_id) REFERENCES todos(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS todo_note_images (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            note_id INTEGER NOT NULL,
            data_url TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(note_id) REFERENCES todo_notes(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS todo_subtasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            todo_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            completed INTEGER NOT NULL DEFAULT 0,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            FOREIGN KEY(todo_id) REFERENCES todos(id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS todo_tags (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            todo_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(todo_id) REFERENCES todos(id) ON DELETE CASCADE,
            UNIQUE(todo_id, name)
        );
        CREATE TABLE IF NOT EXISTS clipboard_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'text',
            source_app TEXT,
            source_process TEXT,
            image_width INTEGER,
            image_height INTEGER,
            pinned INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS snippet_groups (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            color TEXT NOT NULL DEFAULT 'default',
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS snippets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            tags TEXT NOT NULL DEFAULT '[]',
            group_id INTEGER,
            shortcut TEXT,
            language TEXT,
            pinned INTEGER NOT NULL DEFAULT 0,
            use_count INTEGER NOT NULL DEFAULT 0,
            last_used_at TEXT,
            archived_at TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(group_id) REFERENCES snippet_groups(id) ON DELETE SET NULL
        );
        CREATE TABLE IF NOT EXISTS launcher_usage (
            item_id TEXT PRIMARY KEY,
            pinned_at TEXT,
            last_used_at TEXT,
            use_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS launcher_index_snapshots (
            platform TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS plugins (
            id TEXT PRIMARY KEY,
            current_version TEXT NOT NULL,
            pending_version TEXT,
            enabled INTEGER NOT NULL DEFAULT 0,
            runtime_state TEXT NOT NULL DEFAULT 'disabled',
            installed_at TEXT NOT NULL,
            updated_at TEXT,
            last_error TEXT
        );
        CREATE TABLE IF NOT EXISTS plugin_versions (
            plugin_id TEXT NOT NULL,
            version TEXT NOT NULL,
            package_hash TEXT,
            dev_path TEXT,
            display_publisher TEXT,
            verified_publisher_key TEXT,
            install_source TEXT NOT NULL,
            signature_status TEXT NOT NULL,
            trusted_at TEXT,
            installed_at TEXT NOT NULL,
            PRIMARY KEY (plugin_id, version)
        );
        CREATE TABLE IF NOT EXISTS publisher_trust (
            signing_key_id TEXT PRIMARY KEY,
            publisher_id TEXT NOT NULL,
            trusted_at TEXT NOT NULL,
            revoked_at TEXT
        );
        CREATE TABLE IF NOT EXISTS plugin_sessions (
            plugin_id TEXT NOT NULL,
            app_id TEXT NOT NULL,
            plugin_version TEXT NOT NULL,
            session_version INTEGER NOT NULL,
            payload TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (plugin_id, app_id)
        );
        CREATE TABLE IF NOT EXISTS plugin_storage (
            plugin_id TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (plugin_id, key)
        );
        CREATE TABLE IF NOT EXISTS plugin_mcp_exposure (
            plugin_id TEXT PRIMARY KEY,
            exposed INTEGER NOT NULL DEFAULT 0,
            toolset_fingerprint TEXT NOT NULL DEFAULT '',
            disabled_tools TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS builtin_mcp_exposure (
            app_id TEXT PRIMARY KEY,
            exposed INTEGER NOT NULL DEFAULT 1,
            disabled_tools TEXT NOT NULL DEFAULT '[]',
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_clipboard_history_created
            ON clipboard_history(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_launcher_usage_recent
            ON launcher_usage(pinned_at DESC, last_used_at DESC, use_count DESC);
        CREATE INDEX IF NOT EXISTS idx_snippets_usage
            ON snippets(pinned DESC, sort_order ASC, last_used_at DESC, updated_at DESC);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_snippets_shortcut
            ON snippets(shortcut)
            WHERE shortcut IS NOT NULL AND shortcut <> '';
        ",
    )
    .map_err(|error| error.to_string())?;
    conn.execute_batch(
        "BEGIN;
         INSERT OR IGNORE INTO settings (key, value)
         SELECT 'shortcut_main_panel', value
         FROM settings
         WHERE key = 'shortcut_command_palette';
         DELETE FROM settings WHERE key = 'shortcut_command_palette';
         COMMIT;",
    )
    .map_err(|error| error.to_string())?;
    if let Err(error) = conn
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA busy_timeout=3000;")
    {
        tracing::warn!(error = %error, "failed to apply database pragmas");
    }
    Ok(conn)
}

pub fn get_setting(conn: &Connection, key: &str, default: &str) -> String {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .unwrap_or_else(|_| default.to_string())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) {
    if let Err(error) = conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    ) {
        tracing::warn!(setting_key = key, error = %error, "failed to save setting");
    }
}

pub fn load_settings(conn: &Connection) -> Settings {
    let shortcut_main_panel = normalize_shortcut_setting(&get_setting(
        conn,
        "shortcut_main_panel",
        DEFAULT_MAIN_PANEL_SHORTCUT,
    ));
    let shortcut_clipboard_picker = normalize_shortcut_setting(&get_setting(
        conn,
        "shortcut_clipboard_picker",
        DEFAULT_CLIPBOARD_PICKER_SHORTCUT,
    ));
    let shortcut_snippet_picker = normalize_shortcut_setting(&get_setting(
        conn,
        "shortcut_snippet_picker",
        DEFAULT_SNIPPET_PICKER_SHORTCUT,
    ));

    Settings {
        autostart: get_setting(conn, "autostart", "false") == "true",
        sound_enabled: get_setting(conn, "sound_enabled", "false") == "true",
        theme: get_setting(conn, "theme", "system"),
        clipboard_monitor_enabled: get_setting(conn, "clipboard_monitor_enabled", "true") == "true",
        clipboard_max_entries: get_setting(conn, "clipboard_max_entries", "200")
            .parse()
            .unwrap_or(200)
            .clamp(1, 1000),
        clipboard_paste_mode: normalize_clipboard_paste_mode(&get_setting(
            conn,
            "clipboard_paste_mode",
            "clipboard",
        )),
        clipboard_plain_text_only: get_setting(conn, "clipboard_plain_text_only", "true") == "true",
        clipboard_history_retention: normalize_clipboard_history_retention(&get_setting(
            conn,
            "clipboard_history_retention",
            "days",
        )),
        shortcut_main_panel,
        shortcut_clipboard_picker,
        shortcut_snippet_picker,
        storage_dir: String::new(),
        mcp_enabled: get_setting(conn, "mcp_enabled", "true") == "true",
        mcp_port: normalize_mcp_port(
            get_setting(conn, "mcp_port", &DEFAULT_MCP_PORT.to_string())
                .parse()
                .unwrap_or(DEFAULT_MCP_PORT as u64),
        ),
        mcp_token: {
            let existing = get_setting(conn, "mcp_token", "");
            if existing.trim().is_empty() {
                let token = generate_mcp_token();
                set_setting(conn, "mcp_token", &token);
                token
            } else {
                existing
            }
        },
        disabled_builtin_apps: parse_disabled_builtin_apps(&get_setting(
            conn,
            "disabled_builtin_apps",
            "[]",
        )),
    }
}

pub fn save_settings(conn: &Connection, settings: &Settings) {
    set_setting(conn, "autostart", &settings.autostart.to_string());
    set_setting(conn, "sound_enabled", &settings.sound_enabled.to_string());
    set_setting(conn, "theme", &settings.theme);
    set_setting(
        conn,
        "clipboard_monitor_enabled",
        &settings.clipboard_monitor_enabled.to_string(),
    );
    set_setting(
        conn,
        "clipboard_max_entries",
        &settings.clipboard_max_entries.to_string(),
    );
    set_setting(conn, "clipboard_paste_mode", &settings.clipboard_paste_mode);
    set_setting(
        conn,
        "clipboard_plain_text_only",
        &settings.clipboard_plain_text_only.to_string(),
    );
    set_setting(
        conn,
        "clipboard_history_retention",
        &settings.clipboard_history_retention,
    );
    set_setting(conn, "shortcut_main_panel", &settings.shortcut_main_panel);
    set_setting(
        conn,
        "shortcut_clipboard_picker",
        &settings.shortcut_clipboard_picker,
    );
    set_setting(
        conn,
        "shortcut_snippet_picker",
        &settings.shortcut_snippet_picker,
    );
    set_setting(conn, "mcp_enabled", &settings.mcp_enabled.to_string());
    set_setting(conn, "mcp_port", &settings.mcp_port.to_string());
    set_setting(conn, "mcp_token", &settings.mcp_token);
    let disabled_builtin_apps = match serde_json::to_string(&settings.disabled_builtin_apps) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(error = %error, "failed to serialize disabled_builtin_apps");
            "[]".into()
        }
    };
    set_setting(conn, "disabled_builtin_apps", &disabled_builtin_apps);
}

pub fn normalize_clipboard_paste_mode(value: &str) -> String {
    match value {
        "active_app" => "active_app".into(),
        _ => "clipboard".into(),
    }
}

pub fn normalize_shortcut_setting(value: &str) -> String {
    value.trim().to_string()
}

pub fn normalize_clipboard_history_retention(value: &str) -> String {
    match value {
        "weeks" => "weeks".into(),
        "months" => "months".into(),
        "years" => "years".into(),
        "permanent" => "permanent".into(),
        _ => "days".into(),
    }
}

pub fn categorize(name: &str, process: &str) -> &'static str {
    let s = format!("{} {}", name, process).to_lowercase();
    if s.contains("chrome")
        || s.contains("firefox")
        || s.contains("edge")
        || s.contains("browser")
        || s.contains("浏览器")
    {
        "浏览器"
    } else if s.contains("code")
        || s.contains("word")
        || s.contains("excel")
        || s.contains("office")
        || s.contains("wps")
        || s.contains("notion")
        || s.contains("teams")
        || s.contains("slack")
        || s.contains("tempo")
    {
        "办公软件"
    } else if s.contains("steam")
        || s.contains("game")
        || s.contains("bilibili")
        || s.contains("youtube")
        || s.contains("music")
        || s.contains("spotify")
        || s.contains("video")
    {
        "娱乐软件"
    } else {
        "系统程序"
    }
}

pub fn is_system_host_usage(name: &str, process: &str) -> bool {
    let app_name = name.trim().to_lowercase();
    let process_name = process.trim().to_ascii_lowercase();

    if app_name.contains("windows 主进程")
        || app_name.contains("host process for windows")
        || app_name.contains("windows host process")
    {
        return true;
    }

    matches!(
        process_name.as_str(),
        "rundll32.exe"
            | "dllhost.exe"
            | "conhost.exe"
            | "taskhostw.exe"
            | "taskeng.exe"
            | "werfault.exe"
            | "sihost.exe"
            | "fontdrvhost.exe"
    )
}

pub fn add_tempo_time(conn: &Connection, date: &str, hour: u32, seconds: i64) -> i64 {
    if seconds <= 0 {
        return 0;
    }

    let current_hour_seconds: i64 = match conn.query_row(
        "SELECT COALESCE(seconds, 0) FROM tempo_hourly WHERE date = ?1 AND hour = ?2",
        params![date, hour as i64],
        |r| r.get(0),
    ) {
        Ok(seconds) => seconds,
        Err(SqliteError::QueryReturnedNoRows) => 0,
        Err(error) => {
            tracing::warn!(
                hour = hour,
                error = %error,
                "failed to load current hourly tempo usage"
            );
            0
        }
    };
    let seconds = seconds.min((MAX_HOURLY_SECONDS - current_hour_seconds).max(0));
    if seconds <= 0 {
        return 0;
    }

    if let Err(error) = conn.execute(
        "INSERT INTO tempo_daily (date, total_seconds) VALUES (?1, ?2)
         ON CONFLICT(date) DO UPDATE SET total_seconds = MIN(?3, total_seconds + excluded.total_seconds)",
        params![date, seconds, MAX_DAILY_SECONDS],
    ) {
        tracing::warn!(error = %error, "failed to upsert daily tempo usage");
    }
    if let Err(error) = conn.execute(
        "INSERT INTO tempo_hourly (date, hour, seconds) VALUES (?1, ?2, ?3)
         ON CONFLICT(date, hour) DO UPDATE SET seconds = MIN(?4, seconds + excluded.seconds)",
        params![date, hour as i64, seconds, MAX_HOURLY_SECONDS],
    ) {
        tracing::warn!(hour = hour, error = %error, "failed to upsert hourly tempo usage");
    }
    seconds
}

pub fn add_app_time(conn: &Connection, date: &str, name: &str, process: &str, seconds: i64) {
    if seconds <= 0 || is_system_host_usage(name, process) {
        return;
    }

    let category = categorize(name, process);
    if let Err(error) = conn.execute(
        "INSERT INTO app_usage (date, app_name, process_name, category, seconds)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(date, app_name) DO UPDATE SET
           seconds = seconds + excluded.seconds,
           process_name = excluded.process_name,
           category = excluded.category",
        params![date, name, process, category, seconds],
    ) {
        tracing::warn!(seconds = seconds, error = %error, "failed to upsert app usage");
    }
}

pub fn top_apps(conn: &Connection, date: &str, limit: i64) -> Vec<AppUsage> {
    let mut stmt = match conn.prepare(
        "SELECT app_name, process_name, category, seconds FROM app_usage
             WHERE date = ?1 ORDER BY seconds DESC",
    ) {
        Ok(stmt) => stmt,
        Err(error) => {
            tracing::warn!(error = %error, "failed to prepare top apps query");
            return Vec::new();
        }
    };
    let rows = match stmt.query_map(params![date], |r| {
        Ok(AppUsage {
            app_name: r.get(0)?,
            process_name: r.get(1)?,
            category: r.get(2)?,
            seconds: r.get(3)?,
            icon_data_url: None,
        })
    }) {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "failed to query top apps");
            return Vec::new();
        }
    };

    let mut apps = Vec::new();
    for row in rows {
        match row {
            Ok(app) if !is_system_host_usage(&app.app_name, &app.process_name) => apps.push(app),
            Ok(_) => {}
            Err(error) => tracing::warn!(error = %error, "failed to read top app row"),
        }
        if apps.len() >= limit.max(0) as usize {
            break;
        }
    }
    apps
}

pub fn get_daily_total(conn: &Connection, date: &str) -> i64 {
    match conn.query_row(
        "SELECT COALESCE(total_seconds, 0) FROM tempo_daily WHERE date = ?1",
        [date],
        |r| r.get(0),
    ) {
        Ok(total) => total,
        Err(SqliteError::QueryReturnedNoRows) => 0,
        Err(error) => {
            tracing::warn!(error = %error, "failed to load daily tempo usage total");
            0
        }
    }
    .clamp(0, MAX_DAILY_SECONDS)
}

pub fn cleanup_old_data(conn: &Connection) {
    let cutoff = (Local::now().date_naive() - chrono::Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    if let Err(error) = conn.execute("DELETE FROM tempo_daily WHERE date < ?1", [&cutoff]) {
        tracing::warn!(error = %error, "failed to cleanup old daily tempo usage");
    }
    if let Err(error) = conn.execute("DELETE FROM tempo_hourly WHERE date < ?1", [&cutoff]) {
        tracing::warn!(error = %error, "failed to cleanup old hourly tempo usage");
    }
    if let Err(error) = conn.execute("DELETE FROM app_usage WHERE date < ?1", [&cutoff]) {
        tracing::warn!(error = %error, "failed to cleanup old app usage");
    }
}
