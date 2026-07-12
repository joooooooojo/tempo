use crate::asset_protocol::{
    asset_dir, asset_protocol_response, asset_url_for_file_name, storage_key_from_protocol_url,
};
use crate::clipboard_db::hash_bytes;
use base64::Engine as _;
use rusqlite::Connection;
use std::path::Path;
use tauri::http::{Request, Response};
use tauri::AppHandle;

pub const APP_ICON_PROTOCOL: &str = "tempo-app-icon";
pub const APP_ICON_SUBDIR: &str = "app-icons";

pub fn app_icons_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    asset_dir(app, APP_ICON_SUBDIR)
}

pub fn is_app_icon_storage_key(value: &str) -> bool {
    let Some(file_name) = value.strip_prefix(&format!("{APP_ICON_SUBDIR}/")) else {
        return false;
    };
    is_valid_app_icon_file_name(file_name)
}

pub fn is_legacy_app_icon_data_url(value: &str) -> bool {
    value.starts_with("data:image/")
}

pub fn save_app_icon_png(app: &AppHandle, png_bytes: &[u8]) -> Result<String, String> {
    let hash = hash_bytes(png_bytes);
    let file_name = format!("{hash}.png");
    if !is_valid_app_icon_file_name(&file_name) {
        return Err("图标文件名无效".into());
    }

    let dir = app_icons_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&file_name);
    if !path.exists() {
        std::fs::write(&path, png_bytes).map_err(|e| e.to_string())?;
    }

    Ok(format!("{APP_ICON_SUBDIR}/{file_name}"))
}

pub fn hydrate_app_icon_url(value: &str) -> Option<String> {
    if is_app_icon_storage_key(value) {
        let file_name = value.strip_prefix(&format!("{APP_ICON_SUBDIR}/"))?;
        return Some(asset_url_for_file_name(APP_ICON_PROTOCOL, file_name));
    }
    if storage_key_from_protocol_url(APP_ICON_PROTOCOL, APP_ICON_SUBDIR, value).is_some() {
        return Some(value.to_string());
    }
    if is_legacy_app_icon_data_url(value) {
        return Some(value.to_string());
    }
    None
}

pub fn resolve_app_icon_protocol_url(
    app: &AppHandle,
    app_name: &str,
    process_name: &str,
) -> Option<String> {
    let storage_key = resolve_app_icon_storage_key(app, app_name, process_name)?;
    hydrate_app_icon_url(&storage_key)
}

pub fn resolve_app_icon_storage_key(
    app: &AppHandle,
    app_name: &str,
    process_name: &str,
) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static APP_ICON_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let key = format!("{}|{}", app_name.trim(), process_name.trim()).to_lowercase();
    let cache = APP_ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(icons) = cache.lock() {
        if let Some(storage_key) = icons.get(&key) {
            return Some(storage_key.clone());
        }
    }

    let png_bytes = crate::platform::extract_icon_png_bytes(app_name, process_name)?;
    let storage_key = save_app_icon_png(app, &png_bytes).ok()?;
    if let Ok(mut icons) = cache.lock() {
        icons.insert(key, storage_key.clone());
    }
    Some(storage_key)
}

pub fn ensure_app_icon_storage_key(
    app: &AppHandle,
    app_name: &str,
    process_name: &str,
    current: Option<&str>,
) -> Option<String> {
    if let Some(current) = current {
        if is_app_icon_storage_key(current) {
            return Some(current.to_string());
        }
        if let Some(storage_key) = migrate_legacy_app_icon_value(app, current) {
            return Some(storage_key);
        }
    }
    resolve_app_icon_storage_key(app, app_name, process_name)
}

pub fn migrate_legacy_app_icons(app: &AppHandle, conn: &Connection) {
    let mut stmt = match conn.prepare(
        "SELECT date, app_name, process_name, icon_data_url
         FROM app_usage
         WHERE icon_data_url LIKE 'data:image/%'",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return,
    };
    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    }) {
        Ok(rows) => rows.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return,
    };

    for (date, app_name, process_name, icon_data_url) in rows {
        let Some(storage_key) = migrate_legacy_app_icon_value(app, &icon_data_url).or_else(|| {
            resolve_app_icon_storage_key(app, &app_name, &process_name)
        }) else {
            continue;
        };
        let _ = conn.execute(
            "UPDATE app_usage SET icon_data_url = ?1 WHERE date = ?2 AND app_name = ?3",
            rusqlite::params![storage_key, date, app_name],
        );
    }
}

pub fn app_icon_protocol_response(
    app: &AppHandle,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    asset_protocol_response(
        app,
        APP_ICON_SUBDIR,
        |_| "image/png",
        is_valid_app_icon_file_name,
        request,
    )
}

fn migrate_legacy_app_icon_value(app: &AppHandle, value: &str) -> Option<String> {
    let png_bytes = decode_legacy_png_data_url(value)?;
    save_app_icon_png(app, &png_bytes).ok()
}

fn decode_legacy_png_data_url(data_url: &str) -> Option<Vec<u8>> {
    let payload = data_url.strip_prefix("data:image/png;base64,")?;
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .ok()
}

fn is_valid_app_icon_file_name(file_name: &str) -> bool {
    let Some(stem) = Path::new(file_name).file_stem().and_then(|value| value.to_str()) else {
        return false;
    };
    file_name.ends_with(".png")
        && stem.len() == 16
        && stem.chars().all(|ch| ch.is_ascii_hexdigit())
}
