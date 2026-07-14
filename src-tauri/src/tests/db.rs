use crate::db::{add_screen_time, get_daily_total, init_db, load_settings, MAX_HOURLY_SECONDS};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir()
        .join(format!(
            "tempo-rust-test-{}-{}-{nanos}",
            test_name,
            std::process::id()
        ))
        .join("screen_time.db")
}

#[test]
fn init_db_creates_schema_and_is_idempotent() {
    let path = temp_db_path("init-idempotent");
    {
        let conn = init_db(&path).expect("init db");
        let settings = load_settings(&conn);
        assert_eq!(settings.clipboard_max_entries, 200);
    }
    {
        let conn = init_db(&path).expect("init db again");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'todos'",
                [],
                |row| row.get(0),
            )
            .expect("query sqlite schema");
        assert_eq!(count, 1);
    }

    if let Some(parent) = path.parent() {
        drop(std::fs::remove_dir_all(parent));
    }
}

#[test]
fn add_screen_time_caps_hourly_total() {
    let path = temp_db_path("screen-time-cap");
    {
        let conn = init_db(&path).expect("init db");

        let inserted = add_screen_time(&conn, "2026-01-01", 10, MAX_HOURLY_SECONDS + 60);
        assert_eq!(inserted, MAX_HOURLY_SECONDS);

        let inserted = add_screen_time(&conn, "2026-01-01", 10, 30);
        assert_eq!(inserted, 0);

        assert_eq!(get_daily_total(&conn, "2026-01-01"), MAX_HOURLY_SECONDS);
    }

    if let Some(parent) = path.parent() {
        drop(std::fs::remove_dir_all(parent));
    }
}
