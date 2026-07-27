use parking_lot::{Mutex, RwLock};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;
use tauri::http::{
    header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE},
    Method, Request, Response, StatusCode,
};

pub const APP_ICON_PROTOCOL: &str = "tempo-app-icon";
const MAX_MEMORY_ICONS: usize = 256;

#[derive(Clone)]
struct AppIconSource {
    app_name: String,
    source: String,
}

#[derive(Default)]
struct AppIconLru {
    values: HashMap<String, Vec<u8>>,
    order: VecDeque<String>,
}

impl AppIconLru {
    fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        let value = self.values.get(key)?.clone();
        self.touch(key);
        Some(value)
    }

    fn insert(&mut self, key: String, value: Vec<u8>) {
        self.values.insert(key.clone(), value);
        self.touch(&key);
        while self.order.len() > MAX_MEMORY_ICONS {
            if let Some(oldest) = self.order.pop_front() {
                self.values.remove(&oldest);
            }
        }
    }

    fn touch(&mut self, key: &str) {
        if let Some(index) = self.order.iter().position(|cached| cached == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.to_string());
    }
}

pub struct AppIconService {
    sources: RwLock<HashMap<String, AppIconSource>>,
    cache: Mutex<AppIconLru>,
    extraction: Mutex<()>,
    disk_dir: RwLock<Option<PathBuf>>,
}

impl AppIconService {
    pub fn global() -> &'static Self {
        static SERVICE: OnceLock<AppIconService> = OnceLock::new();
        SERVICE.get_or_init(|| AppIconService {
            sources: RwLock::new(HashMap::new()),
            cache: Mutex::new(AppIconLru::default()),
            extraction: Mutex::new(()),
            disk_dir: RwLock::new(None),
        })
    }

    /// Persist resized PNG thumbnails under the Tempo storage root so cold launches
    /// do not re-run macOS `sips` / Windows extraction for every search result.
    pub fn configure_disk_cache(&self, dir: PathBuf) {
        if let Err(error) = std::fs::create_dir_all(&dir) {
            tracing::warn!(error = %error, path = %dir.display(), "failed to create app icon cache dir");
            return;
        }
        *self.disk_dir.write() = Some(dir);
    }

    pub fn icon_url(&self, app_name: &str, source: &str) -> Option<String> {
        let app_name = app_name.trim();
        let source = source.trim();
        if app_name.is_empty() || source.is_empty() {
            return None;
        }

        let key = icon_key(app_name, source);
        self.sources.write().insert(
            key.clone(),
            AppIconSource {
                app_name: app_name.to_string(),
                source: source.to_string(),
            },
        );
        Some(crate::asset_protocol::asset_url_for_file_name(
            APP_ICON_PROTOCOL,
            &format!("{key}.png"),
        ))
    }

    /// Register sources and extract thumbnails off the UI path (e.g. after indexing).
    pub fn prewarm(&self, items: &[(String, String)]) {
        for (app_name, source) in items {
            let Some(_) = self.icon_url(app_name, source) else {
                continue;
            };
            let key = icon_key(app_name, source);
            let _ = self.icon_bytes(&key);
        }
    }

    pub fn protocol_response(&self, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
        if request.method() != Method::GET && request.method() != Method::HEAD {
            return icon_error(StatusCode::METHOD_NOT_ALLOWED);
        }

        let file_name = request.uri().path().trim_start_matches('/');
        let Some(key) = file_name.strip_suffix(".png") else {
            return icon_error(StatusCode::BAD_REQUEST);
        };
        if key.len() != 24 || !key.chars().all(|character| character.is_ascii_hexdigit()) {
            return icon_error(StatusCode::BAD_REQUEST);
        }

        let Some(bytes) = self.icon_bytes(key) else {
            return icon_error(StatusCode::NOT_FOUND);
        };
        let body = if request.method() == Method::HEAD {
            Vec::new()
        } else {
            bytes.clone()
        };
        Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "image/png")
            .header(CONTENT_LENGTH, bytes.len())
            .header(CACHE_CONTROL, "public, max-age=31536000, immutable")
            .body(body)
            .unwrap()
    }

    fn icon_bytes(&self, key: &str) -> Option<Vec<u8>> {
        if let Some(bytes) = self.cache.lock().get(key) {
            return Some(bytes);
        }
        if let Some(bytes) = self.read_disk(key) {
            self.cache.lock().insert(key.to_string(), bytes.clone());
            return Some(bytes);
        }

        // macOS AppKit and sips are unreliable under concurrent extraction. The
        // same serialization also prevents duplicate work for simultaneous views.
        let _extraction_guard = self.extraction.lock();
        if let Some(bytes) = self.cache.lock().get(key) {
            return Some(bytes);
        }
        if let Some(bytes) = self.read_disk(key) {
            self.cache.lock().insert(key.to_string(), bytes.clone());
            return Some(bytes);
        }

        let source = self.sources.read().get(key).cloned()?;
        let _icon_context = crate::platform::icon_extraction_thread_context();
        let bytes = crate::platform::extract_icon_png_bytes(&source.app_name, &source.source)?;
        self.write_disk(key, &bytes);
        self.cache.lock().insert(key.to_string(), bytes.clone());
        Some(bytes)
    }

    fn disk_path(&self, key: &str) -> Option<PathBuf> {
        self.disk_dir
            .read()
            .as_ref()
            .map(|dir| dir.join(format!("{key}.png")))
    }

    fn read_disk(&self, key: &str) -> Option<Vec<u8>> {
        let path = self.disk_path(key)?;
        match std::fs::read(&path) {
            Ok(bytes) if !bytes.is_empty() => Some(bytes),
            Ok(_) => None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                tracing::debug!(error = %error, path = %path.display(), "failed to read app icon disk cache");
                None
            }
        }
    }

    fn write_disk(&self, key: &str, bytes: &[u8]) {
        let Some(path) = self.disk_path(key) else {
            return;
        };
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                tracing::debug!(error = %error, path = %parent.display(), "failed to ensure app icon cache dir");
                return;
            }
        }
        if let Err(error) = atomic_write(&path, bytes) {
            tracing::debug!(error = %error, path = %path.display(), "failed to write app icon disk cache");
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = path.with_extension("png.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

fn icon_key(app_name: &str, source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(app_name.trim().to_lowercase().as_bytes());
    hasher.update([0]);
    hasher.update(source.trim().to_lowercase().as_bytes());
    if let Ok(metadata) = std::fs::metadata(source.trim()) {
        hasher.update(metadata.len().to_le_bytes());
        if let Ok(modified) = metadata.modified().and_then(|time| {
            time.duration_since(UNIX_EPOCH)
                .map_err(std::io::Error::other)
        }) {
            hasher.update(modified.as_secs().to_le_bytes());
        }
    }
    hex::encode(&hasher.finalize()[..12])
}

fn icon_error(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(status).body(Vec::new()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::{icon_key, AppIconLru, MAX_MEMORY_ICONS};

    #[test]
    fn icon_keys_are_stable_and_do_not_expose_sources() {
        let source = r"C:\Program Files\Example\example.exe";
        let key = icon_key("Example", source);
        assert_eq!(key, icon_key("Example", source));
        assert_eq!(key.len(), 24);
        assert!(!key.contains("Example"));
        assert!(!key.contains("Program Files"));
    }

    #[test]
    fn memory_cache_evicts_the_least_recently_used_icon() {
        let mut cache = AppIconLru::default();
        for index in 0..MAX_MEMORY_ICONS {
            cache.insert(index.to_string(), vec![index as u8]);
        }
        assert_eq!(cache.get("0"), Some(vec![0]));
        cache.insert("new".into(), vec![255]);

        assert!(cache.get("1").is_none());
        assert_eq!(cache.get("0"), Some(vec![0]));
        assert_eq!(cache.get("new"), Some(vec![255]));
    }
}
