use tauri::AppHandle;

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn debug_log(scope: String, message: String) {
    tracing::debug!(
        target: "tempo::frontend",
        scope = %crate::logging::sanitize_log_value(&scope),
        message_chars = message.chars().count(),
        "frontend debug log"
    );
}

/// Whether the OS appearance preference is dark (for theme = "system").
#[tauri::command]
pub fn system_prefers_dark() -> bool {
    crate::platform::system_prefers_dark()
}

/// Open the main-panel WebView inspector. Debug / `devtools` feature builds only.
#[tauri::command]
pub fn open_main_panel_devtools(app: AppHandle) -> Result<(), String> {
    #[cfg(not(any(debug_assertions, feature = "devtools")))]
    {
        let _ = app;
        return Err("仅开发环境可用".into());
    }

    #[cfg(any(debug_assertions, feature = "devtools"))]
    {
        use tauri::Manager;
        let window = app
            .get_webview_window(crate::auxiliary_windows::MAIN_PANEL_LABEL)
            .ok_or_else(|| "未找到主面板窗口".to_string())?;
        window.open_devtools();
        Ok(())
    }
}

/// Whether the main-panel inspector is open.
///
/// On Windows, Tauri's `is_devtools_open` is unsupported for WebView2, and there
/// is no official WebView2 API either. Community workaround: find a visible
/// top-level window whose process is `msedgewebview2` and whose title looks like
/// DevTools (often `DevTools - <url>`). Do **not** filter by the host app PID —
/// DevTools lives in a separate Edge WebView2 process.
#[tauri::command]
pub fn is_main_panel_devtools_open(app: AppHandle) -> bool {
    #[cfg(not(any(debug_assertions, feature = "devtools")))]
    {
        let _ = app;
        return false;
    }

    #[cfg(any(debug_assertions, feature = "devtools"))]
    {
        use tauri::Manager;
        if app
            .get_webview_window(crate::auxiliary_windows::MAIN_PANEL_LABEL)
            .is_some_and(|window| window.is_devtools_open())
        {
            return true;
        }

        #[cfg(windows)]
        {
            return windows_devtools_window_open();
        }

        #[cfg(not(windows))]
        {
            false
        }
    }
}

/// WebView2 DevTools runs under `msedgewebview2.exe`, not the Tempo process.
/// See: MicrosoftEdge/WebView2Feedback#2657 / #4157 (title + process name hack).
#[cfg(windows)]
fn windows_devtools_window_open() -> bool {
    use windows::Win32::Foundation::{BOOL, CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct EnumState {
        found: bool,
    }

    fn title_looks_like_devtools(title: &str) -> bool {
        let t = title.trim();
        t.starts_with("DevTools")
            || t.contains("DevTools -")
            || t.starts_with("开发者工具")
            || t.contains("开发者工具 -")
    }

    /// Prefer our panel URL so another app's WebView2 DevTools is less likely to match.
    fn title_looks_like_tempo_page(title: &str) -> bool {
        title.contains("localhost:1420")
            || title.contains("tauri.localhost")
            || title.contains("tauri://localhost")
            || title.contains("asset.localhost")
    }

    fn process_is_msedgewebview2(pid: u32) -> bool {
        unsafe {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return false;
            };
            let mut buf = [0u16; 512];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut size,
            )
            .is_ok();
            let _ = CloseHandle(handle);
            if !ok || size == 0 {
                return false;
            }
            let path = String::from_utf16_lossy(&buf[..size as usize]).to_ascii_lowercase();
            path.contains("msedgewebview2.exe")
        }
    }

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = &mut *(lparam.0 as *mut EnumState);
        if state.found {
            return BOOL(0);
        }
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }

        let mut buf = [0u16; 512];
        let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if len <= 0 {
            return BOOL(1);
        }
        let title = String::from_utf16_lossy(&buf[..len as usize]);
        if !title_looks_like_devtools(&title) {
            return BOOL(1);
        }

        let mut window_pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut window_pid)) };
        let from_webview2 = window_pid != 0 && process_is_msedgewebview2(window_pid);
        // Accept: msedgewebview2 DevTools, or any DevTools title that includes our URL.
        if from_webview2 || title_looks_like_tempo_page(&title) {
            state.found = true;
            return BOOL(0);
        }
        BOOL(1)
    }

    let mut state = EnumState { found: false };
    unsafe {
        let _ = EnumWindows(Some(enum_callback), LPARAM(&mut state as *mut _ as isize));
    }
    state.found
}
