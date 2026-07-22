mod commands;
mod db;
mod platform;
mod plugins;
mod pomodoro;

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
mod mcp;
mod todo_images;
mod tray_menu;

#[cfg(test)]
mod tests;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use db::{
    db_path, init_db, AppState, PomodoroRuntime, TrackerState, DEFAULT_CLIPBOARD_PICKER_SHORTCUT,
    DEFAULT_COMMAND_PALETTE_SHORTCUT, DEFAULT_SNIPPET_PICKER_SHORTCUT,
};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const ACTION_COMMAND_PALETTE: &str = "command_palette";
const ACTION_CLIPBOARD_PICKER: &str = "clipboard_picker";
const ACTION_SNIPPET_PICKER: &str = "snippet_picker";
const ACTION_SHELF_ESCAPE: &str = "shelf_escape";
const SHELF_ESCAPE_SHORTCUT: &str = "Escape";

#[derive(Default)]
struct ShortcutActionMap {
    /// Normalized shortcut string -> action id
    by_shortcut: HashMap<String, &'static str>,
    /// Currently registered raw shortcut strings (for unregister)
    registered: Vec<String>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::install_panic_hook();

    // Single-instance must be registered first so a second launch exits early
    // and focuses the existing process instead of starting another runtime.
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            logging::warn_if_err(
                auxiliary_windows::show_command_palette(app),
                "focus existing window on second launch",
            );
        }));
    }

    let builder = builder
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
        .register_uri_scheme_protocol(app_icons::APP_ICON_PROTOCOL, |_ctx, request| {
            app_icons::AppIconService::global().protocol_response(request)
        })
        .register_uri_scheme_protocol(plugins::ui::PROTOCOL, |ctx, request| {
            plugins::ui::protocol_response(ctx.app_handle(), request)
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
                    // The plugin holds its shortcut registry lock while invoking this handler.
                    // Dispatch from another thread so dynamic Esc registration can only run after
                    // the handler returns and releases that lock.
                    logging::spawn_named("tempo-global-shortcut-dispatch", move || {
                        let app_for_main = app.clone();
                        if let Err(error) = app.run_on_main_thread(move || {
                            let action = app_for_main
                                .try_state::<Mutex<ShortcutActionMap>>()
                                .and_then(|map| {
                                    map.lock()
                                        .by_shortcut
                                        .get(&normalize_shortcut_key(&dispatch_id))
                                        .copied()
                                });
                            let result = match action {
                                Some(ACTION_COMMAND_PALETTE) => {
                                    auxiliary_windows::toggle_command_palette(&app_for_main)
                                }
                                Some(ACTION_CLIPBOARD_PICKER) => {
                                    auxiliary_windows::show_clipboard_picker_window(&app_for_main)
                                }
                                Some(ACTION_SNIPPET_PICKER) => {
                                    auxiliary_windows::show_snippet_picker_window(&app_for_main)
                                }
                                Some(ACTION_SHELF_ESCAPE) => {
                                    auxiliary_windows::hide_shelf_picker_window(&app_for_main)
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
                    });
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
            app_icons::remove_obsolete_disk_cache(app.handle());
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
            app.manage(Mutex::new(ShortcutActionMap::default()));
            commands::launcher::warm_launcher_index(app.handle().clone());
            let mcp_controller = mcp::McpController::new();
            app.manage(mcp_controller.clone());
            commands::check_pending_recurrences(app.handle(), &state);
            mcp_controller.start(app.handle());

            {
                let plugin_host = Arc::new(plugins::host::PluginHost::new(app.handle().clone()));
                app.manage(plugin_host.clone());
                let conn = state.db.lock();
                if let Err(error) = plugins::trust::ensure_plugin_tables(&conn) {
                    tracing::warn!(error = %error, "failed to prepare plugin tables");
                }
                if let Err(error) = plugins::trust::normalize_runtime_states_on_boot(&conn) {
                    tracing::warn!(error = %error, "failed to normalize plugin runtime states");
                }
                match plugins::loader::scan_enabled_contributions(app.handle(), &plugin_host, &conn) {
                    Ok(bundles) => {
                        tracing::info!(count = bundles.len(), "loaded plugin contributions on boot");
                    }
                    Err(error) => {
                        tracing::warn!(error = %error, "failed to scan plugin contributions on boot");
                    }
                }

                // Phase 1 §4.3/§15: only `onStartup` plugins get an eagerly-started Runtime;
                // every other plugin stays lazy until its first command/`runtime.*` call.
                match plugins::paths::packages_dir(app.handle()) {
                    Ok(packages_root) => match plugins::loader::plugins_needing_startup(&conn, &packages_root) {
                        Ok(plugin_ids) => {
                            for plugin_id in plugin_ids {
                                let host = plugin_host.clone();
                                tauri::async_runtime::spawn(async move {
                                    if let Err(error) = host.supervisor.ensure_started(&plugin_id).await {
                                        tracing::warn!(
                                            plugin_id = %plugin_id,
                                            error = %error,
                                            "onStartup plugin activation failed"
                                        );
                                    }
                                });
                            }
                        }
                        Err(error) => {
                            tracing::warn!(error = %error, "failed to scan onStartup plugins");
                        }
                    },
                    Err(error) => {
                        tracing::warn!(error = %error, "failed to resolve plugin packages dir");
                    }
                }
            }

            tray_menu::setup_tray(app)?;
            {
                let settings = {
                    let conn = state.db.lock();
                    db::load_settings(&conn)
                };
                if let Err(error) = apply_global_shortcuts(
                    app.handle(),
                    &settings.shortcut_command_palette,
                    &settings.shortcut_clipboard_picker,
                    &settings.shortcut_snippet_picker,
                ) {
                    tracing::warn!(
                        error = %error,
                        "failed to register saved shortcuts; falling back to defaults"
                    );
                    if let Err(fallback_error) = apply_global_shortcuts(
                        app.handle(),
                        DEFAULT_COMMAND_PALETTE_SHORTCUT,
                        DEFAULT_CLIPBOARD_PICKER_SHORTCUT,
                        DEFAULT_SNIPPET_PICKER_SHORTCUT,
                    ) {
                        tracing::warn!(
                            error = %fallback_error,
                            "failed to register default global shortcuts"
                        );
                        logging::debug_if_err(
                            app.emit(
                                "toast",
                                serde_json::json!({
                                    "message": format!("快捷键注册失败: {fallback_error}")
                                }),
                            ),
                            "emit shortcut registration failure toast",
                        );
                    }
                }
            }
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
                // Belt-and-suspenders after setup work; primary policy is set pre-run above.
                crate::macos_dock::ensure_accessory_policy(app.handle());
            }

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
            commands::settings::regenerate_mcp_token,
            commands::settings::set_storage_dir,
            commands::settings::reset_today,
            commands::settings::reset_all,
            commands::reports::get_known_apps,
            commands::launcher::get_launcher_apps,
            commands::launcher::refresh_launcher_apps,
            commands::launcher::launch_indexed_app,
            commands::launcher::set_launcher_app_pinned,
            commands::launcher::record_launcher_usage,
            commands::launcher::get_launcher_usage,
            auxiliary_windows::set_command_palette_height,
            auxiliary_windows::set_command_palette_size,
            auxiliary_windows::show_command_palette_window,
            auxiliary_windows::prepare_native_file_dialog,
            auxiliary_windows::restore_after_native_file_dialog,
            auxiliary_windows::sync_command_palette_appearance,
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
            commands::pomodoro_cmds::get_pomodoro_state,
            commands::pomodoro_cmds::set_pomodoro_todo,
            commands::pomodoro_cmds::start_pomodoro,
            commands::pomodoro_cmds::get_todo_focus_summary,
            commands::pomodoro_cmds::get_todo_focus_summaries,
            commands::pomodoro_cmds::pause_pomodoro,
            commands::pomodoro_cmds::stop_pomodoro,
            commands::pomodoro_cmds::skip_pomodoro_phase,
            commands::port_manager::get_port_records,
            commands::port_manager::terminate_port_process,
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
            commands::plugins::plugin_runtime_status,
            commands::plugins::plugin_runtime_install,
            commands::plugins::plugin_runtime_uninstall,
            commands::plugins::import_local_plugin,
            commands::plugins::list_plugins,
            commands::plugins::trust_plugin,
            commands::plugins::set_plugin_enabled_command,
            commands::plugins::list_plugin_contributions,
            commands::plugins::plugin_call_command,
            commands::plugins::plugin_bridge_invoke,
            commands::plugins::plugin_ui_prepare,
            commands::plugins::plugin_ui_dispose,
            commands::plugins::plugin_ui_serialize_session,
            commands::plugins::plugin_open_data_dir,
            commands::plugins::plugin_uninstall,
            commands::plugins::set_plugin_mcp_exposed,
            commands::plugins::promote_plugin_pending_version,
            commands::plugins::list_plugin_mcp_tools,
            commands::hosts::get_hosts_workspace,
            commands::hosts::authorize_hosts_write,
            commands::hosts::save_hosts_public,
            commands::hosts::save_hosts_profile,
            commands::hosts::delete_hosts_profile,
            commands::hosts::activate_hosts_profile,
            commands::hosts::get_hosts_profile_content,
            commands::hosts::apply_hosts,
            commands::hosts::flush_dns,
            commands::hosts::list_hosts_backups,
            commands::hosts::restore_hosts_backup,
            commands::translate::get_translate_config,
            commands::translate::update_translate_config,
            commands::translate::translate_text,
            commands::translate::translate_compare,
            commands::translate::test_translate_provider,
            auxiliary_windows::show_clipboard_picker,
            auxiliary_windows::show_snippet_picker,
            auxiliary_windows::hide_shelf_picker,
        ])
        .build(tauri::generate_context!())
        .map(|mut app| {
            // Set before run() so tao applies Accessory at applicationDidFinishLaunching —
            // setting it later in setup still briefly shows a Dock icon (Regular is the default).
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                app.set_dock_visibility(false);
            }

            app.run(|app_handle, event| {
                #[cfg(target_os = "macos")]
                if let tauri::RunEvent::Reopen {
                    has_visible_windows,
                    ..
                } = &event
                {
                    // App reopen (e.g. from Finder) with no visible windows: open quick panel.
                    if !*has_visible_windows {
                        logging::warn_if_err(
                            auxiliary_windows::show_command_palette(app_handle),
                            "show command palette on macos reopen",
                        );
                    }
                }
                let _ = (app_handle, event);
            });
        });

    if let Err(error) = result {
        tracing::error!(error = %error, "tauri application exited with error");
        panic!("error while running tauri application: {error}");
    }
}

pub(crate) fn normalize_shortcut_key(value: &str) -> String {
    match Shortcut::from_str(value.trim()) {
        Ok(shortcut) => shortcut.to_string(),
        Err(_) => value.trim().to_string(),
    }
}

fn validate_shortcut_binding(value: &str) -> Result<Option<String>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.eq_ignore_ascii_case(SHELF_ESCAPE_SHORTCUT) {
        return Err("Esc 已用于关闭货架，请选择其他快捷键".into());
    }
    let shortcut = Shortcut::from_str(trimmed).map_err(|error| format!("无效快捷键: {error}"))?;
    Ok(Some(shortcut.to_string()))
}

pub(crate) fn register_shelf_escape_shortcut(app: &tauri::AppHandle) -> Result<(), String> {
    let normalized = normalize_shortcut_key(SHELF_ESCAPE_SHORTCUT);
    let map_state = app
        .try_state::<Mutex<ShortcutActionMap>>()
        .ok_or_else(|| "shortcut state is not initialized".to_string())?;
    let mut map = map_state.lock();

    if map.by_shortcut.get(&normalized).copied() == Some(ACTION_SHELF_ESCAPE) {
        return Ok(());
    }
    if map.by_shortcut.contains_key(&normalized) {
        return Err("Escape is already assigned to another action".into());
    }

    app.global_shortcut()
        .register(SHELF_ESCAPE_SHORTCUT)
        .map_err(|error| format!("failed to register {SHELF_ESCAPE_SHORTCUT}: {error}"))?;
    map.registered.push(SHELF_ESCAPE_SHORTCUT.to_string());
    map.by_shortcut.insert(normalized, ACTION_SHELF_ESCAPE);
    Ok(())
}

pub(crate) fn unregister_shelf_escape_shortcut(app: &tauri::AppHandle) -> Result<(), String> {
    let normalized = normalize_shortcut_key(SHELF_ESCAPE_SHORTCUT);
    let map_state = app
        .try_state::<Mutex<ShortcutActionMap>>()
        .ok_or_else(|| "shortcut state is not initialized".to_string())?;
    let mut map = map_state.lock();

    if map.by_shortcut.get(&normalized).copied() != Some(ACTION_SHELF_ESCAPE) {
        return Ok(());
    }

    app.global_shortcut()
        .unregister(SHELF_ESCAPE_SHORTCUT)
        .map_err(|error| format!("failed to unregister {SHELF_ESCAPE_SHORTCUT}: {error}"))?;
    map.registered
        .retain(|raw| normalize_shortcut_key(raw) != normalized);
    map.by_shortcut.remove(&normalized);
    Ok(())
}

/// Apply the configurable shortcuts, preserving Esc only while the shelf is visible.
pub fn apply_global_shortcuts(
    app: &tauri::AppHandle,
    command_palette: &str,
    clipboard_picker: &str,
    snippet_picker: &str,
) -> Result<(), String> {
    let palette_raw = command_palette.trim();
    let clipboard_raw = clipboard_picker.trim();
    let snippet_raw = snippet_picker.trim();

    let palette_norm = validate_shortcut_binding(palette_raw)?;
    let clipboard_norm = validate_shortcut_binding(clipboard_raw)?;
    let snippet_norm = validate_shortcut_binding(snippet_raw)?;

    if shortcut_bindings_conflict(&palette_norm, &clipboard_norm)
        || shortcut_bindings_conflict(&palette_norm, &snippet_norm)
        || shortcut_bindings_conflict(&clipboard_norm, &snippet_norm)
    {
        return Err("快捷键不能重复".into());
    }

    let map_state = app
        .try_state::<Mutex<ShortcutActionMap>>()
        .ok_or_else(|| "快捷键状态未初始化".to_string())?;

    let (previous_registered, previous_map) = {
        let map = map_state.lock();
        (map.registered.clone(), map.by_shortcut.clone())
    };

    {
        let mut map = map_state.lock();
        for old in map.registered.drain(..) {
            if let Err(error) = app.global_shortcut().unregister(old.as_str()) {
                tracing::debug!(shortcut = %old, error = %error, "failed to unregister shortcut");
            }
        }
        map.by_shortcut.clear();
    }

    let mut bindings: Vec<(&str, String, &'static str)> = Vec::new();
    if let Some(normalized) = palette_norm {
        bindings.push((palette_raw, normalized, ACTION_COMMAND_PALETTE));
    }
    if let Some(normalized) = clipboard_norm {
        bindings.push((clipboard_raw, normalized, ACTION_CLIPBOARD_PICKER));
    }
    if let Some(normalized) = snippet_norm {
        bindings.push((snippet_raw, normalized, ACTION_SNIPPET_PICKER));
    }
    if auxiliary_windows::is_shelf_picker_visible(app) {
        bindings.push((
            SHELF_ESCAPE_SHORTCUT,
            normalize_shortcut_key(SHELF_ESCAPE_SHORTCUT),
            ACTION_SHELF_ESCAPE,
        ));
    }

    let mut registered: Vec<String> = Vec::new();
    let mut by_shortcut: HashMap<String, &'static str> = HashMap::new();

    for (raw, normalized, action) in &bindings {
        if let Err(error) = app.global_shortcut().register(*raw) {
            for done in &registered {
                let _ = app.global_shortcut().unregister(done.as_str());
            }
            for old in &previous_registered {
                let _ = app.global_shortcut().register(old.as_str());
            }
            let mut map = map_state.lock();
            map.registered = previous_registered;
            map.by_shortcut = previous_map;
            return Err(format!("注册快捷键 {raw} 失败: {error}"));
        }
        registered.push((*raw).to_string());
        by_shortcut.insert(normalized.clone(), *action);
    }

    let mut map = map_state.lock();
    map.registered = registered;
    map.by_shortcut = by_shortcut;
    Ok(())
}

/// Validate bindings before saving settings. Returns trimmed raw strings to persist.
pub fn validate_shortcut_bindings(
    command_palette: &str,
    clipboard_picker: &str,
    snippet_picker: &str,
) -> Result<(String, String, String), String> {
    let palette = validate_shortcut_binding(command_palette)?;
    let clipboard = validate_shortcut_binding(clipboard_picker)?;
    let snippet = validate_shortcut_binding(snippet_picker)?;
    if shortcut_bindings_conflict(&palette, &clipboard)
        || shortcut_bindings_conflict(&palette, &snippet)
        || shortcut_bindings_conflict(&clipboard, &snippet)
    {
        return Err("快捷键不能重复".into());
    }
    Ok((
        command_palette.trim().to_string(),
        clipboard_picker.trim().to_string(),
        snippet_picker.trim().to_string(),
    ))
}

fn shortcut_bindings_conflict(left: &Option<String>, right: &Option<String>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left == right)
}
