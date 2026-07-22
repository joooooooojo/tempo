//! Safe package import: directory copy, full-package hash, staging publish.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tauri::AppHandle;

use super::manifest::PluginManifest;
use super::paths::{ensure_dir, packages_dir, staging_dir};

const MAX_INPUT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 500 * 1024 * 1024;
const MAX_FILES: usize = 10_000;
const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackage {
    pub plugin_id: String,
    pub version: String,
    pub package_hash: String,
    pub install_path: String,
    pub requires_node_runtime: bool,
}

/// Import a local plugin directory into staging, validate, hash, and publish under packages/.
pub fn import_directory(app: &AppHandle, source: &Path) -> Result<InstalledPackage, String> {
    if !source.is_dir() {
        return Err(format!("not a directory: {}", source.display()));
    }

    let manifest_path = source.join("manifest.json");
    let raw =
        fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest.json: {e}"))?;
    let manifest = PluginManifest::parse_str(&raw)?;

    let op_id = format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S"),
        &uuid_like()
    );
    let staging_root = staging_dir(app)?.join(&op_id);
    ensure_dir(&staging_root)?;

    let copy_result = (|| {
        copy_plugin_tree(source, &staging_root)?;
        let staged_manifest = fs::read_to_string(staging_root.join("manifest.json"))
            .map_err(|e| format!("read staged manifest: {e}"))?;
        let staged = PluginManifest::parse_str(&staged_manifest)?;
        if staged.id != manifest.id || staged.version != manifest.version {
            return Err("staged manifest identity mismatch".into());
        }
        verify_entry_files(&staging_root, &staged)?;
        let package_hash = compute_package_hash(&staging_root)?;
        let dest = packages_dir(app)?.join(&staged.id).join(&staged.version);
        if dest.exists() {
            let existing_hash = compute_package_hash(&dest)?;
            if existing_hash != package_hash {
                return Err(format!(
                    "package {}@{} already installed with a different hash",
                    staged.id, staged.version
                ));
            }
            let _ = fs::remove_dir_all(&staging_root);
            return Ok(InstalledPackage {
                plugin_id: staged.id.clone(),
                version: staged.version.clone(),
                package_hash: existing_hash,
                install_path: dest.display().to_string(),
                requires_node_runtime: staged.requires_node_runtime(),
            });
        }

        if let Some(parent) = dest.parent() {
            ensure_dir(parent)?;
        }
        fs::rename(&staging_root, &dest).map_err(|e| {
            format!(
                "publish package failed ({} -> {}): {e}",
                staging_root.display(),
                dest.display()
            )
        })?;

        Ok(InstalledPackage {
            plugin_id: staged.id.clone(),
            version: staged.version.clone(),
            package_hash,
            install_path: dest.display().to_string(),
            requires_node_runtime: staged.requires_node_runtime(),
        })
    })();

    if copy_result.is_err() {
        let _ = fs::remove_dir_all(&staging_root);
    }
    copy_result
}

fn verify_entry_files(root: &Path, manifest: &PluginManifest) -> Result<(), String> {
    if let Some(main) = &manifest.main {
        let path = root.join(main);
        if !path.is_file() {
            return Err(format!("main entry missing: {main}"));
        }
    }
    for app_contrib in &manifest.contributes.apps {
        let path = root.join(&app_contrib.entry);
        if !path.is_file() {
            return Err(format!("app entry missing: {}", app_contrib.entry));
        }
    }
    Ok(())
}

fn copy_plugin_tree(source: &Path, dest: &Path) -> Result<(), String> {
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;

    for (rel, path) in list_regular_files(source)? {
        validate_package_rel_path(&rel)?;
        let meta = fs::metadata(&path).map_err(|e| format!("metadata {}: {e}", path.display()))?;
        let len = meta.len();
        if len > MAX_FILE_BYTES {
            return Err(format!("file too large: {rel}"));
        }
        total_bytes = total_bytes.saturating_add(len);
        if total_bytes > MAX_INPUT_BYTES || total_bytes > MAX_EXTRACTED_BYTES {
            return Err("plugin package exceeds size limits".into());
        }
        file_count += 1;
        if file_count > MAX_FILES {
            return Err("plugin package has too many files".into());
        }

        let target = dest.join(PathBuf::from(rel.replace('/', std::path::MAIN_SEPARATOR_STR)));
        if let Some(parent) = target.parent() {
            ensure_dir(parent)?;
        }
        fs::copy(&path, &target)
            .map_err(|e| format!("copy {} -> {}: {e}", path.display(), target.display()))?;
    }

    if file_count == 0 {
        return Err("plugin directory contains no files".into());
    }
    Ok(())
}

fn list_regular_files(root: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = Vec::new();
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
        let entries = fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("dir entry: {e}"))?;
            let path = entry.path();
            let ft = entry
                .file_type()
                .map_err(|e| format!("file_type {}: {e}", path.display()))?;
            if ft.is_symlink() {
                return Err(format!("symlinks are not allowed: {}", path.display()));
            }
            if ft.is_dir() {
                walk(&path, root, out)?;
            } else if ft.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| format!("strip prefix: {e}"))?;
                let key = rel.to_string_lossy().replace('\\', "/");
                out.push((key, path));
            }
        }
        Ok(())
    }
    walk(root, root, &mut out)?;
    Ok(out)
}

fn validate_package_rel_path(rel: &str) -> Result<(), String> {
    if rel.is_empty() || rel.starts_with('/') || rel.contains("..") || rel.contains(':') {
        return Err(format!("illegal package path: {rel}"));
    }
    if rel.contains('\0') {
        return Err("illegal NUL in path".into());
    }
    Ok(())
}

/// Deterministic full-package hash (design §8.1).
pub fn compute_package_hash(root: &Path) -> Result<String, String> {
    let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (rel, path) in list_regular_files(root)? {
        files.insert(rel, path);
    }

    let mut hasher = Sha256::new();
    for (rel, path) in files {
        let bytes = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        hasher.update((rel.len() as u64).to_be_bytes());
        hasher.update(rel.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}
