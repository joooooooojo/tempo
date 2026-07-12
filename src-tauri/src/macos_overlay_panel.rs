//! Non-activating NSPanel overlays: show/focus overlays without activating Tempo
//! or touching the main window.

use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_nspanel::{
    tauri_panel, CollectionBehavior, ManagerExt, Panel, PanelLevel, StyleMask, WebviewWindowExt,
};

tauri_panel! {
    panel!(OverlayInputPanel {
        config: {
            can_become_key_window: true,
            can_become_main_window: false,
        }
    })

    panel!(OverlayPassivePanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
        }
    })
}

#[derive(Clone, Copy)]
pub struct OverlayPanelConfig {
    pub level: PanelLevel,
    pub collection_behavior: CollectionBehavior,
    pub has_shadow: bool,
    pub becomes_key_only_if_needed: bool,
}

impl OverlayPanelConfig {
    fn apply_input(&self, panel: &dyn Panel) {
        apply_base(panel, true, self);
    }

    fn apply_passive(&self, panel: &dyn Panel) {
        apply_base(panel, false, self);
    }
}

fn overlay_style_mask() -> StyleMask {
    StyleMask::empty().borderless().nonactivating_panel()
}

fn apply_base(panel: &dyn Panel, input: bool, config: &OverlayPanelConfig) {
    panel.set_style_mask(overlay_style_mask().into());
    panel.set_floating_panel(true);
    panel.set_becomes_key_only_if_needed(if input {
        config.becomes_key_only_if_needed
    } else {
        false
    });
    panel.set_hides_on_deactivate(false);
    panel.set_level(config.level.value());
    panel.set_collection_behavior(config.collection_behavior.into());
    panel.set_transparent(true);
    panel.set_has_shadow(config.has_shadow);
    panel.set_opaque(false);
}

pub fn shelf_picker_config() -> OverlayPanelConfig {
    OverlayPanelConfig {
        level: PanelLevel::Status,
        collection_behavior: CollectionBehavior::new()
            .can_join_all_spaces()
            .stationary()
            .full_screen_auxiliary()
            .full_screen_none(),
        has_shadow: true,
        becomes_key_only_if_needed: true,
    }
}

pub fn shelf_backdrop_config() -> OverlayPanelConfig {
    OverlayPanelConfig {
        level: PanelLevel::MainMenu,
        collection_behavior: CollectionBehavior::new()
            .can_join_all_spaces()
            .stationary()
            .full_screen_auxiliary(),
        has_shadow: false,
        becomes_key_only_if_needed: false,
    }
}

pub fn ensure_input_panel(
    app: &AppHandle,
    window: &WebviewWindow,
    label: &str,
    config: &OverlayPanelConfig,
) -> tauri::Result<()> {
    if app.get_webview_panel(label).is_ok() {
        return Ok(());
    }

    let panel = window.to_panel::<OverlayInputPanel>()?;
    config.apply_input(panel.as_ref());
    Ok(())
}

pub fn ensure_passive_panel(
    app: &AppHandle,
    window: &WebviewWindow,
    label: &str,
    config: &OverlayPanelConfig,
) -> tauri::Result<()> {
    if app.get_webview_panel(label).is_ok() {
        return Ok(());
    }

    let panel = window.to_panel::<OverlayPassivePanel>()?;
    config.apply_passive(panel.as_ref());
    Ok(())
}

pub fn show_input_overlay(app: &AppHandle, label: &str) -> tauri::Result<()> {
    let panel = app
        .get_webview_panel(label)
        .map_err(|_| tauri::Error::WindowNotFound)?;
    panel.show_and_make_key();
    Ok(())
}

pub fn show_passive_overlay(app: &AppHandle, label: &str) -> tauri::Result<()> {
    let panel = app
        .get_webview_panel(label)
        .map_err(|_| tauri::Error::WindowNotFound)?;
    panel.order_front_regardless();
    Ok(())
}

pub fn hide_overlay(app: &AppHandle, label: &str) {
    if let Ok(panel) = app.get_webview_panel(label) {
        panel.hide();
        return;
    }

    if let Some(window) = app.get_webview_window(label) {
        let _ = window.hide();
    }
}
