//! Main panel visibility controller.
//!
//! Single owner of "is the quick panel open". Every show / hide funnels through
//! here and the frontend only observes `main-panel:shown` / `main-panel:hidden`.
//!
//! Auto-hide follows **native app activation** (see [`activation`]): the panel
//! closes when another application takes over the foreground. It never closes
//! because a child window, WebView DevTools, a native dialog or one of Tempo's
//! own windows (context menu, shelf, plugin window) took keyboard focus.
//!
//! Callers that need the panel to survive a genuine deactivation (UAC prompts,
//! native pickers, "pin window" in plugin dev mode) register a named hold.

mod activation;

use crate::auxiliary_windows::MAIN_PANEL_LABEL;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent};

/// Let the OS settle the next key / foreground window before judging whether
/// the app is still active: menus, sheets and companion windows become key a
/// few milliseconds after the panel resigns.
const DEACTIVATION_SETTLE: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShowReason {
    Startup,
    Shortcut,
    Tray,
    Command,
    SecondInstance,
    /// macOS Dock / Finder reopen with no visible windows.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Reopen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HideReason {
    /// Another application became active.
    Deactivated,
    /// Global shortcut toggled the panel closed.
    Shortcut,
    /// The panel itself asked to close (Esc, item launched, ...).
    Command,
    /// A plugin called `mainPanel.hide`.
    Plugin,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainPanelShownPayload {
    pub generation: u64,
    pub reason: ShowReason,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainPanelHiddenPayload {
    pub generation: u64,
    pub reason: HideReason,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainPanelState {
    pub visible: bool,
    pub generation: u64,
    pub holds: Vec<String>,
}

#[derive(Default)]
struct Controller {
    /// Logical visibility. Flipped *before* the native show / hide so a
    /// shortcut arriving mid-transition toggles the right way.
    visible: bool,
    /// Incremented on every show; frontend hide requests carry the generation
    /// they were issued for so a stale hide cannot close a newer session.
    generation: u64,
    /// Named reasons to ignore app deactivation.
    holds: BTreeSet<String>,
    /// Invalidates deferred deactivation checks that are already in flight.
    probe_token: u64,
}

static CONTROLLER: OnceLock<Mutex<Controller>> = OnceLock::new();

fn controller() -> &'static Mutex<Controller> {
    CONTROLLER.get_or_init(|| Mutex::new(Controller::default()))
}

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Install platform activation tracking. Call once from `setup`.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    activation::install(app);
}

pub fn is_visible() -> bool {
    controller().lock().visible
}

pub fn state() -> MainPanelState {
    let controller = controller().lock();
    MainPanelState {
        visible: controller.visible,
        generation: controller.generation,
        holds: controller.holds.iter().cloned().collect(),
    }
}

pub fn show(app: &AppHandle, reason: ShowReason) -> tauri::Result<()> {
    let generation = {
        let mut controller = controller().lock();
        controller.generation = controller.generation.wrapping_add(1);
        controller.visible = true;
        controller.probe_token = controller.probe_token.wrapping_add(1);
        controller.generation
    };

    if let Err(error) = present(app) {
        controller().lock().visible = false;
        return Err(error);
    }

    tracing::debug!(?reason, generation, "main panel shown");
    emit(
        app,
        "main-panel:shown",
        MainPanelShownPayload { generation, reason },
    );
    crate::commands::launcher::request_launcher_index_refresh(app);
    Ok(())
}

fn present(app: &AppHandle) -> tauri::Result<()> {
    let window = crate::auxiliary_windows::prepare_main_panel_for_show(app)?;

    #[cfg(target_os = "macos")]
    {
        let config = crate::macos_overlay_panel::main_panel_config();
        crate::macos_overlay_panel::ensure_input_panel(app, &window, MAIN_PANEL_LABEL, &config)?;
        crate::macos_overlay_panel::show_input_overlay(app, MAIN_PANEL_LABEL)?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        window.show()?;
        window.set_focus()?;
    }

    Ok(())
}

/// Hide the panel. Returns `Ok(false)` when it was already hidden.
pub fn hide(app: &AppHandle, reason: HideReason) -> tauri::Result<bool> {
    hide_generation(app, None, reason)
}

/// Hide only if `generation` still matches the current session (or is `None`).
pub fn hide_generation(
    app: &AppHandle,
    generation: Option<u64>,
    reason: HideReason,
) -> tauri::Result<bool> {
    hide_if(app, reason, |controller| {
        generation.is_none_or(|requested| requested == controller.generation)
    })
}

/// Hide when the panel is visible and `guard` holds — both evaluated under the
/// controller lock so a concurrent `show` cannot slip in between.
fn hide_if(
    app: &AppHandle,
    reason: HideReason,
    guard: impl FnOnce(&Controller) -> bool,
) -> tauri::Result<bool> {
    let generation = {
        let mut controller = controller().lock();
        if !controller.visible || !guard(&controller) {
            return Ok(false);
        }
        controller.visible = false;
        controller.probe_token = controller.probe_token.wrapping_add(1);
        controller.generation
    };

    crate::launcher_context_menu::hide_with_main_panel(app);

    if let Some(window) = app.get_webview_window(MAIN_PANEL_LABEL) {
        #[cfg(target_os = "macos")]
        {
            let _ = &window;
            crate::macos_overlay_panel::hide_overlay(app, MAIN_PANEL_LABEL);
        }
        #[cfg(not(target_os = "macos"))]
        window.hide()?;
    }

    tracing::debug!(?reason, generation, "main panel hidden");
    emit(
        app,
        "main-panel:hidden",
        MainPanelHiddenPayload { generation, reason },
    );
    Ok(true)
}

/// Global shortcut: close when the panel is the active window, otherwise bring
/// it (back) to the front — covers "visible but another Tempo window is focused".
pub fn toggle(app: &AppHandle) -> tauri::Result<()> {
    let panel_active = is_visible()
        && app
            .get_webview_window(MAIN_PANEL_LABEL)
            .is_some_and(|window| activation::main_panel_is_active(&window));
    tracing::debug!(panel_active, "main panel toggle");
    if panel_active {
        hide(app, HideReason::Shortcut).map(|_| ())
    } else {
        show(app, ShowReason::Shortcut)
    }
}

pub fn hold(key: impl Into<String>) {
    controller().lock().holds.insert(key.into());
}

pub fn release(key: &str) {
    controller().lock().holds.remove(key);
}

/// Drop every hold. Called when the panel webview navigates so a hold owned by
/// the unloaded page cannot pin the panel open forever.
pub fn clear_holds() {
    controller().lock().holds.clear();
}

/// Window focus feed for platforms without a system-wide foreground hook.
/// Wire from `tauri::Builder::on_window_event`.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let WindowEvent::Focused(focused) = event else {
        return;
    };

    // Windows: `activation` observes EVENT_SYSTEM_FOREGROUND, which is
    // process-aware and unaffected by focus moving into the WebView2 child HWND.
    if cfg!(windows) {
        let _ = (window, focused);
        return;
    }

    if *focused {
        cancel_deactivation_probe();
        return;
    }
    // Any Tempo window resigning key may mean the whole app deactivated
    // (e.g. the context menu was key while the panel stayed visible).
    if is_visible() {
        schedule_deactivation_probe(window.app_handle());
    }
}

/// Called by [`activation`] when another process' window became foreground.
pub(crate) fn note_foreign_foreground() {
    if !is_visible() {
        return;
    }
    if let Some(app) = APP.get() {
        schedule_deactivation_probe(app);
    }
}

/// Called by [`activation`] when one of our own windows became foreground.
pub(crate) fn note_own_foreground() {
    cancel_deactivation_probe();
}

fn cancel_deactivation_probe() {
    let mut controller = controller().lock();
    controller.probe_token = controller.probe_token.wrapping_add(1);
}

fn schedule_deactivation_probe(app: &AppHandle) {
    let token = {
        let mut controller = controller().lock();
        controller.probe_token = controller.probe_token.wrapping_add(1);
        controller.probe_token
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(DEACTIVATION_SETTLE).await;
        if controller().lock().probe_token != token {
            return;
        }
        let app_for_main = app.clone();
        let _ = app.run_on_main_thread(move || settle_deactivation(&app_for_main, token));
    });
}

/// Runs on the main thread after [`DEACTIVATION_SETTLE`]. `token` identifies
/// the probe; any show / hide / own-foreground in the meantime invalidates it.
fn settle_deactivation(app: &AppHandle, token: u64) {
    let still_relevant = |controller: &Controller| {
        controller.probe_token == token && controller.holds.is_empty()
    };
    if !still_relevant(&controller().lock()) {
        return;
    }
    // Platform query outside the lock; the guard re-validates the token so a
    // `show` racing with this probe wins.
    if activation::app_is_active(app) {
        return;
    }
    tracing::debug!("app deactivated; hiding main panel");
    crate::logging::debug_if_err(
        hide_if(app, HideReason::Deactivated, still_relevant),
        "hide main panel on app deactivation",
    );
}

fn emit<P: Serialize + Clone>(app: &AppHandle, event: &str, payload: P) {
    crate::logging::debug_if_err(
        app.emit_to(MAIN_PANEL_LABEL, event, payload),
        "emit main panel visibility event",
    );
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn main_panel_show(app: AppHandle) -> Result<(), String> {
    show(&app, ShowReason::Command).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn main_panel_hide(app: AppHandle, generation: Option<u64>) -> Result<bool, String> {
    hide_generation(&app, generation, HideReason::Command).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn main_panel_state() -> MainPanelState {
    state()
}

#[tauri::command]
pub fn main_panel_hold(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("hold key must not be empty".into());
    }
    hold(key);
    Ok(())
}

#[tauri::command]
pub fn main_panel_release(key: String) {
    release(key.trim());
}
