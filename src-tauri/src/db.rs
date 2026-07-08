use chrono::Local;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUsage {
    pub app_name: String,
    pub process_name: String,
    pub category: String,
    pub seconds: i64,
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub today_screen_seconds: i64,
    pub week_screen_seconds: i64,
    pub month_screen_seconds: i64,
    pub top_apps: Vec<AppUsage>,
    pub continuous_screen_seconds: i64,
    pub status_message: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppLimit {
    pub app_name: String,
    pub limit_seconds: i64,
    pub used_seconds: i64,
    pub warn_sent: bool,
    pub limit_sent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: i64,
    pub title: String,
    pub completed: bool,
    pub due_at: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub images: Vec<TodoImage>,
    pub notes: Vec<TodoNote>,
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
        }
    }
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
}

impl Default for PomodoroRuntime {
    fn default() -> Self {
        Self {
            status: PomodoroStatus::Idle,
            phase: PomodoroPhase::Work,
            remaining_seconds: 0,
            phase_total_seconds: 0,
            cycle_count: 0,
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

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub tracker: Arc<Mutex<TrackerState>>,
    pub pomodoro: Arc<Mutex<PomodoroRuntime>>,
}

pub fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub fn db_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("app data dir")
        .join("screen_time.db")
}

pub fn init_db(path: &PathBuf) -> Connection {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(path).expect("open db");
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
        CREATE TABLE IF NOT EXISTS blocked_apps (
            app_name TEXT PRIMARY KEY
        );
        CREATE TABLE IF NOT EXISTS app_limits (
            app_name TEXT PRIMARY KEY,
            limit_seconds INTEGER NOT NULL,
            warn_sent INTEGER NOT NULL DEFAULT 0,
            limit_sent INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            completed INTEGER NOT NULL DEFAULT 0,
            due_at TEXT,
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
        ",
    )
    .expect("init schema");
    conn.execute("ALTER TABLE app_usage ADD COLUMN icon_data_url TEXT", [])
        .ok();
    conn.execute("ALTER TABLE todos ADD COLUMN due_at TEXT", [])
        .ok();
    conn.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA busy_timeout=3000;",
    )
    .ok();
    conn
}

pub fn get_setting(conn: &Connection, key: &str, default: &str) -> String {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .unwrap_or_else(|_| default.to_string())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .ok();
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
}

pub fn get_pomodoro_sessions_today(conn: &Connection) -> u32 {
    let today = today_str();
    let stored_date = get_setting(conn, "pomodoro_sessions_date", "");
    if stored_date != today {
        return 0;
    }
    get_setting(conn, "pomodoro_sessions_count", "0")
        .parse()
        .unwrap_or(0)
}

pub fn increment_pomodoro_sessions(conn: &Connection) -> u32 {
    let today = today_str();
    let stored_date = get_setting(conn, "pomodoro_sessions_date", "");
    let count = if stored_date == today {
        get_setting(conn, "pomodoro_sessions_count", "0")
            .parse::<u32>()
            .unwrap_or(0)
            + 1
    } else {
        1
    };
    set_setting(conn, "pomodoro_sessions_date", &today);
    set_setting(conn, "pomodoro_sessions_count", &count.to_string());
    count
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

    if app_name == "screen-time-app"
        || app_name == "时窗"
        || process_name == "screen-time-app"
        || process_name.ends_with("screen-time-app")
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

pub fn add_screen_time(conn: &Connection, date: &str, hour: u32, seconds: i64) {
    if seconds <= 0 {
        return;
    }

    conn.execute(
        "INSERT INTO screen_time_daily (date, total_seconds) VALUES (?1, ?2)
         ON CONFLICT(date) DO UPDATE SET total_seconds = total_seconds + excluded.total_seconds",
        params![date, seconds],
    )
    .ok();
    conn.execute(
        "INSERT INTO screen_time_hourly (date, hour, seconds) VALUES (?1, ?2, ?3)
         ON CONFLICT(date, hour) DO UPDATE SET seconds = seconds + excluded.seconds",
        params![date, hour as i64, seconds],
    )
    .ok();
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
    conn.execute(
        "INSERT INTO app_usage (date, app_name, process_name, category, seconds, icon_data_url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(date, app_name) DO UPDATE SET
           seconds = seconds + excluded.seconds,
           process_name = excluded.process_name,
           category = excluded.category,
           icon_data_url = COALESCE(excluded.icon_data_url, app_usage.icon_data_url)",
        params![date, name, process, category, seconds, icon_data_url],
    )
    .ok();
}

pub fn is_blocked(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM blocked_apps WHERE app_name = ?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

pub fn sum_range(conn: &Connection, days: i64) -> i64 {
    let today = Local::now().date_naive();
    let start = today - chrono::Duration::days(days - 1);
    conn.query_row(
        "SELECT COALESCE(SUM(total_seconds), 0) FROM screen_time_daily
         WHERE date >= ?1 AND date <= ?2",
        params![
            start.format("%Y-%m-%d").to_string(),
            today.format("%Y-%m-%d").to_string()
        ],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

pub fn top_apps(conn: &Connection, date: &str, limit: i64) -> Vec<AppUsage> {
    let mut stmt = conn
        .prepare(
            "SELECT app_name, process_name, category, seconds, icon_data_url FROM app_usage
             WHERE date = ?1 ORDER BY seconds DESC",
        )
        .unwrap();
    stmt.query_map(params![date], |r| {
        Ok(AppUsage {
            app_name: r.get(0)?,
            process_name: r.get(1)?,
            category: r.get(2)?,
            seconds: r.get(3)?,
            icon_data_url: r.get(4)?,
        })
    })
    .unwrap()
    .filter_map(|x| x.ok())
    .filter(|app| !is_system_host_usage(&app.app_name, &app.process_name))
    .take(limit.max(0) as usize)
    .collect()
}

pub fn get_daily_total(conn: &Connection, date: &str) -> i64 {
    conn.query_row(
        "SELECT COALESCE(total_seconds, 0) FROM screen_time_daily WHERE date = ?1",
        [date],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

pub fn cleanup_old_data(conn: &Connection) {
    let cutoff = (Local::now().date_naive() - chrono::Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    conn.execute("DELETE FROM screen_time_daily WHERE date < ?1", [&cutoff])
        .ok();
    conn.execute("DELETE FROM screen_time_hourly WHERE date < ?1", [&cutoff])
        .ok();
    conn.execute("DELETE FROM app_usage WHERE date < ?1", [&cutoff])
        .ok();
}
