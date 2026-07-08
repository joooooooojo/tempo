mod commands;
mod db;
mod platform;
mod pomodoro;

#[cfg(target_os = "macos")]
mod macos_dock;
mod auxiliary_windows;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use db::{db_path, init_db, AppState, PomodoroRuntime, TrackerState};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const QUICK_TODO_SHORTCUT: &str = "F2";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        if let Err(error) = auxiliary_windows::show_quick_todo(app) {
                            let _ = app.emit(
                                "toast",
                                serde_json::json!({
                                    "message": format!("快速待办窗口打开失败: {error}")
                                }),
                            );
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            let path = db_path(app.handle());
            let conn = init_db(&path);
            let state = AppState {
                db: Arc::new(Mutex::new(conn)),
                tracker: Arc::new(Mutex::new(TrackerState::default())),
                pomodoro: Arc::new(Mutex::new(PomodoroRuntime::default())),
            };
            commands::start_tracker(app.handle().clone(), state.clone());
            app.manage(state);

            setup_tray(app)?;
            register_quick_todo_shortcut(app.handle());
            auxiliary_windows::precache_auxiliary_windows(app.handle())?;

            #[cfg(target_os = "macos")]
            {
                macos_dock::apply_branding(app.handle());
                let _ = app
                    .handle()
                    .set_activation_policy(tauri::ActivationPolicy::Regular);
            }

            if let Some(window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        commands::hide_to_tray(&app_handle);
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_daily_report,
            commands::get_weekly_report,
            commands::get_settings,
            commands::update_settings,
            commands::reset_today,
            commands::reset_all,
            commands::get_blocked_apps,
            commands::block_app,
            commands::unblock_app,
            commands::get_app_limits,
            commands::set_app_limit,
            commands::remove_app_limit,
            commands::get_todos,
            commands::add_todo,
            commands::update_todo_details,
            commands::set_todo_completed,
            commands::set_todo_pinned,
            commands::delete_todo_image,
            commands::add_todo_note,
            commands::delete_todo_note,
            commands::restore_todo_note,
            commands::delete_todo,
            commands::restore_todo,
            commands::get_known_apps,
            commands::export_todos_backup,
            commands::import_todos_backup,
            commands::save_markdown_image,
            commands::complete_onboarding,
            commands::quit_app,
            commands::hide_to_tray_command,
            commands::show_window,
            commands::get_pomodoro_state,
            commands::start_pomodoro,
            commands::pause_pomodoro,
            commands::stop_pomodoro,
            commands::skip_pomodoro_phase,
            auxiliary_windows::show_eye_care_overlay,
            auxiliary_windows::hide_eye_care_overlay,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn register_quick_todo_shortcut(app: &tauri::AppHandle) {
    if let Err(error) = app.global_shortcut().register(QUICK_TODO_SHORTCUT) {
        eprintln!("Failed to register {QUICK_TODO_SHORTCUT} global shortcut: {error}");
        let _ = app.emit(
            "toast",
            serde_json::json!({
                "message": format!("{QUICK_TODO_SHORTCUT} shortcut unavailable: {error}")
            }),
        );
    }
}

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "打开首页", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "清空当日数据", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出软件", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &reset, &quit])?;

    let mut tray_builder = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .tooltip("时窗: 加载中...")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                let _ = commands::show_window(app.clone());
            }
            "reset" => {
                if let Some(state) = app.try_state::<AppState>() {
                    commands::do_reset_today(&state);
                    let _ = app.emit("toast", serde_json::json!({ "message": "今日数据已清空" }));
                }
            }
            "quit" => {
                commands::quit_app(app.clone());
            }
            _ => {}
        });

    #[cfg(not(target_os = "macos"))]
    {
        tray_builder = tray_builder.on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let _ = commands::show_window(app.clone());
            }
        });
    }

    let tray = tray_builder.build(app)?;

    tray.with_inner_tray_icon(|inner| {
        inner.set_show_menu_on_right_click(!cfg!(target_os = "macos"));
    })?;

    Ok(())
}
