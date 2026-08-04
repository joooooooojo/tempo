mod builtin_plugins;
mod commands;
mod db;
mod notify;
mod platform;
mod plugins;

mod app_icons;
mod asset_protocol;
mod auxiliary_windows;
mod launcher_context_menu;
mod logging;
mod launcher_search;
#[cfg(target_os = "macos")]
mod macos_dock;
#[cfg(target_os = "macos")]
mod macos_overlay_panel;
mod mcp;
mod tray_menu;
#[cfg(windows)]
mod shortcut_hook;

#[cfg(test)]
mod tests;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use db::{
    db_path, init_db, AppState, DEFAULT_CLIPBOARD_PICKER_SHORTCUT, DEFAULT_MAIN_PANEL_SHORTCUT,
    DEFAULT_SNIPPET_PICKER_SHORTCUT,
};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const ACTION_MAIN_PANEL: &str = "main_panel";
const ACTION_CLIPBOARD_PICKER: &str = "clipboard_picker";
const ACTION_SNIPPET_PICKER: &str = "snippet_picker";
const ACTION_SHELF_ESCAPE: &str = "shelf_escape";
const SHELF_ESCAPE_SHORTCUT: &str = "Escape";

const SHORTCUT_ID_MAIN_PANEL: &str = "shortcut_main_panel";
const SHORTCUT_ID_CLIPBOARD_PICKER: &str = "shortcut_clipboard_picker";
const SHORTCUT_ID_SNIPPET_PICKER: &str = "shortcut_snippet_picker";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutOccupationState {
    Ok,
    Empty,
    Conflict,
    Occupied,
    Failed,
    Invalid,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShortcutBindingStatus {
    pub id: String,
    pub shortcut: String,
    pub state: ShortcutOccupationState,
    pub message: Option<String>,
}

#[derive(Default)]
struct ShortcutActionMap {
    /// Normalized shortcut string -> action id
    by_shortcut: HashMap<String, &'static str>,
    /// Currently registered raw shortcut strings (for unregister)
    registered: Vec<String>,
}

#[derive(Default)]
struct ShortcutStatusCache {
    statuses: Vec<ShortcutBindingStatus>,
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
                auxiliary_windows::show_main_panel(app),
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
            builtin_plugins::clipboard::CLIPBOARD_IMAGE_PROTOCOL,
            |ctx, request| {
                builtin_plugins::clipboard::clipboard_image_protocol_response(ctx.app_handle(), request)
            },
        )
        .register_uri_scheme_protocol(builtin_plugins::todo::TODO_IMAGE_PROTOCOL, |ctx, request| {
            builtin_plugins::todo::todo_image_protocol_response(ctx.app_handle(), request)
        })
        .register_uri_scheme_protocol(
            builtin_plugins::file_search::FILE_PREVIEW_PROTOCOL,
            |_ctx, request| builtin_plugins::file_search::file_preview_protocol_response(request),
        )
        .register_asynchronous_uri_scheme_protocol(app_icons::APP_ICON_PROTOCOL, |_ctx, request, responder| {
            // Extraction (especially macOS `sips`) must not run on the sync protocol
            // path — that stalls WKWebView while search tiles first paint.
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(app_icons::AppIconService::global().protocol_response(request));
            });
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
                            let Some(action) = action else {
                                return;
                            };
                            #[cfg(windows)]
                            if !shortcut_hook::claim_dispatch(action) {
                                return;
                            }
                            if let Err(error) =
                                dispatch_shortcut_action(&app_for_main, action)
                            {
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
            if let Ok(storage_dir) = db::current_storage_dir(app.handle())
                .or_else(|_| db::default_storage_dir(app.handle()))
            {
                app_icons::AppIconService::global()
                    .configure_disk_cache(storage_dir.join("icon-cache"));
            }
            let path = db_path(app.handle());
            let conn = init_db(&path).map_err(|error| {
                tracing::error!(error = %error, "failed to initialize database");
                Box::<dyn std::error::Error>::from(std::io::Error::other(error))
            })?;
            {
                let settings = db::load_settings(&conn);
                builtin_plugins::clipboard::db::purge_clipboard_history_by_retention(
                    &conn,
                    &settings.clipboard_history_retention,
                );
            }
            let state = AppState {
                db: Arc::new(Mutex::new(conn)),
                clipboard: Arc::new(Mutex::new(db::ClipboardRuntime::default())),
            };
            commands::launcher::restore_launcher_index_snapshot(&state);
            commands::start_tracker(app.handle().clone(), state.clone());
            builtin_plugins::clipboard::watcher::start_clipboard_watcher(app.handle().clone(), state.clone());
            builtin_plugins::hosts::start_remote_refresh_scheduler(app.handle().clone());
            app.manage(state.clone());
            app.manage(Mutex::new(ShortcutActionMap::default()));
            app.manage(Mutex::new(ShortcutStatusCache::default()));
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
                // every other plugin stays lazy until its first Command or private IPC call.
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
            platform::start_system_appearance_watcher(app.handle().clone());
            #[cfg(windows)]
            shortcut_hook::start(app.handle().clone());
            {
                let settings = {
                    let conn = state.db.lock();
                    db::load_settings(&conn)
                };
                match apply_global_shortcuts(
                    app.handle(),
                    &settings.shortcut_main_panel,
                    &settings.shortcut_clipboard_picker,
                    &settings.shortcut_snippet_picker,
                ) {
                    Ok(statuses) => {
                        let troubled = statuses.iter().any(|status| {
                            matches!(
                                status.state,
                                ShortcutOccupationState::Occupied
                                    | ShortcutOccupationState::Failed
                                    | ShortcutOccupationState::Conflict
                            )
                        });
                        if troubled {
                            tracing::warn!(
                                ?statuses,
                                "some saved shortcuts could not be registered"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "failed to register saved shortcuts; falling back to defaults"
                        );
                        if let Err(fallback_error) = apply_global_shortcuts(
                            app.handle(),
                            DEFAULT_MAIN_PANEL_SHORTCUT,
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
            }
            auxiliary_windows::precache_auxiliary_windows(app.handle())?;

            #[cfg(target_os = "macos")]
            {
                // Belt-and-suspenders after setup work; primary policy is set pre-run above.
                crate::macos_dock::ensure_accessory_policy(app.handle());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            builtin_plugins::reports::get_daily_report,
            builtin_plugins::reports::get_weekly_report,
            builtin_plugins::settings::get_settings,
            builtin_plugins::settings::update_settings,
            builtin_plugins::settings::get_shortcut_statuses,
            builtin_plugins::settings::regenerate_mcp_token,
            builtin_plugins::settings::set_storage_dir,
            builtin_plugins::settings::reset_today,
            builtin_plugins::settings::reset_all,
            builtin_plugins::reports::get_known_apps,
            commands::launcher::get_launcher_apps,
            commands::launcher::refresh_launcher_apps,
            commands::launcher::list_custom_launcher_entries,
            commands::launcher::add_custom_launcher_entries,
            commands::launcher::remove_custom_launcher_entry,
            commands::launcher::rename_custom_launcher_entry,
            commands::launcher::sync_main_panel_search_contributions,
            commands::launcher::search_main_panel_apps,
            commands::launcher::launch_indexed_app,
            commands::launcher::reveal_indexed_app,
            commands::url_browsers::list_installed_url_browsers,
            commands::url_browsers::get_default_url_browser,
            commands::url_browsers::open_url_in_browser,
            commands::launcher::set_launcher_app_pinned,
            commands::launcher::remove_launcher_from_recent,
            commands::launcher::record_launcher_usage,
            commands::launcher::get_launcher_usage,
            auxiliary_windows::set_main_panel_height,
            auxiliary_windows::set_main_panel_size,
            auxiliary_windows::set_main_panel_rect,
            auxiliary_windows::get_main_panel_position,
            auxiliary_windows::set_main_panel_position,
            auxiliary_windows::save_main_panel_position,
            auxiliary_windows::show_main_panel_window,
            auxiliary_windows::prepare_native_file_dialog,
            auxiliary_windows::restore_after_native_file_dialog,
            auxiliary_windows::sync_main_panel_appearance,
            auxiliary_windows::sync_shelf_picker_appearance,
            builtin_plugins::todo::get_todos,
            builtin_plugins::todo::get_todo,
            builtin_plugins::todo::add_todo,
            builtin_plugins::todo::update_todo_details,
            builtin_plugins::todo::set_todo_completed,
            builtin_plugins::todo::set_todo_pinned,
            builtin_plugins::todo::add_todo_subtask,
            builtin_plugins::todo::set_todo_subtask_completed,
            builtin_plugins::todo::update_todo_subtask,
            builtin_plugins::todo::delete_todo_subtask,
            builtin_plugins::todo::delete_todo_image,
            builtin_plugins::todo::add_todo_note,
            builtin_plugins::todo::delete_todo_note,
            builtin_plugins::todo::restore_todo_note,
            builtin_plugins::todo::delete_todo,
            builtin_plugins::todo::restore_todo,
            builtin_plugins::todo::export_todos_backup,
            builtin_plugins::todo::import_todos_backup,
            commands::markdown::save_markdown_image,
            commands::window::quit_app,
            commands::window::debug_log,
            commands::window::system_prefers_dark,
            commands::window::open_main_panel_devtools,
            commands::window::is_main_panel_devtools_open,
            notify::show_user_notification,
            builtin_plugins::port_manager::get_port_records,
            builtin_plugins::port_manager::terminate_port_process,
            builtin_plugins::file_search::file_search_status,
            builtin_plugins::file_search::file_search_ensure_engine,
            builtin_plugins::file_search::file_search_query,
            builtin_plugins::file_search::file_search_open,
            builtin_plugins::file_search::file_search_reveal,
            builtin_plugins::file_search::file_search_preview_meta,
            builtin_plugins::file_search::file_search_preview_url,
            builtin_plugins::file_search::file_search_list_archive,
            builtin_plugins::clipboard::get_clipboard_history,
            builtin_plugins::clipboard::delete_clipboard_history_entry,
            builtin_plugins::clipboard::clear_clipboard_history_command,
            builtin_plugins::clipboard::pin_clipboard_history_entry,
            builtin_plugins::clipboard::copy_text_to_clipboard,
            builtin_plugins::clipboard::copy_clipboard_entry,
            builtin_plugins::clipboard::get_main_panel_clipboard_seed,
            builtin_plugins::clipboard::seed_main_panel_from_system_clipboard,
            builtin_plugins::clipboard::clear_main_panel_clipboard_seed,
            builtin_plugins::snippets::get_snippets,
            builtin_plugins::snippets::get_snippet_groups,
            builtin_plugins::snippets::create_snippet_group,
            builtin_plugins::snippets::update_snippet_group_command,
            builtin_plugins::snippets::delete_snippet_group_command,
            builtin_plugins::snippets::create_snippet,
            builtin_plugins::snippets::update_snippet_command,
            builtin_plugins::snippets::duplicate_snippet_command,
            builtin_plugins::snippets::pin_snippet_command,
            builtin_plugins::snippets::delete_snippet_command,
            builtin_plugins::snippets::copy_snippet_to_clipboard,
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
            plugins::windows::open_plugin_window,
            plugins::windows::plugin_window_context,
            commands::plugins::plugin_open_data_dir,
            commands::plugins::reveal_plugin_install_dir,
            commands::plugins::plugin_uninstall,
            commands::plugins::set_plugin_mcp_exposed,
            commands::plugins::set_plugin_mcp_tool_enabled,
            commands::plugins::promote_plugin_pending_version,
            commands::plugins::list_plugin_mcp_tools,
            commands::plugins::get_plugin_settings_bundle,
            commands::plugins::set_plugin_settings_values,
            builtin_plugins::plugin_dev::plugin_dev_list_projects,
            builtin_plugins::plugin_dev::plugin_dev_create_project,
            builtin_plugins::plugin_dev::plugin_dev_open_project,
            builtin_plugins::plugin_dev::plugin_dev_get_project,
            builtin_plugins::plugin_dev::plugin_dev_write_manifest,
            builtin_plugins::plugin_dev::plugin_dev_update_preferences,
            builtin_plugins::plugin_dev::plugin_dev_probe_ui_url,
            builtin_plugins::plugin_dev::plugin_dev_connect,
            builtin_plugins::plugin_dev::plugin_dev_reload_ui,
            builtin_plugins::plugin_dev::plugin_dev_disconnect,
            builtin_plugins::plugin_dev::plugin_dev_reconnect_runtime,
            builtin_plugins::plugin_dev::plugin_dev_run_mcp_tool,
            builtin_plugins::plugin_dev::plugin_dev_forget_project,
            builtin_plugins::settings::list_builtin_mcp_tools,
            builtin_plugins::settings::get_builtin_mcp_status,
            builtin_plugins::settings::set_builtin_mcp_exposed,
            builtin_plugins::settings::set_builtin_mcp_tool_enabled,
            builtin_plugins::settings::builtin_open_data_dir,
            builtin_plugins::hosts::get_hosts_workspace,
            builtin_plugins::hosts::authorize_hosts_write,
            builtin_plugins::hosts::save_hosts_profile,
            builtin_plugins::hosts::delete_hosts_profile,
            builtin_plugins::hosts::set_hosts_profile_active,
            builtin_plugins::hosts::get_hosts_profile_content,
            builtin_plugins::hosts::open_hosts_file_location,
            builtin_plugins::hosts::refresh_hosts_remote_profile,
            builtin_plugins::hosts::apply_hosts,
            builtin_plugins::hosts::flush_dns,
            builtin_plugins::hosts::list_hosts_backups,
            builtin_plugins::hosts::restore_hosts_backup,
            builtin_plugins::translate::get_translate_config,
            builtin_plugins::translate::update_translate_config,
            builtin_plugins::translate::translate_text,
            builtin_plugins::translate::translate_text_stream,
            builtin_plugins::translate::translate_compare,
            builtin_plugins::translate::test_translate_provider,
            auxiliary_windows::show_clipboard_picker,
            auxiliary_windows::show_snippet_picker,
            auxiliary_windows::hide_shelf_picker,
            launcher_context_menu::show_launcher_context_menu,
            launcher_context_menu::hide_launcher_context_menu,
            launcher_context_menu::launcher_context_menu_action,
        ])
        .build(tauri::generate_context!())
        .map(|app| {
            #[cfg(target_os = "macos")]
            let mut app = app;

            // Set before run() so tao applies Accessory at applicationDidFinishLaunching —
            // setting it later in setup still briefly shows a Dock icon (Regular is the default).
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                app.set_dock_visibility(false);
            }

            app.run(|app_handle, event| {
                if let tauri::RunEvent::Ready = &event {
                    // Ask for notification permission before the main panel is key,
                    // so the system sheet cannot trigger blur→hide on the overlay.
                    #[cfg(target_os = "macos")]
                    notify::prime_macos_authorization(app_handle);

                    logging::warn_if_err(
                        auxiliary_windows::show_main_panel(app_handle),
                        "show main panel on startup",
                    );
                }

                #[cfg(target_os = "macos")]
                if let tauri::RunEvent::Reopen {
                    has_visible_windows,
                    ..
                } = &event
                {
                    // App reopen (e.g. from Finder) with no visible windows: open quick panel.
                    if !*has_visible_windows {
                        logging::warn_if_err(
                            auxiliary_windows::show_main_panel(app_handle),
                            "show main panel on macos reopen",
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

pub(crate) fn dispatch_shortcut_action(
    app: &tauri::AppHandle,
    action: &str,
) -> Result<(), String> {
    let result = match action {
        ACTION_MAIN_PANEL => auxiliary_windows::toggle_main_panel(app),
        ACTION_CLIPBOARD_PICKER => auxiliary_windows::show_clipboard_picker_window(app),
        ACTION_SNIPPET_PICKER => auxiliary_windows::show_snippet_picker_window(app),
        ACTION_SHELF_ESCAPE => auxiliary_windows::hide_shelf_picker_window(app),
        _ => return Ok(()),
    };
    result.map_err(|error| error.to_string())
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
    remember_registered_shortcut(
        &mut map,
        SHELF_ESCAPE_SHORTCUT.to_string(),
        normalized,
        ACTION_SHELF_ESCAPE,
    );
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
    forget_registered_shortcut(&mut map, SHELF_ESCAPE_SHORTCUT);
    Ok(())
}

fn is_configurable_shortcut_action(action: &str) -> bool {
    matches!(
        action,
        ACTION_MAIN_PANEL | ACTION_CLIPBOARD_PICKER | ACTION_SNIPPET_PICKER
    )
}

/// Decide which currently-registered configurable chords should be released.
///
/// Unchanged action→chord pairs are kept so other apps cannot snatch them during
/// status checks or unrelated setting updates.
fn obsolete_configurable_raws(
    registered: &[String],
    by_shortcut: &HashMap<String, &'static str>,
    desired_normalized_by_action: &HashMap<&'static str, String>,
) -> Vec<String> {
    let mut obsolete = Vec::new();
    for raw in registered {
        let normalized = normalize_shortcut_key(raw);
        let Some(action) = by_shortcut.get(&normalized).copied() else {
            continue;
        };
        if !is_configurable_shortcut_action(action) {
            continue;
        }
        let keep = desired_normalized_by_action
            .get(action)
            .is_some_and(|desired| desired == &normalized);
        if !keep {
            obsolete.push(raw.clone());
        }
    }
    obsolete
}

fn remember_registered_shortcut(
    map: &mut ShortcutActionMap,
    raw: String,
    normalized: String,
    action: &'static str,
) {
    if !map
        .registered
        .iter()
        .any(|existing| normalize_shortcut_key(existing) == normalized)
    {
        map.registered.push(raw);
    }
    map.by_shortcut.insert(normalized, action);
}

fn forget_registered_shortcut(map: &mut ShortcutActionMap, raw: &str) {
    let normalized = normalize_shortcut_key(raw);
    map.registered
        .retain(|existing| normalize_shortcut_key(existing) != normalized);
    map.by_shortcut.remove(&normalized);
}

/// Apply the configurable shortcuts, preserving Esc only while the shelf is visible.
///
/// Uses differential updates: chords that are already registered for the same action
/// are left held (no unregister→register gap). Only changed / cleared / conflicted
/// bindings are released. Internal conflicts and OS registration failures do not
/// roll back other successful registrations; settings may still be saved.
pub fn apply_global_shortcuts(
    app: &tauri::AppHandle,
    main_panel: &str,
    clipboard_picker: &str,
    snippet_picker: &str,
) -> Result<Vec<ShortcutBindingStatus>, String> {
    let prepared = prepare_shortcut_bindings(main_panel, clipboard_picker, snippet_picker)?;

    let map_state = app
        .try_state::<Mutex<ShortcutActionMap>>()
        .ok_or_else(|| "快捷键状态未初始化".to_string())?;
    let mut map = map_state.lock();

    let mut desired_normalized_by_action: HashMap<&'static str, String> = HashMap::new();
    for binding in &prepared {
        if let PreparedBindingOutcome::Ready(normalized) = &binding.outcome {
            desired_normalized_by_action.insert(binding.action, normalized.clone());
        }
    }

    let obsolete =
        obsolete_configurable_raws(&map.registered, &map.by_shortcut, &desired_normalized_by_action);
    for raw in obsolete {
        if let Err(error) = app.global_shortcut().unregister(raw.as_str()) {
            tracing::debug!(shortcut = %raw, error = %error, "failed to unregister shortcut");
        }
        forget_registered_shortcut(&mut map, &raw);
    }

    let mut statuses: Vec<ShortcutBindingStatus> = Vec::with_capacity(3);

    for binding in &prepared {
        let mut status = ShortcutBindingStatus {
            id: binding.id.to_string(),
            shortcut: binding.raw.to_string(),
            state: ShortcutOccupationState::Empty,
            message: None,
        };

        match &binding.outcome {
            PreparedBindingOutcome::Empty => {
                status.state = ShortcutOccupationState::Empty;
            }
            PreparedBindingOutcome::Invalid(message) => {
                status.state = ShortcutOccupationState::Invalid;
                status.message = Some(message.clone());
            }
            PreparedBindingOutcome::Conflict => {
                status.state = ShortcutOccupationState::Conflict;
                status.message = Some("与其他快捷键冲突".into());
            }
            PreparedBindingOutcome::Ready(normalized) => {
                let already_ours = map.by_shortcut.get(normalized).copied() == Some(binding.action)
                    && app.global_shortcut().is_registered(binding.raw.as_str());

                if already_ours {
                    status.state = ShortcutOccupationState::Ok;
                } else if app.global_shortcut().is_registered(binding.raw.as_str()) {
                    // Plugin still holds it (e.g. map desync) — reclaim bookkeeping only.
                    remember_registered_shortcut(
                        &mut map,
                        binding.raw.clone(),
                        normalized.clone(),
                        binding.action,
                    );
                    status.state = ShortcutOccupationState::Ok;
                } else {
                    match app.global_shortcut().register(binding.raw.as_str()) {
                        Ok(()) => {
                            remember_registered_shortcut(
                                &mut map,
                                binding.raw.clone(),
                                normalized.clone(),
                                binding.action,
                            );
                            status.state = ShortcutOccupationState::Ok;
                        }
                        Err(error) => {
                            let error_text = error.to_string();
                            let occupied = is_shortcut_occupation_error(&error_text);
                            status.state = if occupied {
                                ShortcutOccupationState::Occupied
                            } else {
                                ShortcutOccupationState::Failed
                            };
                            status.message = Some(if occupied {
                                "已被占用".into()
                            } else {
                                "注册失败".into()
                            });
                            tracing::warn!(
                                shortcut = %binding.raw,
                                error = %error_text,
                                occupied,
                                "failed to register global shortcut"
                            );
                        }
                    }
                }
            }
        }

        statuses.push(status);
    }

    let escape_normalized = normalize_shortcut_key(SHELF_ESCAPE_SHORTCUT);
    if auxiliary_windows::is_shelf_picker_visible(app) {
        if map.by_shortcut.get(&escape_normalized).copied() != Some(ACTION_SHELF_ESCAPE)
            && !map.by_shortcut.contains_key(&escape_normalized)
        {
            match app.global_shortcut().register(SHELF_ESCAPE_SHORTCUT) {
                Ok(()) => {
                    remember_registered_shortcut(
                        &mut map,
                        SHELF_ESCAPE_SHORTCUT.to_string(),
                        escape_normalized.clone(),
                        ACTION_SHELF_ESCAPE,
                    );
                }
                Err(error) => {
                    tracing::debug!(
                        error = %error,
                        "failed to register shelf Escape shortcut"
                    );
                }
            }
        }
    } else if map.by_shortcut.get(&escape_normalized).copied() == Some(ACTION_SHELF_ESCAPE) {
        if let Err(error) = app.global_shortcut().unregister(SHELF_ESCAPE_SHORTCUT) {
            tracing::debug!(error = %error, "failed to unregister shelf Escape shortcut");
        }
        forget_registered_shortcut(&mut map, SHELF_ESCAPE_SHORTCUT);
    }

    #[cfg(windows)]
    {
        // Mirror Ready bindings into the LL hook even when RegisterHotKey failed —
        // uTools-style hooks ignore RegisterHotKey ownership and must be beaten in-chain.
        let mut hook_bindings: Vec<(String, &'static str)> = prepared
            .iter()
            .filter_map(|binding| match &binding.outcome {
                PreparedBindingOutcome::Ready(_) => {
                    Some((binding.raw.clone(), binding.action))
                }
                _ => None,
            })
            .collect();
        if auxiliary_windows::is_shelf_picker_visible(app)
            || map.by_shortcut.get(&escape_normalized).copied() == Some(ACTION_SHELF_ESCAPE)
        {
            hook_bindings.push((SHELF_ESCAPE_SHORTCUT.to_string(), ACTION_SHELF_ESCAPE));
        }
        shortcut_hook::sync_bindings(&hook_bindings);

        for status in &mut statuses {
            if status.state == ShortcutOccupationState::Occupied {
                status.state = ShortcutOccupationState::Ok;
                status.message = None;
            }
        }
    }

    if let Some(cache) = app.try_state::<Mutex<ShortcutStatusCache>>() {
        cache.lock().statuses = statuses.clone();
    }

    Ok(statuses)
}

fn is_shortcut_occupation_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("already registered")
        || lower.contains("alreadyregistered")
        || lower.contains("already been registered")
        || lower.contains("hotkey already")
}

#[derive(Debug)]
struct PreparedShortcutBinding {
    id: &'static str,
    raw: String,
    action: &'static str,
    outcome: PreparedBindingOutcome,
}

#[derive(Debug)]
enum PreparedBindingOutcome {
    Empty,
    Invalid(String),
    Conflict,
    Ready(String),
}

fn prepare_shortcut_bindings(
    main_panel: &str,
    clipboard_picker: &str,
    snippet_picker: &str,
) -> Result<Vec<PreparedShortcutBinding>, String> {
    let specs = [
        (SHORTCUT_ID_MAIN_PANEL, main_panel, ACTION_MAIN_PANEL),
        (
            SHORTCUT_ID_CLIPBOARD_PICKER,
            clipboard_picker,
            ACTION_CLIPBOARD_PICKER,
        ),
        (
            SHORTCUT_ID_SNIPPET_PICKER,
            snippet_picker,
            ACTION_SNIPPET_PICKER,
        ),
    ];

    let mut prepared: Vec<PreparedShortcutBinding> = specs
        .into_iter()
        .map(|(id, raw, action)| {
            let trimmed = raw.trim().to_string();
            let outcome = match validate_shortcut_binding(&trimmed) {
                Ok(None) => PreparedBindingOutcome::Empty,
                Ok(Some(normalized)) => PreparedBindingOutcome::Ready(normalized),
                Err(message) => PreparedBindingOutcome::Invalid(message),
            };
            PreparedShortcutBinding {
                id,
                raw: trimmed,
                action,
                outcome,
            }
        })
        .collect();

    let mut normalized_counts: HashMap<String, usize> = HashMap::new();
    for binding in &prepared {
        if let PreparedBindingOutcome::Ready(normalized) = &binding.outcome {
            *normalized_counts.entry(normalized.clone()).or_insert(0) += 1;
        }
    }

    for binding in &mut prepared {
        if let PreparedBindingOutcome::Ready(normalized) = &binding.outcome {
            if normalized_counts.get(normalized).copied().unwrap_or(0) > 1 {
                binding.outcome = PreparedBindingOutcome::Conflict;
            }
        }
    }

    Ok(prepared)
}

/// Validate bindings before saving settings. Returns trimmed raw strings to persist.
/// Duplicate chords are allowed so the UI can surface conflict status.
pub fn validate_shortcut_bindings(
    main_panel: &str,
    clipboard_picker: &str,
    snippet_picker: &str,
) -> Result<(String, String, String), String> {
    validate_shortcut_binding(main_panel)?;
    validate_shortcut_binding(clipboard_picker)?;
    validate_shortcut_binding(snippet_picker)?;
    Ok((
        main_panel.trim().to_string(),
        clipboard_picker.trim().to_string(),
        snippet_picker.trim().to_string(),
    ))
}

#[cfg(test)]
mod shortcut_status_tests {
    use super::{
        is_shortcut_occupation_error, normalize_shortcut_key, obsolete_configurable_raws,
        prepare_shortcut_bindings, PreparedBindingOutcome, ACTION_CLIPBOARD_PICKER,
        ACTION_MAIN_PANEL, ACTION_SHELF_ESCAPE, ACTION_SNIPPET_PICKER,
    };
    use std::collections::HashMap;

    #[test]
    fn occupation_error_detection_matches_global_hotkey_messages() {
        assert!(is_shortcut_occupation_error(
            "HotKey already registered: HotKey { mods: Modifiers(0x0), key: KeyA }"
        ));
        assert!(is_shortcut_occupation_error(
            "hotkey already registered by another application"
        ));
        assert!(!is_shortcut_occupation_error("Unable to register hotkey: boom"));
    }

    #[test]
    fn duplicate_bindings_are_marked_conflict() {
        let prepared = prepare_shortcut_bindings(
            "Control+Shift+V",
            "Control+Shift+V",
            "Control+Shift+S",
        )
        .expect("prepare");
        assert!(matches!(
            prepared[0].outcome,
            PreparedBindingOutcome::Conflict
        ));
        assert!(matches!(
            prepared[1].outcome,
            PreparedBindingOutcome::Conflict
        ));
        assert!(matches!(
            prepared[2].outcome,
            PreparedBindingOutcome::Ready(_)
        ));
    }

    #[test]
    fn obsolete_raws_keep_unchanged_chords_and_escape() {
        let main = normalize_shortcut_key("Alt+Space");
        let clipboard = normalize_shortcut_key("Control+Shift+V");
        let escape = normalize_shortcut_key("Escape");

        let registered = vec![
            "Alt+Space".into(),
            "Control+Shift+V".into(),
            "Escape".into(),
        ];
        let mut by_shortcut = HashMap::new();
        by_shortcut.insert(main.clone(), ACTION_MAIN_PANEL);
        by_shortcut.insert(clipboard.clone(), ACTION_CLIPBOARD_PICKER);
        by_shortcut.insert(escape, ACTION_SHELF_ESCAPE);

        let mut desired = HashMap::new();
        desired.insert(ACTION_MAIN_PANEL, main);
        desired.insert(ACTION_CLIPBOARD_PICKER, clipboard);
        desired.insert(
            ACTION_SNIPPET_PICKER,
            normalize_shortcut_key("Control+Shift+S"),
        );

        let obsolete = obsolete_configurable_raws(&registered, &by_shortcut, &desired);
        assert!(obsolete.is_empty(), "unchanged chords must stay held: {obsolete:?}");
    }

    #[test]
    fn obsolete_raws_release_only_changed_or_cleared_actions() {
        let main = normalize_shortcut_key("Alt+Space");
        let clipboard = normalize_shortcut_key("Control+Shift+V");
        let snippet = normalize_shortcut_key("Control+Shift+S");

        let registered = vec![
            "Alt+Space".into(),
            "Control+Shift+V".into(),
            "Control+Shift+S".into(),
        ];
        let mut by_shortcut = HashMap::new();
        by_shortcut.insert(main.clone(), ACTION_MAIN_PANEL);
        by_shortcut.insert(clipboard.clone(), ACTION_CLIPBOARD_PICKER);
        by_shortcut.insert(snippet, ACTION_SNIPPET_PICKER);

        let mut desired = HashMap::new();
        // main panel chord changed
        desired.insert(ACTION_MAIN_PANEL, normalize_shortcut_key("Control+Alt+Space"));
        // clipboard unchanged
        desired.insert(ACTION_CLIPBOARD_PICKER, clipboard);
        // snippet cleared (absent from desired)

        let obsolete = obsolete_configurable_raws(&registered, &by_shortcut, &desired);
        assert_eq!(obsolete.len(), 2);
        assert!(obsolete.iter().any(|raw| raw == "Alt+Space"));
        assert!(obsolete.iter().any(|raw| raw == "Control+Shift+S"));
        assert!(!obsolete.iter().any(|raw| raw == "Control+Shift+V"));
    }
}
