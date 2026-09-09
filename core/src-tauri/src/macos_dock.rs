#![allow(unexpected_cfgs)]

use tauri::{ActivationPolicy, AppHandle};

/// Tempo is a menu-bar / tray app on macOS: never show a Dock icon.
/// Prefer setting Accessory on `App` before `run()` (see `lib.rs`) so the Dock never flashes;
/// this helper is for later runtime reinforcement.
pub fn ensure_accessory_policy(app: &AppHandle) {
    crate::logging::debug_if_err(
        app.set_activation_policy(ActivationPolicy::Accessory),
        "set macos accessory activation policy",
    );
    crate::logging::debug_if_err(app.set_dock_visibility(false), "hide macos dock icon");
}
