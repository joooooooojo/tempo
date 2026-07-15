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
    pub eye_care_enabled: bool,
    pub eye_care_interval_minutes: u32,
    pub night_reminder_enabled: bool,
    pub night_reminder_start: String,
    pub night_reminder_end: String,
    pub onboarding_completed: bool,
    pub pomodoro_work_minutes: u32,
    pub pomodoro_short_break_minutes: u32,
    pub pomodoro_long_break_minutes: u32,
    pub pomodoro_sessions_per_cycle: u32,
    pub pomodoro_float_enabled: bool,
    pub pomodoro_float_auto_show: bool,
    pub pomodoro_float_x: Option<i32>,
    pub pomodoro_float_y: Option<i32>,
    pub clipboard_monitor_enabled: bool,
    pub clipboard_max_entries: u32,
    pub clipboard_paste_mode: String,
    pub clipboard_plain_text_only: bool,
    pub clipboard_history_retention: String,
    pub shortcut_quick_todo: String,
    pub shortcut_clipboard_picker: String,
    pub shortcut_snippet_picker: String,
    pub storage_dir: String,
    pub mcp_enabled: bool,
    pub mcp_port: u16,
    pub mcp_token: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autostart: false,
            sound_enabled: false,
            theme: "system".into(),
            eye_care_enabled: true,
            eye_care_interval_minutes: 45,
            night_reminder_enabled: true,
            night_reminder_start: "23:00".into(),
            night_reminder_end: "06:00".into(),
            onboarding_completed: false,
            pomodoro_work_minutes: 25,
            pomodoro_short_break_minutes: 5,
            pomodoro_long_break_minutes: 15,
            pomodoro_sessions_per_cycle: 4,
            pomodoro_float_enabled: false,
            pomodoro_float_auto_show: true,
            pomodoro_float_x: None,
            pomodoro_float_y: None,
            clipboard_monitor_enabled: true,
            clipboard_max_entries: 200,
            clipboard_paste_mode: "clipboard".into(),
            clipboard_plain_text_only: true,
            clipboard_history_retention: "days".into(),
            shortcut_quick_todo: "F2".into(),
            shortcut_clipboard_picker: "F4".into(),
            shortcut_snippet_picker: "F5".into(),
            storage_dir: String::new(),
            mcp_enabled: true,
            mcp_port: DEFAULT_MCP_PORT,
            mcp_token: String::new(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PomodoroStatus {
    Idle,
    Running,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PomodoroPhase {
    Work,
    ShortBreak,
    LongBreak,
}

#[derive(Debug)]
pub struct PomodoroRuntime {
    pub status: PomodoroStatus,
    pub phase: PomodoroPhase,
    pub remaining_seconds: i64,
    pub phase_total_seconds: i64,
    pub cycle_count: u32,
    pub active_todo_id: Option<i64>,
    pub work_session_id: Option<i64>,
}

impl Default for PomodoroRuntime {
    fn default() -> Self {
        Self {
            status: PomodoroStatus::Idle,
            phase: PomodoroPhase::Work,
            remaining_seconds: 0,
            phase_total_seconds: 0,
            cycle_count: 0,
            active_todo_id: None,
            work_session_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomodoroState {
    pub status: String,
    pub phase: String,
    pub remaining_seconds: i64,
    pub phase_total_seconds: i64,
    pub sessions_today: u32,
    pub cycle_count: u32,
    pub active_todo_id: Option<i64>,
    pub active_todo_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoFocusSummary {
    pub todo_id: i64,
    pub sessions_today: u32,
    pub total_seconds_today: i64,
    pub total_seconds_all: i64,
    pub sessions_all: u32,
    pub last_focused_at: Option<String>,
}

pub struct TrackerState {
    pub continuous_seconds: i64,
    pub last_date: String,
    pub night_reminded_today: bool,
}

impl Default for TrackerState {
    fn default() -> Self {
        Self {
            continuous_seconds: 0,
            last_date: today_str(),
            night_reminded_today: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct ClipboardRuntime {
    pub skip_next_capture: bool,
    pub last_source_app: Option<String>,
    pub last_source_process: Option<String>,
    pub decoded_image_cache: HashMap<String, CachedClipboardImage>,
    pub decoded_image_cache_order: VecDeque<String>,
    pub decoded_image_cache_bytes: usize,
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
    pub tracker: Arc<Mutex<TrackerState>>,
    pub pomodoro: Arc<Mutex<PomodoroRuntime>>,
    pub clipboard: Arc<Mutex<ClipboardRuntime>>,
}

pub fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// User-facing data folder name. Keep in sync with `productName` in tauri.conf.json.
pub const APP_STORAGE_FOLDER_NAME: &str = "Tempo";

#[derive(Debug, Serialize, Deserialize)]
struct StorageConfig {
    storage_dir: String,
}

pub fn legacy_app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

pub fn default_storage_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app.path().data_dir().map_err(|e| e.to_string())?;
    Ok(data_dir.join(APP_STORAGE_FOLDER_NAME))
}

pub fn prepare_storage_dir(app: &AppHandle) -> Result<(), String> {
    if has_custom_storage_config(app)? {
        return Ok(());
    }

    let preferred = default_storage_dir(app)?;
    if preferred.join("screen_time.db").exists() {
        if let Some(parent) = preferred.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir_all(&preferred).map_err(|e| e.to_string())?;
        return Ok(());
    }

    let legacy = legacy_app_data_dir(app)?;
    if legacy.join("screen_time.db").exists() || storage_dir_has_data(&legacy) {
        if let Some(parent) = preferred.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        copy_storage_dir(&legacy, &preferred)?;
        return Ok(());
    }

    std::fs::create_dir_all(&preferred).map_err(|e| e.to_string())
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

fn storage_dir_has_data(path: &Path) -> bool {
    if path.join("markdown-images").exists() {
        return true;
    }

    std::fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or_else(|error| {
            tracing::debug!(error = %error, "failed to inspect storage directory");
            false
        })
}

fn copy_storage_dir(source: &Path, target: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if source_path.is_dir() {
            copy_storage_dir(&source_path, &target_path)?;
        } else if source_path.is_file() {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(&source_path, &target_path).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

pub fn storage_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("storage.json"))
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
        Ok(storage_dir) => storage_dir.join("screen_time.db"),
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
        CREATE TABLE IF NOT EXISTS screen_time_daily (
            date TEXT PRIMARY KEY,
            total_seconds INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS screen_time_hourly (
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
            icon_data_url TEXT,
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
            completed_at TEXT
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
        CREATE TABLE IF NOT EXISTS pomodoro_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            todo_id INTEGER,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            duration_seconds INTEGER NOT NULL DEFAULT 0,
            completed INTEGER NOT NULL DEFAULT 0,
            skipped INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(todo_id) REFERENCES todos(id) ON DELETE SET NULL
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
            pinned INTEGER NOT NULL DEFAULT 0,
            use_count INTEGER NOT NULL DEFAULT 0,
            last_used_at TEXT,
            archived_at TEXT,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(group_id) REFERENCES snippet_groups(id) ON DELETE SET NULL
        );
        CREATE INDEX IF NOT EXISTS idx_clipboard_history_created
            ON clipboard_history(created_at DESC);
        ",
    )
    .map_err(|error| error.to_string())?;
    log_optional_migration(
        conn.execute(
            "ALTER TABLE clipboard_history ADD COLUMN image_width INTEGER",
            [],
        ),
        "add clipboard image width column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE clipboard_history ADD COLUMN image_height INTEGER",
            [],
        ),
        "add clipboard image height column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE clipboard_history ADD COLUMN source_process TEXT",
            [],
        ),
        "add clipboard source process column",
    );
    log_optional_migration(
        conn.execute("ALTER TABLE app_usage ADD COLUMN icon_data_url TEXT", []),
        "add app icon data column",
    );
    log_optional_migration(
        conn.execute("ALTER TABLE todos ADD COLUMN due_at TEXT", []),
        "add todo due_at column",
    );
    log_optional_migration(
        conn.execute("ALTER TABLE todos ADD COLUMN pinned_at TEXT", []),
        "add todo pinned_at column",
    );
    let added_todo_content = log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN content TEXT NOT NULL DEFAULT ''",
            [],
        ),
        "add todo content column",
    );
    if added_todo_content {
        log_optional_migration(
            conn.execute("UPDATE todos SET content = title WHERE content = ''", []),
            "backfill todo content column",
        );
    }
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN recurrence TEXT NOT NULL DEFAULT 'none'",
            [],
        ),
        "add todo recurrence column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN remind_1d INTEGER NOT NULL DEFAULT 0",
            [],
        ),
        "add todo remind_1d column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN remind_1h INTEGER NOT NULL DEFAULT 0",
            [],
        ),
        "add todo remind_1h column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN due_reminded_1d INTEGER NOT NULL DEFAULT 0",
            [],
        ),
        "add todo due_reminded_1d column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN due_reminded_1h INTEGER NOT NULL DEFAULT 0",
            [],
        ),
        "add todo due_reminded_1h column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN due_reminded_at INTEGER NOT NULL DEFAULT 0",
            [],
        ),
        "add todo due_reminded_at column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN remind_custom_hours INTEGER",
            [],
        ),
        "add todo custom reminder column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN due_reminded_custom INTEGER NOT NULL DEFAULT 0",
            [],
        ),
        "add todo custom reminder sent column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN recurrence_root_id INTEGER",
            [],
        ),
        "add todo recurrence root column",
    );
    log_optional_migration(
        conn.execute("ALTER TABLE todos ADD COLUMN next_recurrence_at TEXT", []),
        "add todo next recurrence column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE todos ADD COLUMN subtasks_completion_snapshot TEXT",
            [],
        ),
        "add todo subtasks completion snapshot column",
    );
    log_optional_migration(
        conn.execute("ALTER TABLE snippets ADD COLUMN group_id INTEGER", []),
        "add snippet group column",
    );
    log_optional_migration(
        conn.execute("ALTER TABLE snippets ADD COLUMN shortcut TEXT", []),
        "add snippet shortcut column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE snippets ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            [],
        ),
        "add snippet pinned column",
    );
    log_optional_migration(
        conn.execute(
            "ALTER TABLE snippets ADD COLUMN use_count INTEGER NOT NULL DEFAULT 0",
            [],
        ),
        "add snippet use_count column",
    );
    log_optional_migration(
        conn.execute("ALTER TABLE snippets ADD COLUMN last_used_at TEXT", []),
        "add snippet last_used_at column",
    );
    log_optional_migration(
        conn.execute("ALTER TABLE snippets ADD COLUMN archived_at TEXT", []),
        "add snippet archived_at column",
    );
    log_optional_migration(
        conn.execute("ALTER TABLE snippets ADD COLUMN language TEXT", []),
        "add snippet language column",
    );
    log_optional_migration(
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_snippets_usage
         ON snippets(pinned DESC, sort_order ASC, last_used_at DESC, updated_at DESC)",
            [],
        ),
        "create snippet usage index",
    );
    log_optional_migration(
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_snippets_shortcut
         ON snippets(shortcut)
         WHERE shortcut IS NOT NULL AND shortcut <> ''",
            [],
        ),
        "create snippet shortcut index",
    );
    log_optional_migration(
        conn.execute(
            "UPDATE todos
         SET due_at = NULL,
             remind_1d = 0,
             remind_1h = 0,
             remind_custom_hours = NULL
         WHERE recurrence != 'none'",
            [],
        ),
        "normalize recurring todo due reminders",
    );
    log_optional_migration(
        conn.execute(
            "UPDATE todos
         SET recurrence_root_id = id
         WHERE recurrence != 'none' AND recurrence_root_id IS NULL",
            [],
        ),
        "backfill recurrence root id",
    );
    if let Err(error) = conn
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA busy_timeout=3000;")
    {
        tracing::warn!(error = %error, "failed to apply database pragmas");
    }
    Ok(conn)
}

fn log_optional_migration<T>(result: rusqlite::Result<T>, migration: &'static str) -> bool {
    match result {
        Ok(_) => true,
        Err(error) => {
            tracing::debug!(
                migration = %migration,
                error = %error,
                "database migration skipped or failed"
            );
            false
        }
    }
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
    Settings {
        autostart: get_setting(conn, "autostart", "false") == "true",
        sound_enabled: get_setting(conn, "sound_enabled", "false") == "true",
        theme: get_setting(conn, "theme", "system"),
        eye_care_enabled: get_setting(conn, "eye_care_enabled", "true") == "true",
        eye_care_interval_minutes: get_setting(conn, "eye_care_interval_minutes", "45")
            .parse()
            .unwrap_or(45),
        night_reminder_enabled: get_setting(conn, "night_reminder_enabled", "true") == "true",
        night_reminder_start: get_setting(conn, "night_reminder_start", "23:00"),
        night_reminder_end: get_setting(conn, "night_reminder_end", "06:00"),
        onboarding_completed: get_setting(conn, "onboarding_completed", "false") == "true",
        pomodoro_work_minutes: get_setting(conn, "pomodoro_work_minutes", "25")
            .parse()
            .unwrap_or(25),
        pomodoro_short_break_minutes: get_setting(conn, "pomodoro_short_break_minutes", "5")
            .parse()
            .unwrap_or(5),
        pomodoro_long_break_minutes: get_setting(conn, "pomodoro_long_break_minutes", "15")
            .parse()
            .unwrap_or(15),
        pomodoro_sessions_per_cycle: get_setting(conn, "pomodoro_sessions_per_cycle", "4")
            .parse()
            .unwrap_or(4),
        pomodoro_float_enabled: get_setting(conn, "pomodoro_float_enabled", "false") == "true",
        pomodoro_float_auto_show: get_setting(conn, "pomodoro_float_auto_show", "true") == "true",
        pomodoro_float_x: {
            let raw = get_setting(conn, "pomodoro_float_x", "");
            if raw.is_empty() {
                None
            } else {
                match raw.parse() {
                    Ok(value) => Some(value),
                    Err(error) => {
                        tracing::debug!(
                            setting_key = "pomodoro_float_x",
                            error = %error,
                            "failed to parse numeric setting"
                        );
                        None
                    }
                }
            }
        },
        pomodoro_float_y: {
            let raw = get_setting(conn, "pomodoro_float_y", "");
            if raw.is_empty() {
                None
            } else {
                match raw.parse() {
                    Ok(value) => Some(value),
                    Err(error) => {
                        tracing::debug!(
                            setting_key = "pomodoro_float_y",
                            error = %error,
                            "failed to parse numeric setting"
                        );
                        None
                    }
                }
            }
        },
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
        shortcut_quick_todo: normalize_shortcut_setting(
            &get_setting(conn, "shortcut_quick_todo", "F2"),
            "F2",
        ),
        shortcut_clipboard_picker: normalize_shortcut_setting(
            &get_setting(conn, "shortcut_clipboard_picker", "F4"),
            "F4",
        ),
        shortcut_snippet_picker: normalize_shortcut_setting(
            &get_setting(conn, "shortcut_snippet_picker", "F5"),
            "F5",
        ),
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
    }
}

pub fn save_settings(conn: &Connection, settings: &Settings) {
    set_setting(conn, "autostart", &settings.autostart.to_string());
    set_setting(conn, "sound_enabled", &settings.sound_enabled.to_string());
    set_setting(conn, "theme", &settings.theme);
    set_setting(
        conn,
        "eye_care_enabled",
        &settings.eye_care_enabled.to_string(),
    );
    set_setting(
        conn,
        "eye_care_interval_minutes",
        &settings.eye_care_interval_minutes.to_string(),
    );
    set_setting(
        conn,
        "night_reminder_enabled",
        &settings.night_reminder_enabled.to_string(),
    );
    set_setting(conn, "night_reminder_start", &settings.night_reminder_start);
    set_setting(conn, "night_reminder_end", &settings.night_reminder_end);
    set_setting(
        conn,
        "onboarding_completed",
        &settings.onboarding_completed.to_string(),
    );
    set_setting(
        conn,
        "pomodoro_work_minutes",
        &settings.pomodoro_work_minutes.to_string(),
    );
    set_setting(
        conn,
        "pomodoro_short_break_minutes",
        &settings.pomodoro_short_break_minutes.to_string(),
    );
    set_setting(
        conn,
        "pomodoro_long_break_minutes",
        &settings.pomodoro_long_break_minutes.to_string(),
    );
    set_setting(
        conn,
        "pomodoro_sessions_per_cycle",
        &settings.pomodoro_sessions_per_cycle.to_string(),
    );
    set_setting(
        conn,
        "pomodoro_float_enabled",
        &settings.pomodoro_float_enabled.to_string(),
    );
    set_setting(
        conn,
        "pomodoro_float_auto_show",
        &settings.pomodoro_float_auto_show.to_string(),
    );
    if let Some(x) = settings.pomodoro_float_x {
        set_setting(conn, "pomodoro_float_x", &x.to_string());
    }
    if let Some(y) = settings.pomodoro_float_y {
        set_setting(conn, "pomodoro_float_y", &y.to_string());
    }
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
    set_setting(conn, "shortcut_quick_todo", &settings.shortcut_quick_todo);
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
}

pub fn normalize_clipboard_paste_mode(value: &str) -> String {
    match value {
        "active_app" => "active_app".into(),
        _ => "clipboard".into(),
    }
}

pub fn normalize_shortcut_setting(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    trimmed.to_string()
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

pub fn get_pomodoro_sessions_today(conn: &Connection) -> u32 {
    let today = today_str();
    match conn.query_row(
        "SELECT COUNT(*) FROM pomodoro_sessions
         WHERE date(started_at) = ?1
           AND ended_at IS NOT NULL
           AND (completed = 1 OR skipped = 1)",
        [today],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(count) => count.max(0) as u32,
        Err(error) => {
            tracing::warn!(error = %error, "failed to count today's pomodoro sessions");
            0
        }
    }
}

pub fn active_todo_title(conn: &Connection, todo_id: i64) -> Option<String> {
    match conn.query_row(
        "SELECT title FROM todos WHERE id = ?1 AND completed = 0",
        [todo_id],
        |row| row.get(0),
    ) {
        Ok(title) => Some(title),
        Err(SqliteError::QueryReturnedNoRows) => None,
        Err(error) => {
            tracing::warn!(
                todo_id = todo_id,
                error = %error,
                "failed to load active todo title"
            );
            None
        }
    }
}

pub fn start_pomodoro_work_session(
    conn: &Connection,
    todo_id: Option<i64>,
    started_at: &str,
) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO pomodoro_sessions (todo_id, started_at, duration_seconds, completed, skipped)
         VALUES (?1, ?2, 0, 0, 0)",
        params![todo_id, started_at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn finalize_pomodoro_work_session(
    conn: &Connection,
    session_id: i64,
    ended_at: &str,
    duration_seconds: i64,
    completed: bool,
    skipped: bool,
) {
    if let Err(error) = conn.execute(
        "UPDATE pomodoro_sessions
         SET ended_at = ?1,
             duration_seconds = ?2,
             completed = ?3,
             skipped = ?4
         WHERE id = ?5",
        params![
            ended_at,
            duration_seconds.max(0),
            if completed { 1 } else { 0 },
            if skipped { 1 } else { 0 },
            session_id
        ],
    ) {
        tracing::warn!(
            session_id = session_id,
            error = %error,
            "failed to finalize pomodoro work session"
        );
    }
}

pub fn get_todo_focus_summary(conn: &Connection, todo_id: i64) -> TodoFocusSummary {
    let today = today_str();
    match conn.query_row(
        "SELECT
            COALESCE(SUM(CASE WHEN completed = 1 AND date(started_at) = ?1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN date(started_at) = ?1 THEN duration_seconds ELSE 0 END), 0),
            COALESCE(SUM(duration_seconds), 0),
            COALESCE(SUM(CASE WHEN completed = 1 THEN 1 ELSE 0 END), 0),
            MAX(started_at)
         FROM pomodoro_sessions
         WHERE todo_id = ?2",
        params![today, todo_id],
        |row| {
            Ok(TodoFocusSummary {
                todo_id,
                sessions_today: row.get::<_, i64>(0)? as u32,
                total_seconds_today: row.get(1)?,
                total_seconds_all: row.get(2)?,
                sessions_all: row.get::<_, i64>(3)? as u32,
                last_focused_at: row.get(4)?,
            })
        },
    ) {
        Ok(summary) => summary,
        Err(error) => {
            tracing::warn!(
                todo_id = todo_id,
                error = %error,
                "failed to load todo focus summary"
            );
            TodoFocusSummary {
                todo_id,
                sessions_today: 0,
                total_seconds_today: 0,
                total_seconds_all: 0,
                sessions_all: 0,
                last_focused_at: None,
            }
        }
    }
}

pub fn get_todo_focus_summaries(conn: &Connection, todo_ids: &[i64]) -> Vec<TodoFocusSummary> {
    todo_ids
        .iter()
        .map(|todo_id| get_todo_focus_summary(conn, *todo_id))
        .collect()
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
    let app_stem = app_name.strip_suffix(".exe").unwrap_or(&app_name);
    let process_stem = process_name.strip_suffix(".exe").unwrap_or(&process_name);

    if app_stem == "tempo"
        || process_stem == "tempo"
        || process_stem.ends_with("\\tempo")
        || process_stem.ends_with("/tempo")
    {
        return true;
    }

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

pub fn add_screen_time(conn: &Connection, date: &str, hour: u32, seconds: i64) -> i64 {
    if seconds <= 0 {
        return 0;
    }

    let current_hour_seconds: i64 = match conn.query_row(
        "SELECT COALESCE(seconds, 0) FROM screen_time_hourly WHERE date = ?1 AND hour = ?2",
        params![date, hour as i64],
        |r| r.get(0),
    ) {
        Ok(seconds) => seconds,
        Err(SqliteError::QueryReturnedNoRows) => 0,
        Err(error) => {
            tracing::warn!(
                hour = hour,
                error = %error,
                "failed to load current hourly screen time"
            );
            0
        }
    };
    let seconds = seconds.min((MAX_HOURLY_SECONDS - current_hour_seconds).max(0));
    if seconds <= 0 {
        return 0;
    }

    if let Err(error) = conn.execute(
        "INSERT INTO screen_time_daily (date, total_seconds) VALUES (?1, ?2)
         ON CONFLICT(date) DO UPDATE SET total_seconds = MIN(?3, total_seconds + excluded.total_seconds)",
        params![date, seconds, MAX_DAILY_SECONDS],
    ) {
        tracing::warn!(error = %error, "failed to upsert daily screen time");
    }
    if let Err(error) = conn.execute(
        "INSERT INTO screen_time_hourly (date, hour, seconds) VALUES (?1, ?2, ?3)
         ON CONFLICT(date, hour) DO UPDATE SET seconds = MIN(?4, seconds + excluded.seconds)",
        params![date, hour as i64, seconds, MAX_HOURLY_SECONDS],
    ) {
        tracing::warn!(hour = hour, error = %error, "failed to upsert hourly screen time");
    }
    seconds
}

pub fn add_app_time(
    conn: &Connection,
    date: &str,
    name: &str,
    process: &str,
    seconds: i64,
    icon_data_url: Option<&str>,
) {
    if seconds <= 0 || is_system_host_usage(name, process) {
        return;
    }

    let category = categorize(name, process);
    if let Err(error) = conn.execute(
        "INSERT INTO app_usage (date, app_name, process_name, category, seconds, icon_data_url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(date, app_name) DO UPDATE SET
           seconds = seconds + excluded.seconds,
           process_name = excluded.process_name,
           category = excluded.category,
           icon_data_url = COALESCE(excluded.icon_data_url, app_usage.icon_data_url)",
        params![date, name, process, category, seconds, icon_data_url],
    ) {
        tracing::warn!(seconds = seconds, error = %error, "failed to upsert app usage");
    }
}

pub fn top_apps(conn: &Connection, date: &str, limit: i64) -> Vec<AppUsage> {
    let mut stmt = match conn.prepare(
        "SELECT app_name, process_name, category, seconds, icon_data_url FROM app_usage
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
            icon_data_url: r.get(4)?,
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
        "SELECT COALESCE(total_seconds, 0) FROM screen_time_daily WHERE date = ?1",
        [date],
        |r| r.get(0),
    ) {
        Ok(total) => total,
        Err(SqliteError::QueryReturnedNoRows) => 0,
        Err(error) => {
            tracing::warn!(error = %error, "failed to load daily screen time total");
            0
        }
    }
    .clamp(0, MAX_DAILY_SECONDS)
}

pub fn cleanup_old_data(conn: &Connection) {
    let cutoff = (Local::now().date_naive() - chrono::Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    if let Err(error) = conn.execute("DELETE FROM screen_time_daily WHERE date < ?1", [&cutoff]) {
        tracing::warn!(error = %error, "failed to cleanup old daily screen time");
    }
    if let Err(error) = conn.execute("DELETE FROM screen_time_hourly WHERE date < ?1", [&cutoff]) {
        tracing::warn!(error = %error, "failed to cleanup old hourly screen time");
    }
    if let Err(error) = conn.execute("DELETE FROM app_usage WHERE date < ?1", [&cutoff]) {
        tracing::warn!(error = %error, "failed to cleanup old app usage");
    }
}
