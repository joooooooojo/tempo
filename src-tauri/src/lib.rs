mod commands;
mod db;
mod platform;
mod pomodoro;
mod staged_update;

mod app_icons;
mod asset_protocol;
mod auxiliary_windows;
mod clipboard_db;
mod clipboard_images;
mod clipboard_watcher;
mod logging;
#[cfg(target_os = "macos")]
mod macos_dock;
#[cfg(target_os = "macos")]
mod macos_overlay_panel;
mod todo_images;
mod tray_menu;

#[cfg(test)]
mod tests;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use db::{db_path, init_db, AppState, PomodoroRuntime, TrackerState};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const QUICK_TODO_SHORTCUT: &str = "F2";
const CLIPBOARD_PICKER_SHORTCUT: &str = "F4";
const SNIPPET_PICKER_SHORTCUT: &str = "F5";
const SHELF_ESCAPE_SHORTCUT: &str = "Escape";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::install_panic_hook();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .register_uri_scheme_protocol(commands::MARKDOWN_IMAGE_PROTOCOL, |ctx, request| {
            commands::markdown_image_protocol_response(ctx.app_handle(), request)
        })
        .register_uri_scheme_protocol(
            clipboard_images::CLIPBOARD_IMAGE_PROTOCOL,
            |ctx, request| {
                clipboard_images::clipboard_image_protocol_response(ctx.app_handle(), request)
            },
        )
        .register_uri_scheme_protocol(todo_images::TODO_IMAGE_PROTOCOL, |ctx, request| {
            todo_images::todo_image_protocol_response(ctx.app_handle(), request)
        })
        .register_uri_scheme_protocol(app_icons::APP_ICON_PROTOCOL, |ctx, request| {
            app_icons::app_icon_protocol_response(ctx.app_handle(), request)
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init());

    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    let result = builder
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let id = shortcut.to_string();
                    let dispatch_id = id.clone();
                    let app = app.clone();
                    let app_for_main = app.clone();
                    if let Err(error) = app.run_on_main_thread(move || {
                        let result = match dispatch_id.as_str() {
                            QUICK_TODO_SHORTCUT => {
                                auxiliary_windows::show_quick_todo(&app_for_main)
                            }
                            CLIPBOARD_PICKER_SHORTCUT => {
                                auxiliary_windows::show_clipboard_picker_window(&app_for_main)
                            }
                            SNIPPET_PICKER_SHORTCUT => {
                                auxiliary_windows::show_snippet_picker_window(&app_for_main)
                            }
                            SHELF_ESCAPE_SHORTCUT => {
                                if auxiliary_windows::is_shelf_picker_visible(&app_for_main) {
                                    auxiliary_windows::hide_shelf_picker_window(&app_for_main)
                                } else {
                                    Ok(())
                                }
                            }
                            _ => Ok(()),
                        };
                        if let Err(error) = result {
                            tracing::warn!(
                                shortcut = %dispatch_id,
                                error = %error,
                                "global shortcut action failed"
                            );
                            logging::debug_if_err(
                                app_for_main.emit(
                                    "toast",
                                    serde_json::json!({
                                        "message": format!("快捷键窗口打开失败: {error}")
                                    }),
                                ),
                                "emit shortcut failure toast",
                            );
                        }
                    }) {
                        tracing::warn!(
                            shortcut = %id,
                            error = %error,
                            "failed to dispatch global shortcut action to main thread"
                        );
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            match logging::init(app.handle()) {
                Ok(_) => tracing::info!(
                    version = env!("CARGO_PKG_VERSION"),
                    "runtime logging initialized"
                ),
                Err(error) => eprintln!("failed to initialize runtime logging: {error}"),
            }

            staged_update::forward_to_staged_version_if_needed(app.handle()).map_err(|error| {
                tracing::error!(error = %error, "failed to forward to staged version");
                Box::<dyn std::error::Error>::from(std::io::Error::other(error))
            })?;

            db::prepare_storage_dir(app.handle()).map_err(|error| {
                tracing::error!(error = %error, "failed to prepare storage directory");
                Box::<dyn std::error::Error>::from(std::io::Error::other(error))
            })?;
            let path = db_path(app.handle());
            let conn = init_db(&path).map_err(|error| {
                tracing::error!(error = %error, "failed to initialize database");
                Box::<dyn std::error::Error>::from(std::io::Error::other(error))
            })?;
            clipboard_images::migrate_legacy_clipboard_images(app.handle(), &conn);
            todo_images::migrate_legacy_todo_images(app.handle(), &conn);
            app_icons::migrate_legacy_app_icons(app.handle(), &conn);
            {
                let settings = db::load_settings(&conn);
                clipboard_db::purge_clipboard_history_by_retention(
                    &conn,
                    &settings.clipboard_history_retention,
                );
            }
            let state = AppState {
                db: Arc::new(Mutex::new(conn)),
                tracker: Arc::new(Mutex::new(TrackerState::default())),
                pomodoro: Arc::new(Mutex::new(PomodoroRuntime::default())),
                clipboard: Arc::new(Mutex::new(db::ClipboardRuntime::default())),
            };
            commands::start_tracker(app.handle().clone(), state.clone());
            clipboard_watcher::start_clipboard_watcher(app.handle().clone(), state.clone());
            app.manage(state.clone());
            commands::check_pending_recurrences(app.handle(), &state);

            tray_menu::setup_tray(app)?;
            register_global_shortcuts(app.handle());
            auxiliary_windows::precache_auxiliary_windows(app.handle())?;

            if let Some(state) = app.try_state::<AppState>() {
                let should_restore_float = {
                    let runtime = state.pomodoro.lock();
                    runtime.status != db::PomodoroStatus::Idle
                };
                if should_restore_float {
                    logging::warn_if_err(
                        auxiliary_windows::show_pomodoro_float_window(app.handle()),
                        "restore pomodoro float window",
                    );
                }
            }

            #[cfg(target_os = "macos")]
            {
                logging::debug_if_err(
                    app.handle()
                        .set_activation_policy(tauri::ActivationPolicy::Regular),
                    "set macos activation policy",
                );
            }

            logging::warn_if_err(
                staged_update::confirm_current_staged_launch(app.handle()),
                "confirm staged update launch",
            );

            if let Some(window) = app.get_webview_window("main") {
                logging::debug_if_err(window.set_maximizable(true), "set main window maximizable");
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        commands::hide_to_tray(&app_handle);
                        return;
                    }

                    #[cfg(target_os = "macos")]
                    {
                        if crate::macos_dock::is_main_window_in_tray() {
                            if let Some(main) = app_handle.get_webview_window("main") {
                                let visible = match main.is_visible() {
                                    Ok(visible) => visible,
                                    Err(error) => {
                                        tracing::debug!(
                                            error = %error,
                                            "failed to read main window visibility"
                                        );
                                        false
                                    }
                                };
                                if visible {
                                    logging::debug_if_err(
                                        main.hide(),
                                        "hide main window from dock state",
                                    );
                                }
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::reports::get_daily_report,
            commands::reports::get_weekly_report,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::set_storage_dir,
            commands::settings::reset_today,
            commands::settings::reset_all,
            commands::reports::get_known_apps,
            commands::todos::get_todos,
            commands::todos::get_todo,
            commands::todos::add_todo,
            commands::todos::update_todo_details,
            commands::todos::set_todo_completed,
            commands::todos::set_todo_pinned,
            commands::todos::add_todo_subtask,
            commands::todos::set_todo_subtask_completed,
            commands::todos::update_todo_subtask,
            commands::todos::delete_todo_subtask,
            commands::todos::delete_todo_image,
            commands::todos::add_todo_note,
            commands::todos::delete_todo_note,
            commands::todos::restore_todo_note,
            commands::todos::delete_todo,
            commands::todos::restore_todo,
            commands::todos::export_todos_backup,
            commands::todos::import_todos_backup,
            commands::markdown::save_markdown_image,
            commands::settings::complete_onboarding,
            commands::window::quit_app,
            commands::window::debug_log,
            commands::window::hide_to_tray_command,
            commands::window::show_window,
            staged_update::staged_update_status,
            staged_update::staged_check_update,
            staged_update::staged_download_update,
            staged_update::staged_restart_to_update,
            commands::pomodoro_cmds::get_pomodoro_state,
            commands::pomodoro_cmds::set_pomodoro_todo,
            commands::pomodoro_cmds::start_pomodoro,
            commands::pomodoro_cmds::get_todo_focus_summary,
            commands::pomodoro_cmds::get_todo_focus_summaries,
            commands::pomodoro_cmds::pause_pomodoro,
            commands::pomodoro_cmds::stop_pomodoro,
            commands::pomodoro_cmds::skip_pomodoro_phase,
            auxiliary_windows::show_eye_care_overlay,
            auxiliary_windows::hide_eye_care_overlay,
            auxiliary_windows::sync_eye_care_window_background,
            auxiliary_windows::show_pomodoro_float,
            auxiliary_windows::hide_pomodoro_float,
            auxiliary_windows::toggle_pomodoro_float,
            auxiliary_windows::is_pomodoro_float_visible_command,
            auxiliary_windows::set_pomodoro_float_expanded,
            auxiliary_windows::save_pomodoro_float_position,
            auxiliary_windows::popup_pomodoro_float_menu,
            commands::clipboard::get_clipboard_history,
            commands::clipboard::delete_clipboard_history_entry,
            commands::clipboard::clear_clipboard_history_command,
            commands::clipboard::pin_clipboard_history_entry,
            commands::clipboard::copy_text_to_clipboard,
            commands::clipboard::copy_clipboard_entry,
            commands::snippets::get_snippets,
            commands::snippets::get_snippet_groups,
            commands::snippets::create_snippet_group,
            commands::snippets::update_snippet_group_command,
            commands::snippets::delete_snippet_group_command,
            commands::snippets::create_snippet,
            commands::snippets::update_snippet_command,
            commands::snippets::duplicate_snippet_command,
            commands::snippets::pin_snippet_command,
            commands::snippets::delete_snippet_command,
            commands::snippets::copy_snippet_to_clipboard,
            auxiliary_windows::show_clipboard_picker,
            auxiliary_windows::show_snippet_picker,
            auxiliary_windows::hide_shelf_picker,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        tracing::error!(error = %error, "tauri application exited with error");
        panic!("error while running tauri application: {error}");
    }
}

fn register_global_shortcuts(app: &tauri::AppHandle) {
    for shortcut in [
        QUICK_TODO_SHORTCUT,
        CLIPBOARD_PICKER_SHORTCUT,
        SNIPPET_PICKER_SHORTCUT,
        SHELF_ESCAPE_SHORTCUT,
    ] {
        if let Err(error) = app.global_shortcut().register(shortcut) {
            tracing::warn!(
                shortcut = %shortcut,
                error = %error,
                "failed to register global shortcut"
            );
            logging::debug_if_err(
                app.emit(
                    "toast",
                    serde_json::json!({
                        "message": format!("{shortcut} shortcut unavailable: {error}")
                    }),
                ),
                "emit shortcut registration failure toast",
            );
        }
    }
}
