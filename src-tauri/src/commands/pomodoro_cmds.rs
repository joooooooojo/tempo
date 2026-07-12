use crate::db::{AppState, PomodoroState};
use tauri::AppHandle;

#[tauri::command]
pub fn get_pomodoro_state(state: tauri::State<AppState>) -> PomodoroState {
    crate::pomodoro::pomodoro_state_snapshot(&state)
}

#[tauri::command]
pub fn set_pomodoro_todo(
    app: AppHandle,
    state: tauri::State<AppState>,
    todo_id: Option<i64>,
) -> Result<PomodoroState, String> {
    let snapshot = crate::pomodoro::set_pomodoro_todo(&state, todo_id)?;
    crate::pomodoro::push_pomodoro_update(&app, &state);
    Ok(snapshot)
}

#[tauri::command]
pub fn start_pomodoro(
    app: AppHandle,
    state: tauri::State<AppState>,
    todo_id: Option<i64>,
) -> Result<PomodoroState, String> {
    let snapshot = crate::pomodoro::start_pomodoro(&state, todo_id)?;
    crate::pomodoro::push_pomodoro_update(&app, &state);

    let _ = crate::auxiliary_windows::show_pomodoro_float_window(&app);

    Ok(snapshot)
}

#[tauri::command]
pub fn get_todo_focus_summary(
    state: tauri::State<AppState>,
    todo_id: i64,
) -> crate::db::TodoFocusSummary {
    let conn = state.db.lock();
    crate::db::get_todo_focus_summary(&conn, todo_id)
}

#[tauri::command]
pub fn get_todo_focus_summaries(
    state: tauri::State<AppState>,
    todo_ids: Vec<i64>,
) -> Vec<crate::db::TodoFocusSummary> {
    let conn = state.db.lock();
    crate::db::get_todo_focus_summaries(&conn, &todo_ids)
}

#[tauri::command]
pub fn pause_pomodoro(app: AppHandle, state: tauri::State<AppState>) -> PomodoroState {
    let snapshot = crate::pomodoro::pause_pomodoro(&state);
    crate::pomodoro::push_pomodoro_update(&app, &state);
    snapshot
}

#[tauri::command]
pub fn stop_pomodoro(app: AppHandle, state: tauri::State<AppState>) -> PomodoroState {
    let snapshot = crate::pomodoro::stop_pomodoro(&state);
    crate::pomodoro::push_pomodoro_update(&app, &state);
    snapshot
}

#[tauri::command]
pub fn skip_pomodoro_phase(app: AppHandle, state: tauri::State<AppState>) -> PomodoroState {
    crate::pomodoro::skip_pomodoro_phase(&app, &state)
}
