use crate::db::{
    active_todo_title, finalize_pomodoro_work_session, get_pomodoro_sessions_today, load_settings,
    start_pomodoro_work_session, AppState, PomodoroPhase, PomodoroRuntime, PomodoroState,
    PomodoroStatus, Settings,
};
use chrono::Local;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

pub fn pomodoro_state_snapshot(state: &AppState) -> PomodoroState {
    let (status, phase, remaining_seconds, phase_total_seconds, cycle_count, active_todo_id) = {
        let runtime = state.pomodoro.lock();
        (
            runtime.status,
            runtime.phase,
            runtime.remaining_seconds,
            runtime.phase_total_seconds,
            runtime.cycle_count,
            runtime.active_todo_id,
        )
    };

    let sessions_today = {
        let conn = state.db.lock();
        get_pomodoro_sessions_today(&conn)
    };

    let active_todo_title = active_todo_id.and_then(|todo_id| {
        let conn = state.db.lock();
        active_todo_title(&conn, todo_id)
    });

    PomodoroState {
        status: status_label(status).into(),
        phase: phase_label(phase).into(),
        remaining_seconds,
        phase_total_seconds,
        sessions_today,
        cycle_count,
        active_todo_id,
        active_todo_title,
    }
}

pub fn set_pomodoro_todo(state: &AppState, todo_id: Option<i64>) -> Result<PomodoroState, String> {
    {
        let runtime = state.pomodoro.lock();
        if runtime.status != PomodoroStatus::Idle {
            return Err("番茄钟进行中，暂不能更换绑定待办".into());
        }
    }

    if let Some(todo_id) = todo_id {
        let conn = state.db.lock();
        if active_todo_title(&conn, todo_id).is_none() {
            return Err("待办不存在或已完成".into());
        }
    }

    state.pomodoro.lock().active_todo_id = todo_id;
    Ok(pomodoro_state_snapshot(state))
}

pub fn tick_pomodoro(app: &AppHandle, state: &AppState, elapsed_seconds: i64) {
    let should_complete = {
        let mut runtime = state.pomodoro.lock();
        if runtime.status != PomodoroStatus::Running {
            return;
        }

        runtime.remaining_seconds = (runtime.remaining_seconds - elapsed_seconds).max(0);
        runtime.remaining_seconds == 0
    };

    push_pomodoro_update(app, state);

    if should_complete {
        complete_pomodoro_phase(app, state, false);
    }
}

pub fn start_pomodoro(state: &AppState, todo_id: Option<i64>) -> Result<PomodoroState, String> {
    let settings = {
        let conn = state.db.lock();
        load_settings(&conn)
    };

    {
        let mut runtime = state.pomodoro.lock();
        match runtime.status {
            PomodoroStatus::Idle => {
                if let Some(todo_id) = todo_id {
                    let conn = state.db.lock();
                    if active_todo_title(&conn, todo_id).is_none() {
                        return Err("待办不存在或已完成".into());
                    }
                    runtime.active_todo_id = Some(todo_id);
                }
                begin_phase(&mut runtime, PomodoroPhase::Work, &settings, state);
                runtime.status = PomodoroStatus::Running;
            }
            PomodoroStatus::Paused => {
                runtime.status = PomodoroStatus::Running;
            }
            PomodoroStatus::Running => {}
        }
    }

    Ok(pomodoro_state_snapshot(state))
}

pub fn pause_pomodoro(state: &AppState) -> PomodoroState {
    {
        let mut runtime = state.pomodoro.lock();
        if runtime.status == PomodoroStatus::Running {
            runtime.status = PomodoroStatus::Paused;
        }
    }

    pomodoro_state_snapshot(state)
}

pub fn stop_pomodoro(state: &AppState) -> PomodoroState {
    finalize_current_work_session(state, false, false);

    {
        let mut runtime = state.pomodoro.lock();
        *runtime = PomodoroRuntime::default();
    }

    pomodoro_state_snapshot(state)
}

pub fn skip_pomodoro_phase(app: &AppHandle, state: &AppState) -> PomodoroState {
    let active = {
        let runtime = state.pomodoro.lock();
        runtime.status != PomodoroStatus::Idle
    };

    if active {
        complete_pomodoro_phase(app, state, true);
    }

    pomodoro_state_snapshot(state)
}

fn complete_pomodoro_phase(app: &AppHandle, state: &AppState, skipped: bool) {
    let settings = {
        let conn = state.db.lock();
        load_settings(&conn)
    };

    let finished_phase = {
        let runtime = state.pomodoro.lock();
        runtime.phase
    };

    if finished_phase == PomodoroPhase::Work {
        finalize_current_work_session(state, !skipped, skipped);
    }

    let next_phase = match finished_phase {
        PomodoroPhase::Work => {
            let mut runtime = state.pomodoro.lock();
            runtime.cycle_count += 1;
            let cycle_count = runtime.cycle_count;
            drop(runtime);

            if cycle_count >= settings.pomodoro_sessions_per_cycle {
                PomodoroPhase::LongBreak
            } else {
                PomodoroPhase::ShortBreak
            }
        }
        PomodoroPhase::ShortBreak => PomodoroPhase::Work,
        PomodoroPhase::LongBreak => {
            state.pomodoro.lock().cycle_count = 0;
            PomodoroPhase::Work
        }
    };

    notify_phase_end(app, finished_phase, skipped);

    {
        let mut runtime = state.pomodoro.lock();
        begin_phase(&mut runtime, next_phase, &settings, state);
        runtime.status = PomodoroStatus::Running;
    }

    push_pomodoro_update(app, state);
}

fn begin_phase(
    runtime: &mut PomodoroRuntime,
    phase: PomodoroPhase,
    settings: &Settings,
    state: &AppState,
) {
    runtime.phase = phase;
    runtime.phase_total_seconds = phase_seconds(phase, settings);
    runtime.remaining_seconds = runtime.phase_total_seconds;

    if phase == PomodoroPhase::Work {
        let started_at = Local::now().to_rfc3339();
        let session_id = {
            let conn = state.db.lock();
            start_pomodoro_work_session(&conn, runtime.active_todo_id, &started_at).ok()
        };
        runtime.work_session_id = session_id;
    } else {
        runtime.work_session_id = None;
    }
}

fn finalize_current_work_session(state: &AppState, completed: bool, skipped: bool) {
    let (session_id, duration_seconds) = {
        let runtime = state.pomodoro.lock();
        let Some(session_id) = runtime.work_session_id else {
            return;
        };
        let duration_seconds = runtime.phase_total_seconds - runtime.remaining_seconds;
        (session_id, duration_seconds)
    };

    let ended_at = Local::now().to_rfc3339();
    {
        let conn = state.db.lock();
        finalize_pomodoro_work_session(
            &conn,
            session_id,
            &ended_at,
            duration_seconds,
            completed,
            skipped,
        );
    }

    state.pomodoro.lock().work_session_id = None;
}

fn phase_seconds(phase: PomodoroPhase, settings: &Settings) -> i64 {
    match phase {
        PomodoroPhase::Work => settings.pomodoro_work_minutes as i64 * 60,
        PomodoroPhase::ShortBreak => settings.pomodoro_short_break_minutes as i64 * 60,
        PomodoroPhase::LongBreak => settings.pomodoro_long_break_minutes as i64 * 60,
    }
}

fn notify_phase_end(app: &AppHandle, phase: PomodoroPhase, skipped: bool) {
    let phase_name = phase_label(phase);
    let message: String = match phase {
        PomodoroPhase::Work if skipped => "已跳过专注，开始休息".into(),
        PomodoroPhase::Work => "专注完成，该休息了！".into(),
        PomodoroPhase::ShortBreak if skipped => "已跳过短休，开始专注".into(),
        PomodoroPhase::ShortBreak => "短休结束，继续专注吧".into(),
        PomodoroPhase::LongBreak if skipped => "已跳过长休，开始专注".into(),
        PomodoroPhase::LongBreak => "长休结束，准备新一轮专注".into(),
    };

    emit_on_main(
        app,
        "reminder",
        json!({
            "type": "pomodoro_phase_end",
            "phase": phase_name,
            "skipped": skipped,
        }),
    );
    emit_on_main(app, "toast", json!({ "message": message }));

    focus_main_window(app);
}

fn focus_main_window(app: &AppHandle) {
    if crate::auxiliary_windows::is_pomodoro_float_visible(app) {
        return;
    }

    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    });
}

pub fn push_pomodoro_update(app: &AppHandle, state: &AppState) {
    let snapshot = pomodoro_state_snapshot(state);
    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = app_handle.emit("pomodoro-update", snapshot);
    });
}

fn emit_on_main(app: &AppHandle, event: &str, payload: serde_json::Value) {
    let app_handle = app.clone();
    let event = event.to_string();
    let _ = app.run_on_main_thread(move || {
        let _ = app_handle.emit(&event, payload);
    });
}

fn status_label(status: PomodoroStatus) -> &'static str {
    match status {
        PomodoroStatus::Idle => "idle",
        PomodoroStatus::Running => "running",
        PomodoroStatus::Paused => "paused",
    }
}

fn phase_label(phase: PomodoroPhase) -> &'static str {
    match phase {
        PomodoroPhase::Work => "work",
        PomodoroPhase::ShortBreak => "short_break",
        PomodoroPhase::LongBreak => "long_break",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ClipboardRuntime, TrackerState};
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
    fn start_pause_and_stop_return_without_deadlock() {
        let state = test_state();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let started = start_pomodoro(&state, None).expect("start pomodoro");
            let paused = pause_pomodoro(&state);
            let stopped = stop_pomodoro(&state);
            tx.send((started, paused, stopped)).ok();
        });

        let (started, paused, stopped) = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("pomodoro commands should return without deadlocking");

        assert_eq!(started.status, "running");
        assert_eq!(paused.status, "paused");
        assert_eq!(stopped.status, "idle");
    }
}
