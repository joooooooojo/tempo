//! OS file-list clipboard (CF_HDROP / NSFilenamesPboardType). Paths only — no file bytes.

use std::path::{Path, PathBuf};

pub fn serialize_clipboard_paths(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let as_strings: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .collect();
    if as_strings.is_empty() {
        return None;
    }
    serde_json::to_string(&as_strings).ok()
}

pub fn parse_clipboard_paths(content: &str) -> Result<Vec<PathBuf>, String> {
    let values: Vec<String> =
        serde_json::from_str(content).map_err(|error| format!("invalid file clipboard entry: {error}"))?;
    let paths: Vec<PathBuf> = values
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .collect();
    if paths.is_empty() {
        return Err("file clipboard entry has no paths".into());
    }
    Ok(paths)
}

pub fn ensure_clipboard_paths_exist(paths: &[PathBuf]) -> Result<(), String> {
    let missing: Vec<&PathBuf> = paths.iter().filter(|path| !path.exists()).collect();
    if missing.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = missing
        .iter()
        .map(|path| display_path_name(path))
        .collect();
    Err(format!("文件不存在: {}", names.join(", ")))
}

pub fn display_path_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn format_clipboard_files_preview(paths: &[PathBuf]) -> String {
    match paths {
        [] => String::new(),
        [only] => display_path_name(only),
        [first, ..] => format!("{} 等 {} 项", display_path_name(first), paths.len()),
    }
}

#[cfg(target_os = "windows")]
pub fn read_clipboard_file_paths() -> Result<Option<Vec<PathBuf>>, FileClipboardError> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    const CF_HDROP: u32 = 15;

    unsafe {
        if IsClipboardFormatAvailable(CF_HDROP).is_err() {
            return Ok(None);
        }
        if OpenClipboard(HWND::default()).is_err() {
            return Err(FileClipboardError::Busy);
        }
        let result = (|| {
            let handle = GetClipboardData(CF_HDROP).map_err(|_| FileClipboardError::Unavailable)?;
            let hdrop = HDROP(handle.0);
            let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
            if count == 0 {
                return Ok(None);
            }
            let mut paths = Vec::with_capacity(count as usize);
            for index in 0..count {
                let len = DragQueryFileW(hdrop, index, None);
                if len == 0 {
                    continue;
                }
                let mut buffer = vec![0u16; len as usize + 1];
                let written = DragQueryFileW(hdrop, index, Some(&mut buffer));
                if written == 0 {
                    continue;
                }
                let path = String::from_utf16_lossy(&buffer[..written as usize]);
                if !path.is_empty() {
                    paths.push(PathBuf::from(path));
                }
            }
            Ok(if paths.is_empty() { None } else { Some(paths) })
        })();
        let _ = CloseClipboard();
        result
    }
}

#[cfg(target_os = "windows")]
pub fn write_clipboard_file_paths(paths: &[PathBuf]) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{FALSE, HWND, POINT, TRUE};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows::Win32::UI::Shell::DROPFILES;

    const CF_HDROP: u32 = 15;

    if paths.is_empty() {
        return Err("no file paths to write".into());
    }

    let mut path_bytes: Vec<u16> = Vec::new();
    for path in paths {
        path_bytes.extend(OsStr::new(path).encode_wide());
        path_bytes.push(0);
    }
    path_bytes.push(0);

    let header_size = std::mem::size_of::<DROPFILES>();
    let total_size = header_size + path_bytes.len() * std::mem::size_of::<u16>();

    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, total_size).map_err(|error| error.to_string())?;
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            return Err("failed to lock CF_HDROP memory".into());
        }

        let dropfiles = DROPFILES {
            pFiles: header_size as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: FALSE,
            fWide: TRUE,
        };
        std::ptr::write(ptr.cast::<DROPFILES>(), dropfiles);
        std::ptr::copy_nonoverlapping(
            path_bytes.as_ptr(),
            ptr.cast::<u8>().add(header_size).cast::<u16>(),
            path_bytes.len(),
        );
        let _ = GlobalUnlock(handle);

        OpenClipboard(HWND::default()).map_err(|error| error.to_string())?;
        let write_result = (|| {
            EmptyClipboard().map_err(|error| error.to_string())?;
            SetClipboardData(CF_HDROP, windows::Win32::Foundation::HANDLE(handle.0))
                .map_err(|error| error.to_string())?;
            Ok(())
        })();
        let _ = CloseClipboard();
        write_result
    }
}

#[cfg(target_os = "macos")]
pub fn read_clipboard_file_paths() -> Result<Option<Vec<PathBuf>>, FileClipboardError> {
    use std::ffi::CStr;
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let pasteboard_class = Class::get("NSPasteboard").ok_or(FileClipboardError::Unavailable)?;
        let pasteboard: *mut Object = msg_send![pasteboard_class, generalPasteboard];
        if pasteboard.is_null() {
            return Err(FileClipboardError::Unavailable);
        }

        let type_name = match ns_string("NSFilenamesPboardType") {
            Some(value) => value,
            None => return Err(FileClipboardError::Unavailable),
        };
        let filenames: *mut Object = msg_send![pasteboard, propertyListForType: type_name];
        if filenames.is_null() {
            return Ok(None);
        }

        let count: usize = msg_send![filenames, count];
        if count == 0 {
            return Ok(None);
        }

        let mut paths = Vec::with_capacity(count);
        for index in 0..count {
            let item: *mut Object = msg_send![filenames, objectAtIndex: index];
            if item.is_null() {
                continue;
            }
            let utf8: *const std::os::raw::c_char = msg_send![item, UTF8String];
            if utf8.is_null() {
                continue;
            }
            let path = CStr::from_ptr(utf8).to_string_lossy().into_owned();
            if !path.is_empty() {
                paths.push(PathBuf::from(path));
            }
        }

        Ok(if paths.is_empty() { None } else { Some(paths) })
    }
}

#[cfg(target_os = "macos")]
pub fn write_clipboard_file_paths(paths: &[PathBuf]) -> Result<(), String> {
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    if paths.is_empty() {
        return Err("no file paths to write".into());
    }

    unsafe {
        let pasteboard_class =
            Class::get("NSPasteboard").ok_or_else(|| "NSPasteboard unavailable".to_string())?;
        let pasteboard: *mut Object = msg_send![pasteboard_class, generalPasteboard];
        if pasteboard.is_null() {
            return Err("NSPasteboard unavailable".into());
        }

        let array_class =
            Class::get("NSMutableArray").ok_or_else(|| "NSMutableArray unavailable".to_string())?;
        let array: *mut Object = msg_send![array_class, arrayWithCapacity: paths.len()];
        if array.is_null() {
            return Err("failed to allocate path array".into());
        }

        for path in paths {
            let Some(ns_path) = ns_string(&path.to_string_lossy()) else {
                continue;
            };
            let _: () = msg_send![array, addObject: ns_path];
        }

        let count: usize = msg_send![array, count];
        if count == 0 {
            return Err("no valid file paths to write".into());
        }

        let type_name =
            ns_string("NSFilenamesPboardType").ok_or_else(|| "NSString unavailable".to_string())?;
        let _: () = msg_send![pasteboard, clearContents];
        let ok: bool = msg_send![pasteboard, setPropertyList: array forType: type_name];
        if !ok {
            return Err("failed to write file paths to pasteboard".into());
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn read_clipboard_file_paths() -> Result<Option<Vec<PathBuf>>, FileClipboardError> {
    Ok(None)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn write_clipboard_file_paths(_paths: &[PathBuf]) -> Result<(), String> {
    Err("file clipboard is not supported on this platform".into())
}

#[derive(Debug)]
pub enum FileClipboardError {
    Busy,
    Unavailable,
}

#[cfg(target_os = "macos")]
unsafe fn ns_string(value: &str) -> Option<*mut objc::runtime::Object> {
    use std::ffi::CString;
    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    let class = Class::get("NSString")?;
    let c_string = CString::new(value).ok()?;
    let object: *mut Object = msg_send![class, stringWithUTF8String: c_string.as_ptr()];
    if object.is_null() {
        None
    } else {
        Some(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_and_parse_roundtrip() {
        let paths = vec![
            PathBuf::from("/tmp/a.pdf"),
            PathBuf::from("/tmp/nested/b.docx"),
        ];
        let json = serialize_clipboard_paths(&paths).expect("serialize");
        let parsed = parse_clipboard_paths(&json).expect("parse");
        assert_eq!(parsed, paths);
    }

    #[test]
    fn serialize_rejects_empty() {
        assert!(serialize_clipboard_paths(&[]).is_none());
    }

    #[test]
    fn ensure_missing_paths_reports_names() {
        let missing = PathBuf::from("/definitely/missing/tempo-clipboard-test-file.bin");
        let error = ensure_clipboard_paths_exist(&[missing]).expect_err("should miss");
        assert!(error.contains("文件不存在"));
        assert!(error.contains("tempo-clipboard-test-file.bin"));
    }

    #[test]
    fn preview_formats_single_and_multi() {
        assert_eq!(
            format_clipboard_files_preview(&[PathBuf::from("/tmp/a.pdf")]),
            "a.pdf"
        );
        assert_eq!(
            format_clipboard_files_preview(&[
                PathBuf::from("/tmp/a.pdf"),
                PathBuf::from("/tmp/b.pdf")
            ]),
            "a.pdf 等 2 项"
        );
    }
}
