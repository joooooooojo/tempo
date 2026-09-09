//! Shared plugin icon contract used by manifest validation and every icon-serving surface.

use std::path::Path;

use base64::Engine as _;

pub const MAX_ICON_BYTES: u64 = 256 * 1024;
pub const SUPPORTED_ICON_FORMATS: &str = "SVG, PNG, JPEG, WebP, GIF";

pub fn mime_type_for_path(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

pub fn data_url_from_package_file(package_root: &Path, rel_path: &str) -> Option<String> {
    let canonical_root = package_root.canonicalize().ok()?;
    let canonical_path = package_root.join(rel_path).canonicalize().ok()?;
    if !canonical_path.starts_with(&canonical_root) || !canonical_path.is_file() {
        return None;
    }
    let mime = mime_type_for_path(&canonical_path)?;
    if canonical_path.metadata().ok()?.len() > MAX_ICON_BYTES {
        return None;
    }
    let bytes = std::fs::read(&canonical_path).ok()?;
    if bytes.len() as u64 > MAX_ICON_BYTES {
        return None;
    }
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_all_supported_icon_extensions_case_insensitively() {
        for (path, mime) in [
            ("icon.svg", "image/svg+xml"),
            ("icon.PNG", "image/png"),
            ("icon.jpg", "image/jpeg"),
            ("icon.JPEG", "image/jpeg"),
            ("icon.webp", "image/webp"),
            ("icon.GIF", "image/gif"),
        ] {
            assert_eq!(mime_type_for_path(Path::new(path)), Some(mime));
        }
    }

    #[test]
    fn rejects_formats_outside_the_manifest_contract() {
        for path in ["icon.bmp", "icon.ico", "icon.icns", "icon.avif", "icon"] {
            assert_eq!(mime_type_for_path(Path::new(path)), None);
        }
    }

    #[test]
    fn package_icon_data_urls_use_the_declared_format_mime() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tempo-plugin-icon-data-{unique}"));
        let icons = root.join("icons");
        std::fs::create_dir_all(&icons).unwrap();
        std::fs::write(icons.join("app.WebP"), [1, 2, 3]).unwrap();

        let data_url = data_url_from_package_file(&root, "icons/app.WebP").unwrap();
        let _ = std::fs::remove_dir_all(root);
        assert_eq!(data_url, "data:image/webp;base64,AQID");
    }
}
