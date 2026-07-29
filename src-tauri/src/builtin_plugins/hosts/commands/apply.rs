use tauri::AppHandle;

use super::support::*;
use super::types::*;

#[tauri::command]
pub fn apply_hosts(app: AppHandle) -> Result<HostsWorkspace, String> {
    apply_composed(&app, "apply")
}

#[tauri::command]
pub fn flush_dns() -> Result<(), String> {
    flush_dns_cache()
}

