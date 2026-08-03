use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

use crate::auxiliary_windows::{
    clamp_position_to_monitor, monitor_containing_position, MAIN_PANEL_LABEL,
};

pub const LAUNCHER_CONTEXT_MENU_LABEL: &str = "launcher-context-menu";

const MENU_WIDTH: f64 = 172.0;
const MENU_ITEM_HEIGHT: f64 = 32.0;
const MENU_PAD_Y: f64 = 6.0;
const MENU_SEPARATOR_HEIGHT: f64 = 9.0;

const DISABLE_NATIVE_CONTEXT_MENU_SCRIPT: &str = r#"
document.addEventListener("contextmenu", function (event) {
  event.preventDefault();
}, { capture: true });
"#;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherContextMenuItem {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub danger: bool,
    #[serde(default)]
    pub separator_before: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowLauncherContextMenuArgs {
    /// Cursor position in physical screen pixels.
    pub x: i32,
    pub y: i32,
    pub items: Vec<LauncherContextMenuItem>,
    pub target: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherContextMenuPreparePayload {
    items: Vec<LauncherContextMenuItem>,
    target: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherContextMenuClosedPayload {
    reason: String,
}

fn menu_logical_size(items: &[LauncherContextMenuItem]) -> (f64, f64) {
    let mut height = MENU_PAD_Y * 2.0;
    for item in items {
        if item.separator_before {
            height += MENU_SEPARATOR_HEIGHT;
        }
        height += MENU_ITEM_HEIGHT;
    }
    if items.is_empty() {
        height = MENU_PAD_Y * 2.0 + MENU_ITEM_HEIGHT;
    }
    (MENU_WIDTH, height)
}

fn menu_opaque_background(app: &AppHandle) -> Color {
    let theme = app
        .try_state::<crate::db::AppState>()
        .map(|state| {
            let conn = state.db.lock();
            crate::db::get_setting(&conn, "theme", "system")
        })
        .unwrap_or_else(|| "system".into());
    let dark = match theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => crate::platform::system_prefers_dark(),
    };
    // Match CSS `--popover`: light `0 0% 100%`, dark `220 12% 11%`.
    if dark {
        Color(25, 27, 31, 255)
    } else {
        Color(255, 255, 255, 255)
    }
}

fn build_launcher_context_menu_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let background = menu_opaque_background(app);
    WebviewWindowBuilder::new(
        app,
        LAUNCHER_CONTEXT_MENU_LABEL,
        WebviewUrl::App("/?view=launcher-context-menu".into()),
    )
    .title("")
    .inner_size(MENU_WIDTH, MENU_PAD_Y * 2.0 + MENU_ITEM_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(false)
    .background_color(background)
    .shadow(cfg!(any(target_os = "macos", target_os = "windows")))
    .always_on_top(true)
    .skip_taskbar(true)
    .visible_on_all_workspaces(true)
    .visible(false)
    .focused(false)
    .initialization_script(DISABLE_NATIVE_CONTEXT_MENU_SCRIPT)
    .build()
}

fn polish_launcher_context_menu_window(app: &AppHandle, window: &WebviewWindow) {
    let theme = app
        .try_state::<crate::db::AppState>()
        .map(|state| {
            let conn = state.db.lock();
            crate::db::get_setting(&conn, "theme", "system")
        })
        .unwrap_or_else(|| "system".into());
    let native = match theme.as_str() {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => Some(if crate::platform::system_prefers_dark() {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }),
    };
    crate::logging::debug_if_err(window.set_theme(native), "sync launcher context menu theme");

    let background = menu_opaque_background(app);
    crate::logging::debug_if_err(
        window.set_background_color(Some(background)),
        "set launcher context menu opaque background",
    );
    crate::logging::debug_if_err(window.set_shadow(true), "set launcher context menu shadow");
}

fn ensure_launcher_context_menu_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(LAUNCHER_CONTEXT_MENU_LABEL) {
        polish_launcher_context_menu_window(app, &window);
        return Ok(window);
    }
    let window = build_launcher_context_menu_window(app)?;
    polish_launcher_context_menu_window(app, &window);
    crate::logging::debug_if_err(window.hide(), "hide newly created launcher context menu");
    Ok(window)
}

/// Create the floating menu webview during app setup (not from a sync invoke).
/// On Windows, building a WebviewWindow inside a synchronous command deadlocks.
pub fn precache(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(LAUNCHER_CONTEXT_MENU_LABEL).is_some() {
        return Ok(());
    }
    let window = build_launcher_context_menu_window(app)?;
    polish_launcher_context_menu_window(app, &window);
    crate::logging::debug_if_err(window.hide(), "precache hide launcher context menu");
    Ok(())
}

fn place_launcher_context_menu(
    app: &AppHandle,
    window: &WebviewWindow,
    x: i32,
    y: i32,
    items: &[LauncherContextMenuItem],
) -> tauri::Result<()> {
    let (width, height) = menu_logical_size(items);
    window.set_size(LogicalSize::new(width, height))?;

    let cursor = PhysicalPosition::new(x, y);
    let monitor = monitor_containing_position(app, cursor)
        .or_else(|| {
            app.get_webview_window(MAIN_PANEL_LABEL)
                .and_then(|main| main.current_monitor().ok().flatten())
        })
        .or_else(|| app.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        window.set_position(cursor)?;
        return Ok(());
    };

    let scale = monitor.scale_factor();
    let physical = PhysicalSize::new(
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    );
    let position = clamp_position_to_monitor(cursor, physical, &monitor);
    window.set_position(position)?;
    Ok(())
}

fn emit_closed(app: &AppHandle, reason: &str) {
    let payload = LauncherContextMenuClosedPayload {
        reason: reason.to_string(),
    };
    crate::logging::debug_if_err(
        app.emit_to(MAIN_PANEL_LABEL, "launcher-context-menu:closed", &payload),
        "emit launcher-context-menu:closed",
    );
    crate::logging::debug_if_err(
        app.emit_to(
            LAUNCHER_CONTEXT_MENU_LABEL,
            "launcher-context-menu:hide",
            &payload,
        ),
        "emit launcher-context-menu:hide",
    );
}

/// Async so a fallback window create on Windows cannot deadlock the IPC thread.
#[tauri::command]
pub async fn show_launcher_context_menu(
    app: AppHandle,
    args: ShowLauncherContextMenuArgs,
) -> Result<(), String> {
    if args.items.is_empty() {
        return Err("右键菜单不能为空".into());
    }

    let window = ensure_launcher_context_menu_window(&app).map_err(|error| error.to_string())?;
    let payload = LauncherContextMenuPreparePayload {
        items: args.items.clone(),
        target: args.target.clone(),
    };

    crate::logging::debug_if_err(
        app.emit_to(
            LAUNCHER_CONTEXT_MENU_LABEL,
            "launcher-context-menu:prepare",
            &payload,
        ),
        "emit launcher-context-menu:prepare",
    );

    place_launcher_context_menu(&app, &window, args.x, args.y, &args.items)
        .map_err(|error| error.to_string())?;

    crate::logging::debug_if_err(
        window.set_always_on_top(true),
        "set launcher context menu always on top",
    );

    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;

    crate::logging::debug_if_err(
        app.emit_to(
            LAUNCHER_CONTEXT_MENU_LABEL,
            "launcher-context-menu:open",
            &payload,
        ),
        "emit launcher-context-menu:open",
    );
    Ok(())
}

#[tauri::command]
pub fn hide_launcher_context_menu(
    app: AppHandle,
    reason: Option<String>,
) -> Result<(), String> {
    hide_launcher_context_menu_window(&app, reason.as_deref().unwrap_or("dismiss"))
        .map_err(|error| error.to_string())
}

pub fn hide_launcher_context_menu_window(
    app: &AppHandle,
    reason: &str,
) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(LAUNCHER_CONTEXT_MENU_LABEL) {
        if window.is_visible().unwrap_or(false) {
            crate::logging::debug_if_err(window.hide(), "hide launcher context menu");
        }
    }

    // Always notify so main-panel blur-hide suppress is released even if already hidden.
    emit_closed(app, reason);
    Ok(())
}

/// Close the floating menu when the main panel itself is dismissed.
pub fn hide_with_main_panel(app: &AppHandle) {
    let _ = hide_launcher_context_menu_window(app, "dismiss");
}

#[tauri::command]
pub async fn launcher_context_menu_action(
    app: AppHandle,
    action_id: String,
    target: serde_json::Value,
) -> Result<(), String> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ActionPayload {
        action_id: String,
        target: serde_json::Value,
    }

    let payload = ActionPayload { action_id, target };
    crate::logging::debug_if_err(
        app.emit_to(MAIN_PANEL_LABEL, "launcher-context-menu:action", &payload),
        "emit launcher-context-menu:action",
    );
    hide_launcher_context_menu_window(&app, "action").map_err(|error| error.to_string())?;

    if let Some(main) = app.get_webview_window(MAIN_PANEL_LABEL) {
        let _ = main.set_focus();
    }
    Ok(())
}
