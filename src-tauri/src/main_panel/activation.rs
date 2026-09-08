//! Platform probes answering "is Tempo still the active application?".
//!
//! * Windows — an out-of-context `EVENT_SYSTEM_FOREGROUND` hook reports every
//!   foreground change; a window counts as ours when its process is this
//!   process or a descendant (WebView2 browser / DevTools, plugin runtimes).
//! * macOS — the panel is a non-activating `NSPanel`, so `NSApp.isActive` is
//!   meaningless. Tempo is active while any of its windows is key or a modal
//!   session (`NSOpenPanel`, alerts) is running. Resign-key events arrive via
//!   `tauri::WindowEvent::Focused(false)` and are fed in by the controller.
//! * Other — any Tempo window focused.

pub use imp::{app_is_active, install, main_panel_is_active};

#[cfg(not(windows))]
fn any_window_focused(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;

    app.webview_windows()
        .values()
        .any(|window| window.is_focused().unwrap_or(false))
}

#[cfg(windows)]
mod imp {
    use std::collections::HashMap;
    use tauri::{AppHandle, WebviewWindow};
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, GetWindowThreadProcessId, EVENT_SYSTEM_FOREGROUND,
        GA_ROOT, WINEVENT_OUTOFCONTEXT,
    };

    /// WebView2 (browser → renderer) and plugin runtimes sit at most this many
    /// levels below the host process. Bounding the walk also caps damage from
    /// PID reuse in the parent chain.
    const MAX_ANCESTRY_DEPTH: usize = 4;

    /// Out-of-context WinEvent callbacks are delivered through the message loop
    /// of the installing thread — the Tauri main loop pumps ours.
    pub fn install(app: &AppHandle) {
        let _ = app.run_on_main_thread(|| {
            let hook = unsafe {
                SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    None,
                    Some(on_foreground_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if hook.0.is_null() {
                tracing::warn!("failed to install foreground hook; main panel will not auto-hide");
            } else {
                tracing::info!("main panel foreground hook installed");
            }
        });
    }

    unsafe extern "system" fn on_foreground_event(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _event_thread: u32,
        _event_time: u32,
    ) {
        if event != EVENT_SYSTEM_FOREGROUND {
            return;
        }
        // Cheap own-process test here; the deferred probe does the full
        // ancestry walk so DevTools / WebView2 windows are still recognised.
        let own = !hwnd.0.is_null() && window_process_id(hwnd) == unsafe { GetCurrentProcessId() };
        if own {
            super::super::note_own_foreground();
        } else {
            super::super::note_foreign_foreground();
        }
    }

    pub fn app_is_active(_app: &AppHandle) -> bool {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return false;
        }
        let pid = window_process_id(foreground);
        pid != 0 && process_tree_contains(pid)
    }

    pub fn main_panel_is_active(window: &WebviewWindow) -> bool {
        let Some(hwnd) = crate::auxiliary_windows::windows_hwnd(window) else {
            return window.is_focused().unwrap_or(false);
        };
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return false;
        }
        foreground == hwnd || unsafe { GetAncestor(foreground, GA_ROOT) } == hwnd
    }

    fn window_process_id(hwnd: HWND) -> u32 {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid
    }

    fn process_tree_contains(pid: u32) -> bool {
        let own = unsafe { GetCurrentProcessId() };
        if pid == own {
            return true;
        }
        let Some(parents) = parent_process_map() else {
            return false;
        };
        let mut current = pid;
        for _ in 0..MAX_ANCESTRY_DEPTH {
            let Some(&parent) = parents.get(&current) else {
                return false;
            };
            if parent == own {
                return true;
            }
            if parent == 0 || parent == current {
                return false;
            }
            current = parent;
        }
        false
    }

    /// `pid -> parent pid` for every live process.
    fn parent_process_map() -> Option<HashMap<u32, u32>> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut parents = HashMap::new();
            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
            Some(parents)
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use tauri::{AppHandle, WebviewWindow};

    /// Resign-key notifications reach the controller through window events.
    pub fn install(_app: &AppHandle) {}

    /// Must run on the main thread (AppKit).
    pub fn app_is_active(app: &AppHandle) -> bool {
        use objc::runtime::{Class, Object};
        use objc::{msg_send, sel, sel_impl};

        unsafe {
            if let Some(app_class) = Class::get("NSApplication") {
                let ns_app: *mut Object = msg_send![app_class, sharedApplication];
                if !ns_app.is_null() {
                    let key_window: *mut Object = msg_send![ns_app, keyWindow];
                    let modal_window: *mut Object = msg_send![ns_app, modalWindow];
                    if !key_window.is_null() || !modal_window.is_null() {
                        return true;
                    }
                }
            }
        }
        super::any_window_focused(app)
    }

    pub fn main_panel_is_active(window: &WebviewWindow) -> bool {
        window.is_focused().unwrap_or(false)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use tauri::{AppHandle, WebviewWindow};

    pub fn install(_app: &AppHandle) {}

    pub fn app_is_active(app: &AppHandle) -> bool {
        super::any_window_focused(app)
    }

    pub fn main_panel_is_active(window: &WebviewWindow) -> bool {
        window.is_focused().unwrap_or(false)
    }
}
