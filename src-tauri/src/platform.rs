//! 检测用户是否真正在使用电脑，避免后台空转虚增时长。

use std::path::Path;
#[cfg(any(windows, target_os = "macos"))]
use std::path::PathBuf;

/// 空闲超过此时间（秒）则暂停计时（看视频/阅读通常仍有偶尔输入）
const IDLE_THRESHOLD_SECS: u32 = 180;

#[derive(Debug, Clone)]
pub struct ForegroundApp {
    pub name: String,
    pub process_name: String,
}

#[cfg(windows)]
pub struct IconExtractionThreadContext {
    initialized: bool,
}

#[cfg(windows)]
impl Drop for IconExtractionThreadContext {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
    }
}

#[cfg(windows)]
pub fn icon_extraction_thread_context() -> IconExtractionThreadContext {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    IconExtractionThreadContext { initialized }
}

#[cfg(not(windows))]
pub struct IconExtractionThreadContext;

#[cfg(not(windows))]
pub fn icon_extraction_thread_context() -> IconExtractionThreadContext {
    IconExtractionThreadContext
}

pub fn extract_icon_png_bytes(app_name: &str, process_name: &str) -> Option<Vec<u8>> {
    let candidates = icon_path_candidates(app_name, process_name);
    let candidate_count = candidates.len();
    for candidate in candidates {
        if let Some(bytes) = get_cached_icon_png_bytes(&candidate) {
            return Some(bytes);
        }
    }
    if candidate_count > 0 {
        tracing::debug!(
            candidate_count = candidate_count,
            "failed to extract app icon from candidate paths"
        );
    }
    None
}

#[cfg(windows)]
pub fn is_session_locked() -> bool {
    use windows::Win32::System::StationsAndDesktops::{
        CloseDesktop, OpenInputDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS,
    };

    unsafe {
        match OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) {
            Ok(handle) => {
                let _ = CloseDesktop(handle);
                false
            }
            Err(_) => true,
        }
    }
}

#[cfg(not(windows))]
pub fn is_session_locked() -> bool {
    false
}

#[cfg(windows)]
pub fn idle_seconds() -> u32 {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if !GetLastInputInfo(&mut info).as_bool() {
            return 0;
        }
        let now = GetTickCount();
        now.wrapping_sub(info.dwTime) / 1000
    }
}

#[cfg(target_os = "macos")]
pub fn idle_seconds() -> u32 {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::kCFAllocatorDefault;
    use core_foundation_sys::dictionary::CFDictionaryRef;
    use core_foundation_sys::string::CFStringRef;
    use std::ffi::c_void;

    type IoIterator = u32;
    type IoObject = u32;
    type IoRegistryEntry = u32;

    const K_IO_REGISTRY_ITERATE_RECURSIVELY: u32 = 0x0000_0001;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOServiceGetMatchingServices(
            main_port: u32,
            matching: CFDictionaryRef,
            existing: *mut IoIterator,
        ) -> i32;
        fn IOServiceMatching(name: *const i8) -> CFDictionaryRef;
        fn IOIteratorNext(iterator: IoIterator) -> IoObject;
        fn IORegistryEntryCreateCFProperty(
            entry: IoRegistryEntry,
            key: CFStringRef,
            allocator: *const c_void,
            options: u32,
        ) -> *const c_void;
        fn IOObjectRelease(object: IoObject) -> i32;
    }

    unsafe {
        let matching = IOServiceMatching(b"IOHIDSystem\0".as_ptr() as *const i8);
        if matching.is_null() {
            return 0;
        }

        let mut iterator = 0;
        if IOServiceGetMatchingServices(0, matching, &mut iterator) != 0 {
            return 0;
        }

        let entry = IOIteratorNext(iterator);
        let _ = IOObjectRelease(iterator);
        if entry == 0 {
            return 0;
        }

        let key = CFString::new("HIDIdleTime");
        let value = IORegistryEntryCreateCFProperty(
            entry,
            key.as_concrete_TypeRef(),
            kCFAllocatorDefault,
            K_IO_REGISTRY_ITERATE_RECURSIVELY,
        );
        let _ = IOObjectRelease(entry);

        if value.is_null() {
            return 0;
        }

        let value = CFType::wrap_under_create_rule(value as _);
        let Some(nanos) = value
            .downcast::<CFNumber>()
            .and_then(|number| number.to_i64())
        else {
            return 0;
        };

        (nanos / 1_000_000_000).clamp(0, u32::MAX as i64) as u32
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn idle_seconds() -> u32 {
    0
}

/// 是否应继续累计屏幕/应用时长
pub fn should_count_time() -> bool {
    if is_session_locked() {
        return false;
    }
    if idle_seconds() >= IDLE_THRESHOLD_SECS {
        return false;
    }
    true
}

pub fn get_foreground_app() -> Option<ForegroundApp> {
    if is_session_locked() {
        return None;
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(app) = get_foreground_app_macos() {
            return Some(app);
        }
    }

    match active_win_pos_rs::get_active_window() {
        Ok(win) => {
            let name = win.app_name.trim().to_string();
            #[cfg(windows)]
            let process = {
                let path = win.process_path.to_string_lossy().trim().to_string();
                if path.is_empty() {
                    win.process_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                } else {
                    path
                }
            };
            #[cfg(not(windows))]
            let process = win
                .process_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if name.is_empty() {
                return None;
            }

            if is_ignored_foreground_app(&name, &process, None) {
                return None;
            }

            Some(ForegroundApp {
                name,
                process_name: process,
            })
        }
        Err(error) => {
            tracing::debug!(error = ?error, "failed to read active foreground window");
            None
        }
    }
}

fn is_ignored_foreground_app(name: &str, process: &str, _bundle_id: Option<&str>) -> bool {
    let lower = format!("{} {}", name, process).to_lowercase();
    lower.contains("lockapp")
        || lower.contains("lock screen")
        || lower.contains("screen saver")
        || lower.contains("screensaver")
}

#[cfg(target_os = "macos")]
fn get_foreground_app_macos() -> Option<ForegroundApp> {
    use appkit_nsworkspace_bindings::{INSRunningApplication, INSWorkspace, NSWorkspace, INSURL};
    use std::path::PathBuf;

    unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication();
        if app.0.is_null() {
            return None;
        }

        let bundle_id = nsstring_to_rust_string(app.bundleIdentifier().0);
        let localized_name = nsstring_to_rust_string(app.localizedName().0);
        let bundle_url = app.bundleURL();
        let executable_url = app.executableURL();
        if bundle_url.0.is_null() && executable_url.0.is_null() && localized_name.is_empty() {
            return None;
        }

        let bundle_path_str = if bundle_url.0.is_null() {
            String::new()
        } else {
            nsstring_to_rust_string(bundle_url.path().0)
        };
        let bundle_path = PathBuf::from(bundle_path_str.trim());
        let executable_path = if executable_url.0.is_null() {
            String::new()
        } else {
            nsstring_to_rust_string(executable_url.path().0)
        };
        let process_name = std::path::Path::new(executable_path.trim())
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        let display_name = if !localized_name.trim().is_empty() {
            localized_name.trim().to_string()
        } else if bundle_path.extension().is_some_and(|ext| ext == "app") {
            resolve_macos_display_name(&bundle_path).or_else(|| {
                bundle_path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
            })?
        } else {
            return None;
        };

        if is_ignored_foreground_app(
            &display_name,
            &process_name,
            (!bundle_id.is_empty()).then_some(bundle_id.as_str()),
        ) {
            return None;
        }

        Some(ForegroundApp {
            name: display_name,
            process_name,
        })
    }
}

#[cfg(target_os = "macos")]
fn nsstring_to_rust_string(nsstring: *mut objc::runtime::Object) -> String {
    if nsstring.is_null() {
        return String::new();
    }

    unsafe {
        let cstr: *const i8 = msg_send![nsstring, UTF8String];
        if cstr.is_null() {
            return String::new();
        }

        std::ffi::CStr::from_ptr(cstr)
            .to_string_lossy()
            .into_owned()
    }
}

/// 有前台应用或用户近期有输入时才计屏幕时长
pub fn should_count_tempo_time(foreground: &Option<ForegroundApp>) -> bool {
    if !should_count_time() {
        return false;
    }
    // 有明确的前台应用 → 计入
    if foreground.is_some() {
        return true;
    }
    // 无前台窗口但用户近期有操作（60秒内）→ 仍计入（桌面/全屏场景）
    idle_seconds() < 60
}

#[cfg(any(windows, target_os = "macos"))]
fn get_cached_icon_png_bytes(path: &Path) -> Option<Vec<u8>> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static ICON_CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<Vec<u8>>>>> = OnceLock::new();

    let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(icons) = cache.lock() {
        if let Some(icon) = icons.get(path) {
            return icon.clone();
        }
    }

    let icon = extract_icon_png_bytes_from_path(path);
    if let Ok(mut icons) = cache.lock() {
        icons.insert(path.to_path_buf(), icon.clone());
    }
    icon
}

#[cfg(not(any(windows, target_os = "macos")))]
fn get_cached_icon_png_bytes(_path: &Path) -> Option<Vec<u8>> {
    None
}

fn push_existing_path_candidate(candidates: &mut Vec<PathBuf>, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }

    let path = PathBuf::from(trimmed);
    if path.exists() {
        candidates.push(path);
    }
}

#[cfg(windows)]
fn icon_path_candidates(app_name: &str, process_name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    push_windows_icon_candidate(&mut candidates, process_name);
    push_windows_icon_candidate(&mut candidates, app_name);

    candidates
}

#[cfg(windows)]
fn push_windows_icon_candidate(candidates: &mut Vec<PathBuf>, value: &str) {
    let trimmed = value.trim();
    if trimmed.to_ascii_lowercase().starts_with("shell:") {
        candidates.push(PathBuf::from(trimmed));
    } else {
        push_existing_path_candidate(candidates, trimmed);
    }
}

#[cfg(target_os = "macos")]
fn icon_path_candidates(app_name: &str, process_name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    push_existing_path_candidate(&mut candidates, process_name);
    push_existing_path_candidate(&mut candidates, app_name);

    let names = [
        app_name.trim().to_string(),
        process_name.trim().trim_end_matches(".app").to_string(),
    ];

    for name in names.into_iter().filter(|name| !name.is_empty()) {
        for root in [
            "/Applications",
            "/System/Applications",
            "/System/Applications/Utilities",
            "/Applications/Utilities",
        ] {
            let candidate = Path::new(root).join(format!("{name}.app"));
            if candidate.exists() {
                candidates.push(candidate);
            }
        }
    }

    candidates
}

#[cfg(not(any(windows, target_os = "macos")))]
fn icon_path_candidates(_app_name: &str, _process_name: &str) -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(windows)]
fn extract_icon_png_bytes_from_path(path: &Path) -> Option<Vec<u8>> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
    };
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHParseDisplayName, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_PIDL,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, DrawIconEx, PrivateExtractIconsW, DI_NORMAL, HICON,
    };

    const ICON_SIZE: i32 = 128;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let is_shell_namespace = path
        .to_string_lossy()
        .to_ascii_lowercase()
        .starts_with("shell:");
    let icon = if is_shell_namespace {
        unsafe { shell_icon_from_display_name(PCWSTR(wide.as_ptr())) }
    } else {
        unsafe { resource_icon_from_path(&wide) }
            .or_else(|| unsafe { shell_icon_from_path(PCWSTR(wide.as_ptr())) })
    }?;

    unsafe {
        let hdc = CreateCompatibleDC(HDC::default());
        if hdc.0.is_null() {
            tracing::debug!("failed to create icon render device context");
            let _ = DestroyIcon(icon);
            return None;
        }

        let mut bitmap_info = BITMAPINFO::default();
        bitmap_info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: ICON_SIZE,
            biHeight: -ICON_SIZE,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let mut bits: *mut c_void = std::ptr::null_mut();
        let bitmap = match CreateDIBSection(
            hdc,
            &bitmap_info,
            DIB_RGB_COLORS,
            &mut bits,
            HANDLE::default(),
            0,
        ) {
            Ok(bitmap) => bitmap,
            Err(error) => {
                tracing::debug!(error = %error, "failed to create icon render bitmap");
                let _ = DeleteDC(hdc);
                let _ = DestroyIcon(icon);
                return None;
            }
        };

        let old_object = SelectObject(hdc, bitmap);
        let drawn = DrawIconEx(hdc, 0, 0, icon, ICON_SIZE, ICON_SIZE, 0, None, DI_NORMAL).is_ok();
        if !drawn {
            tracing::debug!("failed to draw app icon");
        }
        let _ = SelectObject(hdc, old_object);
        let _ = DestroyIcon(icon);

        let png_bytes = if drawn && !bits.is_null() {
            let len = (ICON_SIZE * ICON_SIZE * 4) as usize;
            let bgra = std::slice::from_raw_parts(bits as *const u8, len);
            let has_alpha = bgra.chunks_exact(4).any(|pixel| pixel[3] != 0);
            let mut rgba = Vec::with_capacity(len);

            for pixel in bgra.chunks_exact(4) {
                rgba.push(pixel[2]);
                rgba.push(pixel[1]);
                rgba.push(pixel[0]);
                rgba.push(if has_alpha { pixel[3] } else { 255 });
            }

            encode_png(ICON_SIZE as u32, ICON_SIZE as u32, &rgba)
        } else {
            None
        };

        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(hdc);
        return png_bytes;
    }

    unsafe fn shell_icon_from_path(path: PCWSTR) -> Option<HICON> {
        let mut info = SHFILEINFOW::default();
        let result = SHGetFileInfoW(
            path,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );

        (result != 0 && !info.hIcon.0.is_null()).then_some(info.hIcon)
    }

    unsafe fn shell_icon_from_display_name(path: PCWSTR) -> Option<HICON> {
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        if SHParseDisplayName(path, None::<&IBindCtx>, &mut pidl, 0, None).is_err()
            || pidl.is_null()
        {
            return None;
        }

        let mut info = SHFILEINFOW::default();
        let result = SHGetFileInfoW(
            PCWSTR(pidl.cast()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_PIDL | SHGFI_ICON | SHGFI_LARGEICON,
        );
        CoTaskMemFree(Some(pidl.cast()));
        (result != 0 && !info.hIcon.0.is_null()).then_some(info.hIcon)
    }

    unsafe fn resource_icon_from_path(path: &[u16]) -> Option<HICON> {
        if path.len() > 260 {
            return None;
        }

        let mut fixed = [0u16; 260];
        fixed[..path.len()].copy_from_slice(path);

        let mut icons = [HICON::default(); 1];
        let count =
            PrivateExtractIconsW(&fixed, 0, ICON_SIZE, ICON_SIZE, Some(&mut icons), None, 0);

        (count > 0 && !icons[0].0.is_null()).then_some(icons[0])
    }
}

#[cfg(windows)]
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = match encoder.write_header() {
            Ok(writer) => writer,
            Err(error) => {
                tracing::debug!(
                    width = width,
                    height = height,
                    error = %error,
                    "failed to prepare app icon png encoder"
                );
                return None;
            }
        };
        if let Err(error) = writer.write_image_data(rgba) {
            tracing::debug!(
                width = width,
                height = height,
                bytes = rgba.len(),
                error = %error,
                "failed to encode app icon png"
            );
            return None;
        }
    }
    Some(bytes)
}

#[cfg(target_os = "macos")]
fn extract_icon_png_bytes_from_path(path: &Path) -> Option<Vec<u8>> {
    use std::fs;
    use std::hash::{Hash, Hasher};
    use std::process::Command;

    let app_bundle = find_app_bundle(path)?;
    let icon_path = resolve_macos_icon_path(&app_bundle)?;
    let icon_ext = icon_path
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase();

    if icon_ext == "png" {
        match fs::read(&icon_path) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                tracing::debug!(error = %error, "failed to read macos app icon png");
                None
            }
        }
    } else {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        icon_path.hash(&mut hasher);
        let out = std::env::temp_dir().join(format!(
            "tempo-icon-{}-{}.png",
            std::process::id(),
            hasher.finish()
        ));

        let status = match Command::new("sips")
            .args(["-s", "format", "png", "-Z", "128"])
            .arg(&icon_path)
            .arg("--out")
            .arg(&out)
            .status()
        {
            Ok(status) => status,
            Err(error) => {
                tracing::debug!(error = %error, "failed to run macos icon converter");
                return None;
            }
        };

        if !status.success() {
            if let Err(error) = fs::remove_file(&out) {
                tracing::debug!(error = %error, "failed to remove incomplete macos icon output");
            }
            tracing::debug!(
                status_code = status.code().unwrap_or(-1),
                "macos icon converter exited unsuccessfully"
            );
            return None;
        }

        let bytes = match fs::read(&out) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::debug!(error = %error, "failed to read converted macos app icon");
                return None;
            }
        };
        if let Err(error) = fs::remove_file(&out) {
            tracing::debug!(error = %error, "failed to remove converted macos icon output");
        }
        Some(bytes)
    }
}

#[cfg(target_os = "macos")]
fn find_app_bundle(path: &Path) -> Option<PathBuf> {
    for candidate in path.ancestors() {
        if candidate.extension().is_some_and(|ext| ext == "app") {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn resolve_macos_icon_path(app_bundle: &Path) -> Option<PathBuf> {
    let resources = app_bundle.join("Contents").join("Resources");
    let plist = app_bundle.join("Contents").join("Info.plist");

    if let Some(icon_name) = read_macos_plist_value(&plist, "CFBundleIconFile") {
        for candidate in macos_icon_candidates(&resources, &icon_name) {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    let entries = match std::fs::read_dir(resources) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::debug!(error = %error, "failed to read macos app resources directory");
            return None;
        }
    };

    entries
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry.path()),
            Err(error) => {
                tracing::debug!(error = %error, "failed to read macos app resources entry");
                None
            }
        })
        .find(|path| path.extension().is_some_and(|ext| ext == "icns"))
}

#[cfg(target_os = "macos")]
fn macos_icon_candidates(resources: &Path, icon_name: &str) -> Vec<PathBuf> {
    let name = icon_name.trim();
    let mut candidates = Vec::new();

    if name.is_empty() {
        return candidates;
    }

    let icon_path = Path::new(name);
    if icon_path.is_absolute() {
        candidates.push(icon_path.to_path_buf());
    } else {
        candidates.push(resources.join(icon_path));
    }

    if icon_path.extension().is_none() {
        candidates.push(resources.join(format!("{name}.icns")));
        candidates.push(resources.join(format!("{name}.png")));
    }

    candidates
}

pub fn simulate_paste() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("osascript")
            .arg("-e")
            .arg("tell application \"System Events\" to keystroke \"v\" using command down")
            .status()
            .map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("粘贴失败，请在系统设置中授予辅助功能权限".into())
        }
    }

    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_KEYUP, VIRTUAL_KEY,
        };

        const VK_CONTROL: u16 = 0x11;
        const VK_V: u16 = 0x56;

        fn key_input(vk: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        }

        unsafe {
            let inputs = [
                key_input(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
                key_input(VK_V, KEYBD_EVENT_FLAGS(0)),
                key_input(VK_V, KEYEVENTF_KEYUP),
                key_input(VK_CONTROL, KEYEVENTF_KEYUP),
            ];
            let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            if sent == inputs.len() as u32 {
                Ok(())
            } else {
                Err("粘贴失败".into())
            }
        }
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    {
        Err("当前平台不支持自动粘贴".into())
    }
}

#[cfg(target_os = "macos")]
fn read_macos_plist_value(plist: &Path, key: &str) -> Option<String> {
    let output = match std::process::Command::new("/usr/libexec/PlistBuddy")
        .arg("-c")
        .arg(format!("Print :{key}"))
        .arg(plist)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            tracing::debug!(
                key = %key,
                error = %error,
                "failed to run macos plist reader"
            );
            return None;
        }
    };

    if !output.status.success() {
        return None;
    }

    let value = match String::from_utf8(output.stdout) {
        Ok(value) => value.trim().to_string(),
        Err(error) => {
            tracing::debug!(
                key = %key,
                error = %error,
                "failed to decode macos plist reader output"
            );
            return None;
        }
    };
    (!value.is_empty()).then_some(value)
}

#[cfg(target_os = "macos")]
fn resolve_macos_display_name(app_bundle: &Path) -> Option<String> {
    let plist = app_bundle.join("Contents").join("Info.plist");
    read_macos_plist_value(&plist, "CFBundleDisplayName")
        .or_else(|| read_macos_plist_value(&plist, "CFBundleName"))
}
