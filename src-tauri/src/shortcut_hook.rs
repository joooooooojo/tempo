//! Windows low-level keyboard hook guard for global shortcuts.
//!
//! `RegisterHotKey` alone loses to apps like uTools that install `WH_KEYBOARD_LL`
//! and swallow chords earlier in the hook chain. This module mirrors Tempo's
//! bindings in an LL hook, consumes matches, and periodically reinstalls so we
//! stay near the front of the chain.

use crate::logging;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_0, VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9,
    VK_A, VK_ADD, VK_B, VK_BACK, VK_C, VK_CAPITAL, VK_CONTROL, VK_D, VK_DECIMAL, VK_DELETE,
    VK_DIVIDE, VK_DOWN, VK_E, VK_END, VK_ESCAPE, VK_F, VK_F1, VK_F10, VK_F11, VK_F12, VK_F13,
    VK_F14, VK_F15, VK_F16, VK_F17, VK_F18, VK_F19, VK_F2, VK_F20, VK_F21, VK_F22, VK_F23, VK_F24,
    VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_G, VK_H, VK_HOME, VK_I, VK_INSERT, VK_J,
    VK_K, VK_L, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_M, VK_MEDIA_NEXT_TRACK,
    VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_MENU, VK_MULTIPLY, VK_N,
    VK_NEXT, VK_NUMLOCK, VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD3, VK_NUMPAD4, VK_NUMPAD5,
    VK_NUMPAD6, VK_NUMPAD7, VK_NUMPAD8, VK_NUMPAD9, VK_O, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4,
    VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_P,
    VK_PAUSE, VK_PLAY, VK_PRIOR, VK_Q, VK_R, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_S, VK_SCROLL, VK_SHIFT, VK_SNAPSHOT, VK_SPACE, VK_SUBTRACT, VK_T, VK_TAB, VK_U, VK_UP,
    VK_V, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP, VK_W, VK_X, VK_Y, VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, MsgWaitForMultipleObjects, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, DispatchMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, PM_REMOVE,
    QS_ALLINPUT, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

const MOD_CTRL: u16 = 0b0001;
const MOD_ALT: u16 = 0b0010;
const MOD_SHIFT: u16 = 0b0100;
const MOD_WIN: u16 = 0b1000;

/// Ask the hook thread to reinstall immediately (must only be handled there).
const WM_TEMPO_REINSTALL_HOOK: u32 = WM_APP + 42;

const REINSTALL_INTERVAL: Duration = Duration::from_millis(800);
/// Only needs to collapse RegisterHotKey + LL-hook double delivery for the *same*
/// physical press (a few ms apart). A longer window ate intentional reopen after
/// mouse blur-hide (Alt+Space → click away → Alt+Space within ~280ms).
const DISPATCH_DEBOUNCE: Duration = Duration::from_millis(80);

type ChordKey = (u16, u32); // (mods, vk)

struct HookShared {
    bindings: HashMap<ChordKey, &'static str>,
    app: Option<AppHandle>,
    last_dispatch: HashMap<&'static str, Instant>,
    hook_thread_id: Option<u32>,
    /// Raw HHOOK pointer stored as isize so HookShared stays Send.
    hook: isize,
}

static SHARED: OnceLock<Mutex<HookShared>> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);

fn shared() -> &'static Mutex<HookShared> {
    SHARED.get_or_init(|| {
        Mutex::new(HookShared {
            bindings: HashMap::new(),
            app: None,
            last_dispatch: HashMap::new(),
            hook_thread_id: None,
            hook: 0,
        })
    })
}

/// Start the LL-hook thread once. Safe to call repeatedly.
pub fn start(app: AppHandle) {
    {
        let mut state = shared().lock();
        state.app = Some(app);
    }
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    logging::spawn_named("tempo-shortcut-ll-hook", hook_thread_main);
}

/// Replace watched chords. Pass `(shortcut_string, action_id)` pairs.
pub fn sync_bindings(bindings: &[(String, &'static str)]) {
    let mut next = HashMap::new();
    for (raw, action) in bindings {
        match parse_chord(raw) {
            Ok(key) => {
                next.insert(key, *action);
            }
            Err(error) => {
                tracing::debug!(shortcut = %raw, error = %error, "skip hook binding");
            }
        }
    }
    {
        let mut state = shared().lock();
        state.bindings = next;
    }
    request_reinstall();
}

/// Ask the hook thread to reinstall at the front of the LL chain (e.g. after
/// main-panel blur-hide, when competing tools often reinstall their own hooks).
pub fn request_reinstall() {
    let thread_id = shared().lock().hook_thread_id;
    if let Some(thread_id) = thread_id {
        let _ = unsafe {
            PostThreadMessageW(
                thread_id,
                WM_TEMPO_REINSTALL_HOOK,
                WPARAM(0),
                LPARAM(0),
            )
        };
    }
}

/// Returns true when this press should run the action (debounce across hook + RegisterHotKey).
pub fn claim_dispatch(action: &'static str) -> bool {
    let mut state = shared().lock();
    let now = Instant::now();
    if let Some(last) = state.last_dispatch.get(action) {
        if now.duration_since(*last) < DISPATCH_DEBOUNCE {
            return false;
        }
    }
    state.last_dispatch.insert(action, now);
    true
}

fn hook_thread_main() {
    let thread_id = unsafe { GetCurrentThreadId() };
    shared().lock().hook_thread_id = Some(thread_id);

    if let Err(error) = install_hook() {
        tracing::warn!(error = %error, "failed to install shortcut LL hook");
    } else {
        tracing::info!("shortcut LL hook installed (guards against uTools-style hooks)");
    }

    let mut last_reinstall = Instant::now();
    let mut msg = MSG::default();

    loop {
        let _ = unsafe { MsgWaitForMultipleObjects(None, false, 100, QS_ALLINPUT) };

        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    let _ = uninstall_hook();
                    return;
                }
                if msg.message == WM_TEMPO_REINSTALL_HOOK {
                    if let Err(error) = reinstall_hook() {
                        tracing::debug!(error = %error, "shortcut LL hook reinstall failed");
                    }
                    last_reinstall = Instant::now();
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        if last_reinstall.elapsed() >= REINSTALL_INTERVAL {
            if let Err(error) = reinstall_hook() {
                tracing::debug!(error = %error, "shortcut LL hook reinstall failed");
            }
            last_reinstall = Instant::now();
        }
    }
}

fn install_hook() -> Result<(), String> {
    let module = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
    let hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            HINSTANCE(module.0),
            0,
        )
    }
    .map_err(|e| e.to_string())?;
    shared().lock().hook = hook.0 as isize;
    Ok(())
}

fn uninstall_hook() -> Result<(), String> {
    let hook_ptr = {
        let mut state = shared().lock();
        let hook_ptr = state.hook;
        state.hook = 0;
        hook_ptr
    };
    if hook_ptr == 0 {
        return Ok(());
    }
    let hook = HHOOK(hook_ptr as *mut _);
    unsafe { UnhookWindowsHookEx(hook) }.map_err(|e| e.to_string())
}

fn reinstall_hook() -> Result<(), String> {
    // Install first so we briefly hold two hooks, then drop the old one —
    // keeps us at the front of the chain more often than uninstall-first.
    let previous_ptr = shared().lock().hook;
    let module = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
    let hook = unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(low_level_keyboard_proc),
            HINSTANCE(module.0),
            0,
        )
    }
    .map_err(|e| e.to_string())?;
    shared().lock().hook = hook.0 as isize;
    if previous_ptr != 0 && previous_ptr != hook.0 as isize {
        let _ = unsafe { UnhookWindowsHookEx(HHOOK(previous_ptr as *mut _)) };
    }
    Ok(())
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let msg = wparam.0 as u32;
    if msg != WM_KEYDOWN && msg != WM_SYSKEYDOWN {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let info = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let vk = info.vkCode;
    if is_modifier_vk(vk) {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let mods = current_mods();
    let action = {
        let state = shared().lock();
        state.bindings.get(&(mods, vk)).copied()
    };

    let Some(action) = action else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };

    if !claim_dispatch(action) {
        // Still swallow repeats so competing hooks do not see auto-repeat.
        return LRESULT(1);
    }

    let app = shared().lock().app.clone();
    if let Some(app) = app {
        let app_for_main = app.clone();
        logging::spawn_named("tempo-shortcut-ll-dispatch", move || {
            let _ = app.run_on_main_thread(move || {
                if let Err(error) = crate::dispatch_shortcut_action(&app_for_main, action) {
                    tracing::warn!(action, error = %error, "LL hook shortcut action failed");
                }
            });
        });
    }

    // Swallow so uTools / other LL hooks further down the chain never see it.
    LRESULT(1)
}

fn current_mods() -> u16 {
    let mut mods = 0u16;
    if key_down(VK_CONTROL) {
        mods |= MOD_CTRL;
    }
    if key_down(VK_MENU) || key_down(VK_LMENU) || key_down(VK_RMENU) {
        mods |= MOD_ALT;
    }
    if key_down(VK_SHIFT) || key_down(VK_LSHIFT) || key_down(VK_RSHIFT) {
        mods |= MOD_SHIFT;
    }
    if key_down(VK_LWIN) || key_down(VK_RWIN) {
        mods |= MOD_WIN;
    }
    mods
}

fn key_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 }
}

fn is_modifier_vk(vk: u32) -> bool {
    vk == VK_CONTROL.0 as u32
        || vk == VK_MENU.0 as u32
        || vk == VK_LMENU.0 as u32
        || vk == VK_RMENU.0 as u32
        || vk == VK_SHIFT.0 as u32
        || vk == VK_LSHIFT.0 as u32
        || vk == VK_RSHIFT.0 as u32
        || vk == VK_LWIN.0 as u32
        || vk == VK_RWIN.0 as u32
}

fn parse_chord(raw: &str) -> Result<ChordKey, String> {
    let shortcut = Shortcut::from_str(raw.trim()).map_err(|e| e.to_string())?;
    let vk = code_to_vk(shortcut.key).ok_or_else(|| format!("unsupported key: {:?}", shortcut.key))?;
    Ok((modifiers_mask(shortcut.mods), vk.0 as u32))
}

fn modifiers_mask(mods: Modifiers) -> u16 {
    let mut mask = 0u16;
    if mods.contains(Modifiers::CONTROL) {
        mask |= MOD_CTRL;
    }
    if mods.contains(Modifiers::ALT) {
        mask |= MOD_ALT;
    }
    if mods.contains(Modifiers::SHIFT) {
        mask |= MOD_SHIFT;
    }
    if mods.intersects(Modifiers::SUPER | Modifiers::META) {
        mask |= MOD_WIN;
    }
    mask
}

fn code_to_vk(key: Code) -> Option<VIRTUAL_KEY> {
    Some(match key {
        Code::KeyA => VK_A,
        Code::KeyB => VK_B,
        Code::KeyC => VK_C,
        Code::KeyD => VK_D,
        Code::KeyE => VK_E,
        Code::KeyF => VK_F,
        Code::KeyG => VK_G,
        Code::KeyH => VK_H,
        Code::KeyI => VK_I,
        Code::KeyJ => VK_J,
        Code::KeyK => VK_K,
        Code::KeyL => VK_L,
        Code::KeyM => VK_M,
        Code::KeyN => VK_N,
        Code::KeyO => VK_O,
        Code::KeyP => VK_P,
        Code::KeyQ => VK_Q,
        Code::KeyR => VK_R,
        Code::KeyS => VK_S,
        Code::KeyT => VK_T,
        Code::KeyU => VK_U,
        Code::KeyV => VK_V,
        Code::KeyW => VK_W,
        Code::KeyX => VK_X,
        Code::KeyY => VK_Y,
        Code::KeyZ => VK_Z,
        Code::Digit0 => VK_0,
        Code::Digit1 => VK_1,
        Code::Digit2 => VK_2,
        Code::Digit3 => VK_3,
        Code::Digit4 => VK_4,
        Code::Digit5 => VK_5,
        Code::Digit6 => VK_6,
        Code::Digit7 => VK_7,
        Code::Digit8 => VK_8,
        Code::Digit9 => VK_9,
        Code::Equal => VK_OEM_PLUS,
        Code::Comma => VK_OEM_COMMA,
        Code::Minus => VK_OEM_MINUS,
        Code::Period => VK_OEM_PERIOD,
        Code::Semicolon => VK_OEM_1,
        Code::Slash => VK_OEM_2,
        Code::Backquote => VK_OEM_3,
        Code::BracketLeft => VK_OEM_4,
        Code::Backslash => VK_OEM_5,
        Code::BracketRight => VK_OEM_6,
        Code::Quote => VK_OEM_7,
        Code::Backspace => VK_BACK,
        Code::Tab => VK_TAB,
        Code::Space => VK_SPACE,
        Code::Enter => VK_RETURN,
        Code::CapsLock => VK_CAPITAL,
        Code::Escape => VK_ESCAPE,
        Code::PageUp => VK_PRIOR,
        Code::PageDown => VK_NEXT,
        Code::End => VK_END,
        Code::Home => VK_HOME,
        Code::ArrowLeft => VK_LEFT,
        Code::ArrowUp => VK_UP,
        Code::ArrowRight => VK_RIGHT,
        Code::ArrowDown => VK_DOWN,
        Code::PrintScreen => VK_SNAPSHOT,
        Code::Insert => VK_INSERT,
        Code::Delete => VK_DELETE,
        Code::F1 => VK_F1,
        Code::F2 => VK_F2,
        Code::F3 => VK_F3,
        Code::F4 => VK_F4,
        Code::F5 => VK_F5,
        Code::F6 => VK_F6,
        Code::F7 => VK_F7,
        Code::F8 => VK_F8,
        Code::F9 => VK_F9,
        Code::F10 => VK_F10,
        Code::F11 => VK_F11,
        Code::F12 => VK_F12,
        Code::F13 => VK_F13,
        Code::F14 => VK_F14,
        Code::F15 => VK_F15,
        Code::F16 => VK_F16,
        Code::F17 => VK_F17,
        Code::F18 => VK_F18,
        Code::F19 => VK_F19,
        Code::F20 => VK_F20,
        Code::F21 => VK_F21,
        Code::F22 => VK_F22,
        Code::F23 => VK_F23,
        Code::F24 => VK_F24,
        Code::NumLock => VK_NUMLOCK,
        Code::Numpad0 => VK_NUMPAD0,
        Code::Numpad1 => VK_NUMPAD1,
        Code::Numpad2 => VK_NUMPAD2,
        Code::Numpad3 => VK_NUMPAD3,
        Code::Numpad4 => VK_NUMPAD4,
        Code::Numpad5 => VK_NUMPAD5,
        Code::Numpad6 => VK_NUMPAD6,
        Code::Numpad7 => VK_NUMPAD7,
        Code::Numpad8 => VK_NUMPAD8,
        Code::Numpad9 => VK_NUMPAD9,
        Code::NumpadAdd => VK_ADD,
        Code::NumpadDecimal => VK_DECIMAL,
        Code::NumpadDivide => VK_DIVIDE,
        Code::NumpadEnter => VK_RETURN,
        Code::NumpadMultiply => VK_MULTIPLY,
        Code::NumpadSubtract => VK_SUBTRACT,
        Code::ScrollLock => VK_SCROLL,
        Code::AudioVolumeDown => VK_VOLUME_DOWN,
        Code::AudioVolumeUp => VK_VOLUME_UP,
        Code::AudioVolumeMute => VK_VOLUME_MUTE,
        Code::MediaPlay => VK_PLAY,
        Code::MediaPause => VK_PAUSE,
        Code::MediaPlayPause => VK_MEDIA_PLAY_PAUSE,
        Code::MediaStop => VK_MEDIA_STOP,
        Code::MediaTrackNext => VK_MEDIA_NEXT_TRACK,
        Code::MediaTrackPrevious => VK_MEDIA_PREV_TRACK,
        Code::Pause => VK_PAUSE,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{modifiers_mask, parse_chord, MOD_ALT, MOD_CTRL, MOD_SHIFT};
    use tauri_plugin_global_shortcut::Modifiers;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_SPACE;

    #[test]
    fn parses_alt_space() {
        let (mods, vk) = parse_chord("Alt+Space").expect("parse");
        assert_eq!(mods, MOD_ALT);
        assert_eq!(vk, VK_SPACE.0 as u32);
    }

    #[test]
    fn parses_control_shift_v() {
        let (mods, vk) = parse_chord("Control+Shift+V").expect("parse");
        assert_eq!(mods, MOD_CTRL | MOD_SHIFT);
        assert_eq!(vk, windows::Win32::UI::Input::KeyboardAndMouse::VK_V.0 as u32);
    }

    #[test]
    fn modifiers_mask_matches_bits() {
        assert_eq!(
            modifiers_mask(Modifiers::CONTROL | Modifiers::ALT),
            MOD_CTRL | MOD_ALT
        );
    }
}
