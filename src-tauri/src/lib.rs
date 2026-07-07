mod commands;
mod db;
mod platform;

use db::{db_path, init_db, AppState, TrackerState};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
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
            };
            commands::start_tracker(app.handle().clone(), state.clone());
            app.manage(state);

            setup_tray(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
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
            commands::get_known_apps,
            commands::export_report,
            commands::complete_onboarding,
            commands::quit_app,
            commands::show_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "打开首页", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "清空当日数据", true, None::<&str>)?;
    let export = MenuItem::with_id(app, "export", "导出报表", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出软件", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &reset, &export, &quit])?;

    let _tray = TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
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
            "export" => {
                let _ = app.emit(
                    "toast",
                    serde_json::json!({ "message": "请在报表页面导出数据" }),
                );
                let _ = commands::show_window(app.clone());
            }
            "quit" => {
                let _ = app.emit("request-quit", ());
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let _ = commands::show_window(app.clone());
            }
        })
        .build(app)?;

    Ok(())
}
