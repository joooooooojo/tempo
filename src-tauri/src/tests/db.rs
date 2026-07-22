use crate::db::{
    add_screen_time, get_daily_total, init_db, load_settings, save_settings, set_setting,
    DEFAULT_CLIPBOARD_PICKER_SHORTCUT, DEFAULT_COMMAND_PALETTE_SHORTCUT,
    DEFAULT_SNIPPET_PICKER_SHORTCUT, MAX_HOURLY_SECONDS,
};
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
        assert_eq!(
            settings.shortcut_command_palette,
            DEFAULT_COMMAND_PALETTE_SHORTCUT
        );
        assert_eq!(
            settings.shortcut_clipboard_picker,
            DEFAULT_CLIPBOARD_PICKER_SHORTCUT
        );
        assert_eq!(
            settings.shortcut_snippet_picker,
            DEFAULT_SNIPPET_PICKER_SHORTCUT
        );
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
fn shortcut_settings_migrate_old_defaults_and_preserve_empty_bindings() {
    let path = temp_db_path("shortcut-settings");
    {
        let conn = init_db(&path).expect("init db");
        set_setting(&conn, "shortcut_quick_todo", "F2");
        set_setting(&conn, "shortcut_clipboard_picker", "F4");
        set_setting(&conn, "shortcut_snippet_picker", "F5");

        let mut settings = load_settings(&conn);
        assert_eq!(
            settings.shortcut_command_palette,
            DEFAULT_COMMAND_PALETTE_SHORTCUT
        );
        assert_eq!(
            settings.shortcut_clipboard_picker,
            DEFAULT_CLIPBOARD_PICKER_SHORTCUT
        );
        assert_eq!(
            settings.shortcut_snippet_picker,
            DEFAULT_SNIPPET_PICKER_SHORTCUT
        );

        settings.shortcut_command_palette.clear();
        save_settings(&conn, &settings);
        assert_eq!(load_settings(&conn).shortcut_command_palette, "");
    }

    if let Some(parent) = path.parent() {
        drop(std::fs::remove_dir_all(parent));
    }
}

#[test]
fn command_palette_previous_default_migrates_without_overwriting_custom_bindings() {
    let path = temp_db_path("command-palette-shortcut-migration");
    {
        let conn = init_db(&path).expect("init db");
        set_setting(&conn, "shortcut_command_palette", "Control+Shift+T");
        assert_eq!(
            load_settings(&conn).shortcut_command_palette,
            DEFAULT_COMMAND_PALETTE_SHORTCUT
        );

        set_setting(&conn, "shortcut_command_palette", "Control+Alt+P");
        assert_eq!(
            load_settings(&conn).shortcut_command_palette,
            "Control+Alt+P"
        );
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
