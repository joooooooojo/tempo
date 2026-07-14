use crate::db::{AppState, ClipboardRuntime, PomodoroRuntime, TrackerState};
use crate::pomodoro::{pause_pomodoro, start_pomodoro, stop_pomodoro};
use parking_lot::Mutex;
use rusqlite::Connection;
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};

fn test_state() -> AppState {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute_batch(
        "
        CREATE TABLE settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            completed INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE pomodoro_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            todo_id INTEGER,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            duration_seconds INTEGER NOT NULL DEFAULT 0,
            completed INTEGER NOT NULL DEFAULT 0,
            skipped INTEGER NOT NULL DEFAULT 0
        );
        ",
    )
    .expect("create test tables");

    AppState {
        db: Arc::new(Mutex::new(conn)),
        tracker: Arc::new(Mutex::new(TrackerState::default())),
        pomodoro: Arc::new(Mutex::new(PomodoroRuntime::default())),
        clipboard: Arc::new(Mutex::new(ClipboardRuntime::default())),
    }
}

#[test]
fn start_pause_and_stop_return_without_deadlocking() {
    let state = test_state();
    let (tx, rx) = mpsc::channel();

    crate::logging::spawn_named("tempo-test-pomodoro-deadlock", move || {
        let started = start_pomodoro(&state, None).expect("start pomodoro");
        let paused = pause_pomodoro(&state);
        let stopped = stop_pomodoro(&state);
        tx.send((started, paused, stopped))
            .expect("send pomodoro command results");
    });

    let (started, paused, stopped) = rx
        .recv_timeout(Duration::from_secs(1))
        .expect("pomodoro commands should return without deadlocking");

    assert_eq!(started.status, "running");
    assert_eq!(paused.status, "paused");
    assert_eq!(stopped.status, "idle");
}
