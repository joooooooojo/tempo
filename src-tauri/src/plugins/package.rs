//! Safe package import: directory copy, full-package hash, staging publish.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
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

    let staging_root = new_staging_root(app)?;
    let result = (|| {
        copy_plugin_tree(source, &staging_root)?;
        publish_staged(app, &staging_root)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging_root);
    }
    result
}

/// Import a local plugin zip archive into staging, validate, hash, and publish under packages/.
/// Reuses the same staging/hash/path rules as `import_directory` — nothing in the archive is
/// executed and only the central directory + entry contents are read before user confirmation.
pub fn import_zip(app: &AppHandle, archive_path: &Path) -> Result<InstalledPackage, String> {
    if !archive_path.is_file() {
        return Err(format!("not a file: {}", archive_path.display()));
    }
    let archive_meta =
        fs::metadata(archive_path).map_err(|e| format!("stat zip: {e}"))?;
    if archive_meta.len() > MAX_INPUT_BYTES {
        return Err("plugin zip exceeds the 100 MiB input limit".into());
    }

    let staging_root = new_staging_root(app)?;
    let result = (|| {
        extract_zip_tree(archive_path, &staging_root)?;
        publish_staged(app, &staging_root)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging_root);
    }
    result
}

fn new_staging_root(app: &AppHandle) -> Result<PathBuf, String> {
    let op_id = format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S"),
        &uuid_like()
    );
    let staging_root = staging_dir(app)?.join(&op_id);
    ensure_dir(&staging_root)?;
    Ok(staging_root)
}

/// Validate the already-copied/extracted staging tree, compute its full-package hash, and
/// atomically publish it under `packages/{id}/{version}` as an untrusted, disabled install
/// (design §8.4). No plugin code runs during this step.
fn publish_staged(app: &AppHandle, staging_root: &Path) -> Result<InstalledPackage, String> {
    let staged_manifest = fs::read_to_string(staging_root.join("manifest.json"))
        .map_err(|e| format!("read staged manifest: {e}"))?;
    let staged = PluginManifest::parse_str(&staged_manifest)?;
    verify_entry_files(staging_root, &staged)?;
    let package_hash = compute_package_hash(staging_root)?;
    let dest = packages_dir(app)?.join(&staged.id).join(&staged.version);
    if dest.exists() {
        let existing_hash = compute_package_hash(&dest)?;
        if existing_hash != package_hash {
            return Err(format!(
                "package {}@{} already installed with a different hash",
                staged.id, staged.version
            ));
        }
        let _ = fs::remove_dir_all(staging_root);
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
    fs::rename(staging_root, &dest).map_err(|e| {
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
}

/// Re-hash an already-installed package and compare against the trusted/recorded hash before
/// letting the loader execute any of its code (design §8.4 step 9 — "before executing plugin
/// code"). Callers pass the hash recorded at trust time.
pub fn verify_package_hash(install_path: &Path, expected_hash: &str) -> Result<(), String> {
    let actual = compute_package_hash(install_path)?;
    if actual != expected_hash {
        return Err(format!(
            "package hash mismatch (expected {expected_hash}, got {actual}); \
             the installed files changed since this version was trusted"
        ));
    }
    Ok(())
}

/// Extract a zip archive into `dest`, applying the same size/count/path safety rules as
/// directory import (design §8.4). Nothing is executed; entries are streamed straight to disk.
fn extract_zip_tree(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(archive_path).map_err(|e| format!("open zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("read zip: {e}"))?;

    if archive.len() > MAX_FILES {
        return Err("plugin zip has too many entries".into());
    }

    let mut file_count = 0usize;
    let mut total_bytes = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("read zip entry {index}: {e}"))?;

        // `enclosed_name` already rejects `..`, absolute paths and other traversal tricks;
        // we additionally re-validate with our own rules for defense in depth.
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(format!("unsafe zip entry path: {}", entry.name()));
        };
        let rel = enclosed.to_string_lossy().replace('\\', "/");
        if rel.is_empty() || rel.ends_with('/') {
            // Directory entry — created implicitly when writing files below.
            continue;
        }
        validate_package_rel_path(&rel)?;

        const S_IFLNK: u32 = 0o120000;
        const S_IFMT: u32 = 0o170000;
        if let Some(mode) = entry.unix_mode() {
            if mode & S_IFMT == S_IFLNK {
                return Err(format!("symlinks are not allowed in zip: {rel}"));
            }
        }

        let size = entry.size();
        if size > MAX_FILE_BYTES {
            return Err(format!("file too large in zip: {rel}"));
        }
        total_bytes = total_bytes.saturating_add(size);
        if total_bytes > MAX_EXTRACTED_BYTES {
            return Err("plugin zip exceeds size limits after extraction".into());
        }
        file_count += 1;
        if file_count > MAX_FILES {
            return Err("plugin zip has too many files".into());
        }

        let target = dest.join(PathBuf::from(rel.replace('/', std::path::MAIN_SEPARATOR_STR)));
        let canonical_dest = dest
            .canonicalize()
            .map_err(|e| format!("canonicalize staging dir: {e}"))?;
        if let Some(parent) = target.parent() {
            ensure_dir(parent)?;
        }
        let canonical_parent = target
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .ok_or_else(|| format!("unsafe zip entry path: {rel}"))?;
        if !canonical_parent.starts_with(&canonical_dest) {
            return Err(format!("unsafe zip entry path: {rel}"));
        }

        let mut out = fs::File::create(&target)
            .map_err(|e| format!("write {}: {e}", target.display()))?;
        let mut limited = (&mut entry).take(MAX_FILE_BYTES + 1);
        let written = std::io::copy(&mut limited, &mut out)
            .map_err(|e| format!("extract {rel}: {e}"))?;
        if written > MAX_FILE_BYTES {
            return Err(format!("file too large in zip: {rel}"));
        }
    }

    if file_count == 0 {
        return Err("plugin zip contains no files".into());
    }
    Ok(())
}

fn verify_entry_files(root: &Path, manifest: &PluginManifest) -> Result<(), String> {
    // Package contract: manifest.json + fixed root entries live as siblings.
    // - UI plugins: index.html
    // - Headless / hybrid with Runtime: main.mjs or main.js
    if manifest.has_ui() {
        let ui = root.join(super::manifest::UI_ENTRY_FILE);
        if !ui.is_file() {
            return Err(format!(
                "UI plugins require `{}` beside manifest.json",
                super::manifest::UI_ENTRY_FILE
            ));
        }
    }
    if let Some(main) = &manifest.main {
        let path = root.join(main);
        if !path.is_file() {
            return Err(format!(
                "main entry missing at package root: {main} (expected beside manifest.json)"
            ));
        }
    } else if !manifest.has_ui() {
        return Err(format!(
            "headless plugins require `{}` or `{}` beside manifest.json",
            super::manifest::MAIN_ENTRY_FILES[0],
            super::manifest::MAIN_ENTRY_FILES[1]
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        assert!(validate_package_rel_path("../escape").is_err());
        assert!(validate_package_rel_path("a/../../b").is_err());
        assert!(validate_package_rel_path("/absolute").is_err());
        assert!(validate_package_rel_path("c:\\windows").is_err());
        assert!(validate_package_rel_path("has\0nul").is_err());
    }

    #[test]
    fn accepts_normal_relative_paths() {
        assert!(validate_package_rel_path("main.mjs").is_ok());
        assert!(validate_package_rel_path("icons/app.svg").is_ok());
        assert!(validate_package_rel_path("manifest.json").is_ok());
    }

    #[test]
    fn rejects_empty_path() {
        assert!(validate_package_rel_path("").is_err());
    }

    /// Directory import must reject symlinks anywhere in the tree (design §8.4 step 3/§15-2):
    /// a plugin package cannot smuggle content in by pointing a "file" outside the staging
    /// root, and Windows junctions/symlinks get the same treatment via `file_type().is_symlink()`.
    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_directory_tree() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("tempo-plugin-symlink-test-{}", uuid_like()));
        fs::create_dir_all(&root).expect("create test root");

        let target = root.join("real.txt");
        fs::write(&target, b"hello").expect("write real file");
        let link = root.join("linked.txt");
        symlink(&target, &link).expect("create symlink");

        let result = list_regular_files(&root);
        let _ = fs::remove_dir_all(&root);

        let error = result.expect_err("symlinked file must be rejected");
        assert!(error.contains("symlink"), "unexpected error: {error}");
    }

    /// A symlinked *directory* must also be rejected, not just symlinked files — `list_regular_files`
    /// checks `file_type().is_symlink()` before recursing (design §8.4 step 3).
    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_directories() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("tempo-plugin-symlink-dir-test-{}", uuid_like()));
        let real_dir = root.join("real-dir");
        fs::create_dir_all(&real_dir).expect("create real dir");
        fs::write(real_dir.join("inner.txt"), b"hi").expect("write inner file");
        let link_dir = root.join("linked-dir");
        symlink(&real_dir, &link_dir).expect("create symlinked dir");

        let result = list_regular_files(&root);
        let _ = fs::remove_dir_all(&root);

        let error = result.expect_err("symlinked directory must be rejected");
        assert!(error.contains("symlink"), "unexpected error: {error}");
    }
}
