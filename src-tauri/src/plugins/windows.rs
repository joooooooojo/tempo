//! Plugin app rectangle resolution and normal top-level standalone windows.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{
    AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::db::AppState;

use super::host::{PluginHost, PluginWindowInstance};
use super::ids::is_valid_local_id;
use super::manifest::{
    AppRect, PluginWindowMode, RectValue, APP_WINDOW_MAX_HEIGHT, APP_WINDOW_MAX_WIDTH,
    APP_WINDOW_MIN_HEIGHT, APP_WINDOW_MIN_WIDTH,
};
use super::package::verify_package_hash;
use super::paths::packages_dir;
use super::trust::{ensure_plugin_tables, get_installed_plugin};
use super::ui;

const WINDOW_LABEL_PREFIX: &str = "plugin-window-";
pub const DEFAULT_APP_WINDOW_HEIGHT: f64 = 580.0;

#[derive(Debug, Clone, Copy)]
pub struct ResolvedWindowRect {
    pub width: f64,
    pub height: f64,
    pub physical_size: PhysicalSize<u32>,
    pub position: PhysicalPosition<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPluginWindowArgs {
    pub plugin_id: String,
    pub app_id: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWindowContext {
    pub plugin_id: String,
    pub app_id: String,
    pub params: Value,
}

impl From<PluginWindowInstance> for PluginWindowContext {
    fn from(value: PluginWindowInstance) -> Self {
        Self {
            plugin_id: value.plugin_id,
            app_id: value.app_local_id,
            params: value.params,
        }
    }
}

pub fn plugin_window_label(plugin_id: &str, app_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plugin_id.as_bytes());
    hasher.update(b"/");
    hasher.update(app_id.as_bytes());
    format!("{WINDOW_LABEL_PREFIX}{}", hex::encode(hasher.finalize()))
}

/// Create one normal OS window per contributed app, or focus its existing window.
/// This must stay async: synchronous Tauri commands run on the IPC/main thread, where
/// creating a Windows WebView2 window can deadlock the caller and the application event loop.
#[tauri::command]
pub async fn open_plugin_window(
    app: AppHandle,
    state: State<'_, AppState>,
    host: State<'_, Arc<PluginHost>>,
    args: OpenPluginWindowArgs,
) -> Result<(), String> {
    tracing::debug!(
        plugin_id = %args.plugin_id,
        app_id = %args.app_id,
        "standalone plugin window open requested"
    );
    if !is_valid_local_id(&args.app_id) {
        return Err(format!("invalid app id: {}", args.app_id));
    }

    let (title, rect) = {
        let conn = state.db.lock();
        ensure_plugin_tables(&conn)?;
        let row = get_installed_plugin(&conn, &args.plugin_id)?
            .ok_or_else(|| "plugin not found".to_string())?;
        if !row.enabled || !row.trusted {
            return Err("plugin is not enabled".into());
        }

        let install_path = packages_dir(&app)?.join(&row.id).join(&row.current_version);
        let manifest = ui::read_manifest(&install_path)?;
        let contribution = manifest
            .contributes
            .apps
            .iter()
            .find(|candidate| candidate.id == args.app_id)
            .ok_or_else(|| format!("plugin does not contribute app {}", args.app_id))?;
        if contribution.window_mode != PluginWindowMode::Standalone {
            return Err("plugin app windowMode is not standalone".into());
        }
        contribution.rect.validate()?;

        let package_hash = row.package_hash.unwrap_or_default();
        if package_hash.is_empty() {
            return Err("plugin package hash is unknown; re-import required".into());
        }
        verify_package_hash(&install_path, &package_hash)?;
        (contribution.name.clone(), contribution.rect.clone())
    };

    let plugin_id = args.plugin_id.clone();
    let app_id = args.app_id.clone();
    let label = plugin_window_label(&plugin_id, &app_id);
    if let Some(window) = app.get_webview_window(&label) {
        tracing::debug!(
            plugin_id = %plugin_id,
            app_id = %app_id,
            window_label = %label,
            "focusing existing standalone plugin window"
        );
        sync_macos_plugin_window_presence(&app, true);
        crate::logging::debug_if_err(window.show(), "show existing plugin window");
        crate::logging::debug_if_err(window.unminimize(), "restore existing plugin window");
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    // Opening follows the cursor just like the main panel. Using the panel's
    // current_monitor here can briefly return the monitor it occupied before this open.
    let resolved = resolve_window_rect(&app, None, &rect)?;

    host.register_plugin_window(
        label.clone(),
        PluginWindowInstance {
            plugin_id: args.plugin_id,
            app_local_id: args.app_id,
            params: args.params,
        },
    );

    let builder =
        WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("/?view=plugin-window".into()))
            .title(title)
            .inner_size(resolved.width, resolved.height)
            .decorations(true)
            .resizable(true)
            .maximizable(true)
            .minimizable(true)
            .closable(true)
            .skip_taskbar(false)
            .visible(false)
            .focused(false);

    let window = match builder.build() {
        Ok(window) => window,
        Err(error) => {
            host.remove_plugin_window(&label);
            tracing::error!(
                plugin_id = %plugin_id,
                app_id = %app_id,
                window_label = %label,
                error = %error,
                "failed to build standalone plugin window"
            );
            return Err(error.to_string());
        }
    };
    if let Err(error) = window.set_position(resolved.position) {
        host.remove_plugin_window(&label);
        crate::logging::debug_if_err(window.destroy(), "destroy unpositioned plugin window");
        return Err(error.to_string());
    }
    if let Err(error) = window.set_size(resolved.physical_size) {
        host.remove_plugin_window(&label);
        crate::logging::debug_if_err(window.destroy(), "destroy unsized plugin window");
        return Err(error.to_string());
    }

    let event_app = app.clone();
    let event_host = host.inner().clone();
    let event_label = label.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::CloseRequested { .. }) {
            if let Err(error) = persist_plugin_window_sessions(&event_app, &event_host, &event_label)
            {
                tracing::debug!(window_label = %event_label, error = %error, "failed to persist plugin window session");
            }
            return;
        }
        if !matches!(event, WindowEvent::Destroyed) {
            return;
        }
        event_host.remove_plugin_window(&event_label);
        for view_id in event_host.views_for_window(&event_label) {
            event_host.destroy_view(&view_id);
        }
        sync_macos_plugin_window_presence(&event_app, event_host.has_plugin_windows());
    });

    sync_macos_plugin_window_presence(&app, true);
    if let Err(error) = window.show().and_then(|_| window.set_focus()) {
        host.remove_plugin_window(&label);
        crate::logging::debug_if_err(window.destroy(), "destroy unopened plugin window");
        sync_macos_plugin_window_presence(&app, host.has_plugin_windows());
        tracing::error!(
            plugin_id = %plugin_id,
            app_id = %app_id,
            window_label = %label,
            error = %error,
            "failed to show standalone plugin window"
        );
        return Err(error.to_string());
    }
    tracing::info!(
        window_label = %label,
        "standalone plugin window opened"
    );
    Ok(())
}

/// Resolve context from the calling Tauri window label instead of accepting plugin identity
/// from JavaScript.
#[tauri::command]
pub fn plugin_window_context(
    window: WebviewWindow,
    host: State<'_, Arc<PluginHost>>,
) -> Result<PluginWindowContext, String> {
    host.plugin_window(window.label())
        .map(Into::into)
        .ok_or_else(|| "current window is not a plugin window".to_string())
}

pub fn set_plugin_window_rect(app: &AppHandle, label: &str, rect: &AppRect) -> Result<(), String> {
    rect.validate()?;
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| "plugin window not found".to_string())?;
    let resolved = resolve_window_rect(app, Some(&window), rect)?;
    window
        .set_position(resolved.position)
        .map_err(|error| error.to_string())?;
    window
        .set_size(resolved.physical_size)
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn resolve_window_rect(
    app: &AppHandle,
    reference_window: Option<&WebviewWindow>,
    rect: &AppRect,
) -> Result<ResolvedWindowRect, String> {
    rect.validate()?;
    let monitor = target_monitor(app, reference_window)
        .ok_or_else(|| "no monitor available for plugin window".to_string())?;
    Ok(resolve_rect_in_monitor(&monitor, rect))
}

fn target_monitor(app: &AppHandle, reference_window: Option<&WebviewWindow>) -> Option<Monitor> {
    reference_window
        .and_then(|window| window.current_monitor().ok().flatten())
        .or_else(|| {
            app.cursor_position().ok().and_then(|position| {
                app.monitor_from_point(position.x, position.y)
                    .ok()
                    .flatten()
            })
        })
        .or_else(|| app.primary_monitor().ok().flatten())
}

fn resolve_rect_in_monitor(monitor: &Monitor, rect: &AppRect) -> ResolvedWindowRect {
    let scale = monitor.scale_factor();
    let work_area = monitor.work_area();
    let available_width = work_area.size.width as f64 / scale;
    let available_height = work_area.size.height as f64 / scale;
    let width = resolve_dimension(
        rect.width.as_ref(),
        crate::auxiliary_windows::MAIN_PANEL_WIDTH,
        available_width,
        APP_WINDOW_MIN_WIDTH,
        APP_WINDOW_MAX_WIDTH,
    );
    let height = resolve_dimension(
        rect.height.as_ref(),
        DEFAULT_APP_WINDOW_HEIGHT,
        available_height,
        APP_WINDOW_MIN_HEIGHT,
        APP_WINDOW_MAX_HEIGHT,
    );
    let default_top = ((available_height - crate::auxiliary_windows::MAIN_PANEL_MAX_HEIGHT) / 2.0)
        .clamp(96.0, 320.0);
    let x = resolve_position(rect.x.as_ref(), available_width, width, None);
    let y = resolve_position(rect.y.as_ref(), available_height, height, Some(default_top));

    ResolvedWindowRect {
        width,
        height,
        physical_size: resolved_physical_size(width, height, scale),
        position: PhysicalPosition::new(
            work_area.position.x + (x * scale).round() as i32,
            work_area.position.y + (y * scale).round() as i32,
        ),
    }
}

fn resolved_physical_size(width: f64, height: f64, scale: f64) -> PhysicalSize<u32> {
    PhysicalSize::new(
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
}

fn resolve_dimension(
    value: Option<&RectValue>,
    default_pixels: f64,
    available: f64,
    min_pixels: f64,
    max_pixels: f64,
) -> f64 {
    let requested = match value {
        Some(RectValue::Pixels(pixels)) => *pixels,
        Some(RectValue::Expression(expression)) => {
            available * parse_percent(expression).unwrap_or(100.0) / 100.0
        }
        None => default_pixels,
    };
    let maximum = max_pixels.min(available.max(1.0));
    requested.clamp(min_pixels.min(maximum), maximum)
}

fn resolve_position(
    value: Option<&RectValue>,
    available: f64,
    window_size: f64,
    default_pixels: Option<f64>,
) -> f64 {
    let travel = (available - window_size).max(0.0);
    let requested = match value {
        Some(RectValue::Pixels(pixels)) => *pixels,
        Some(RectValue::Expression(expression))
            if expression.trim().eq_ignore_ascii_case("center") =>
        {
            travel / 2.0
        }
        Some(RectValue::Expression(expression)) => {
            travel * parse_percent(expression).unwrap_or(0.0) / 100.0
        }
        None => default_pixels.unwrap_or(travel / 2.0),
    };
    requested.clamp(0.0, travel)
}

fn parse_percent(expression: &str) -> Option<f64> {
    expression
        .trim()
        .strip_suffix('%')
        .and_then(|number| number.trim().parse::<f64>().ok())
}

pub fn close_plugin_window_later(app: &AppHandle, label: &str) {
    let app = app.clone();
    let label = label.to_string();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        if let Some(window) = app.get_webview_window(&label) {
            crate::logging::debug_if_err(window.close(), "close plugin window");
        }
    });
}

pub fn close_plugin_windows(app: &AppHandle, host: &PluginHost, plugin_id: &str) {
    for label in host.plugin_window_labels_for_plugin(plugin_id) {
        if let Some(window) = app.get_webview_window(&label) {
            crate::logging::debug_if_err(window.destroy(), "destroy disabled plugin window");
        }
        host.remove_plugin_window(&label);
        for view_id in host.views_for_window(&label) {
            host.destroy_view(&view_id);
        }
    }
    sync_macos_plugin_window_presence(app, host.has_plugin_windows());
}

fn persist_plugin_window_sessions(
    app: &AppHandle,
    host: &PluginHost,
    label: &str,
) -> Result<(), String> {
    let Some(window_context) = host.plugin_window(label) else {
        return Ok(());
    };
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "app state unavailable".to_string())?;
    let conn = state.db.lock();
    ensure_plugin_tables(&conn)?;
    let Some(row) = get_installed_plugin(&conn, &window_context.plugin_id)? else {
        return Ok(());
    };
    let install_path = packages_dir(app)?
        .join(&window_context.plugin_id)
        .join(&row.current_version);
    let manifest = ui::read_manifest(&install_path)?;
    let session_version = manifest
        .contributes
        .apps
        .iter()
        .find(|app| app.id == window_context.app_local_id)
        .and_then(|app| app.session_version)
        .unwrap_or(1);

    for view_id in host.views_for_window(label) {
        let Some(payload) = host.take_cached_session_payload(&view_id) else {
            continue;
        };
        ui::save_session(
            &conn,
            &window_context.plugin_id,
            &window_context.app_local_id,
            &row.current_version,
            session_version,
            &payload,
        )?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn sync_macos_plugin_window_presence(app: &AppHandle, visible: bool) {
    if visible {
        crate::logging::debug_if_err(
            app.set_activation_policy(tauri::ActivationPolicy::Regular),
            "set normal macos activation policy for plugin window",
        );
        crate::logging::debug_if_err(
            app.set_dock_visibility(true),
            "show macos dock icon for plugin window",
        );
    } else {
        crate::macos_dock::ensure_accessory_policy(app);
    }
}

#[cfg(not(target_os = "macos"))]
fn sync_macos_plugin_window_presence(_app: &AppHandle, _visible: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_labels_are_stable_and_do_not_expose_plugin_ids() {
        let first = plugin_window_label("com.example.hello", "main");
        assert_eq!(first, plugin_window_label("com.example.hello", "main"));
        assert_ne!(first, plugin_window_label("com.example.hello", "other"));
        assert!(first.starts_with(WINDOW_LABEL_PREFIX));
        assert!(!first.contains("com.example.hello"));
    }

    #[test]
    fn percentages_and_center_use_the_resolved_window_size() {
        let width = resolve_dimension(
            Some(&RectValue::Expression("75%".into())),
            920.0,
            1920.0,
            APP_WINDOW_MIN_WIDTH,
            APP_WINDOW_MAX_WIDTH,
        );
        assert_eq!(width, 1440.0);
        assert_eq!(
            resolve_position(
                Some(&RectValue::Expression("center".into())),
                1920.0,
                width,
                None,
            ),
            240.0
        );
        assert_eq!(
            resolve_position(
                Some(&RectValue::Expression("100%".into())),
                1920.0,
                width,
                None,
            ),
            480.0
        );
    }

    #[test]
    fn omitted_rect_uses_main_panel_defaults() {
        assert_eq!(
            resolve_dimension(
                None,
                crate::auxiliary_windows::MAIN_PANEL_WIDTH,
                1920.0,
                APP_WINDOW_MIN_WIDTH,
                APP_WINDOW_MAX_WIDTH,
            ),
            crate::auxiliary_windows::MAIN_PANEL_WIDTH
        );
        assert_eq!(
            resolve_dimension(
                None,
                DEFAULT_APP_WINDOW_HEIGHT,
                1080.0,
                APP_WINDOW_MIN_HEIGHT,
                APP_WINDOW_MAX_HEIGHT,
            ),
            DEFAULT_APP_WINDOW_HEIGHT
        );
    }

    #[test]
    fn physical_size_uses_the_target_monitor_scale() {
        assert_eq!(
            resolved_physical_size(800.0, 580.0, 1.0),
            PhysicalSize::new(800, 580)
        );
        assert_eq!(
            resolved_physical_size(800.0, 580.0, 1.5),
            PhysicalSize::new(1200, 870)
        );
    }
}
