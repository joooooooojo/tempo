use crate::db::{
    get_pomodoro_sessions_today, increment_pomodoro_sessions, load_settings, AppState,
    PomodoroPhase, PomodoroRuntime, PomodoroState, PomodoroStatus, Settings,
};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

pub fn pomodoro_state_snapshot(state: &AppState) -> PomodoroState {
    let (status, phase, remaining_seconds, phase_total_seconds, cycle_count) = {
        let runtime = state.pomodoro.lock();
        (
            runtime.status,
            runtime.phase,
            runtime.remaining_seconds,
            runtime.phase_total_seconds,
            runtime.cycle_count,
        )
    };

    let sessions_today = {
        let conn = state.db.lock();
        get_pomodoro_sessions_today(&conn)
    };

    PomodoroState {
        status: status_label(status).into(),
        phase: phase_label(phase).into(),
        remaining_seconds,
        phase_total_seconds,
        sessions_today,
        cycle_count,
    }
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

pub fn start_pomodoro(state: &AppState) -> Result<PomodoroState, String> {
    let settings = {
        let conn = state.db.lock();
        load_settings(&conn)
    };

    {
        let mut runtime = state.pomodoro.lock();
        match runtime.status {
            PomodoroStatus::Idle => {
                begin_phase(&mut runtime, PomodoroPhase::Work, &settings);
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

    let next_phase = match finished_phase {
        PomodoroPhase::Work => {
            let sessions_today = {
                let conn = state.db.lock();
                increment_pomodoro_sessions(&conn)
            };

            let mut runtime = state.pomodoro.lock();
            runtime.cycle_count += 1;
            let cycle_count = runtime.cycle_count;
            drop(runtime);

            let _ = sessions_today;
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
        begin_phase(&mut runtime, next_phase, &settings);
        runtime.status = PomodoroStatus::Running;
    }

    push_pomodoro_update(app, state);
}

fn begin_phase(runtime: &mut PomodoroRuntime, phase: PomodoroPhase, settings: &Settings) {
    runtime.phase = phase;
    runtime.phase_total_seconds = phase_seconds(phase, settings);
    runtime.remaining_seconds = runtime.phase_total_seconds;
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
    if snapshot.status == "idle" {
        return;
    }

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
    use crate::db::TrackerState;
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
            ",
        )
        .expect("create settings table");

        AppState {
            db: Arc::new(Mutex::new(conn)),
            tracker: Arc::new(Mutex::new(TrackerState::default())),
            pomodoro: Arc::new(Mutex::new(PomodoroRuntime::default())),
        }
    }

    #[test]
    fn start_pause_and_stop_return_without_deadlock() {
        let state = test_state();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let started = start_pomodoro(&state).expect("start pomodoro");
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
