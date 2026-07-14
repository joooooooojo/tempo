use chrono::Utc;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Cursor};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use url::Url;

const STAGED_UPDATE_ENDPOINT: &str =
    "https://github.com/joooooooojo/tempo/releases/latest/download/staged-latest.json";
const STAGED_ROOT_DIR: &str = "staged-updates";
const STATE_FILE: &str = "update-state.json";
const VERSIONS_DIR: &str = "versions";
const STAGED_CHILD_ARG: &str = "--tempo-staged-child";
const STAGED_VERSION_ARG: &str = "--tempo-staged-version=";
const MAX_PENDING_LAUNCH_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StagedUpdateState {
    #[serde(default)]
    pub active: Option<StagedVersionSlot>,
    #[serde(default)]
    pub pending: Option<StagedVersionSlot>,
    #[serde(default)]
    pub previous: Option<StagedVersionSlot>,
    #[serde(default)]
    pub failed: Vec<FailedStagedVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedVersionSlot {
    pub version: String,
    pub launch_path: String,
    pub target: String,
    pub installed_at: String,
    #[serde(default)]
    pub launch_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailedStagedVersion {
    pub version: String,
    pub reason: String,
    pub failed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StagedUpdateResult {
    pub status: String,
    pub current_version: String,
    pub version: Option<String>,
    pub pending_version: Option<String>,
    pub active_version: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct StagedUpdateProgress {
    phase: &'static str,
    downloaded: u64,
    total: u64,
    version: String,
}

#[tauri::command]
pub fn staged_update_status(app: AppHandle) -> Result<StagedUpdateResult, String> {
    let root = staged_root(&app)?;
    let state = read_state(&root)?;
    Ok(result_from_state("idle", &state, None, None))
}

#[tauri::command]
pub async fn staged_check_update(app: AppHandle) -> Result<StagedUpdateResult, String> {
    let root = staged_root(&app)?;
    let state = read_state(&root)?;
    if let Some(pending) = ready_pending_slot(&state) {
        return Ok(result_from_state(
            "ready",
            &state,
            Some(pending.version.clone()),
            None,
        ));
    }

    let updater = staged_updater(&app)?;
    match updater.check().await.map_err(|error| {
        tracing::warn!(error = %error, "failed to check staged update");
        error.to_string()
    })? {
        Some(update) => Ok(result_from_state(
            "available",
            &state,
            Some(update.version),
            update.body,
        )),
        None => Ok(result_from_state("latest", &state, None, None)),
    }
}

#[tauri::command]
pub async fn staged_download_update(app: AppHandle) -> Result<StagedUpdateResult, String> {
    let root = staged_root(&app)?;
    let updater = staged_updater(&app)?;
    let Some(update) = updater.check().await.map_err(|error| {
        tracing::warn!(error = %error, "failed to check staged update before download");
        error.to_string()
    })?
    else {
        let state = read_state(&root)?;
        return Ok(result_from_state("latest", &state, None, None));
    };

    let version = update.version.clone();
    emit_progress(&app, "downloading", 0, 0, &version);

    let downloaded = Arc::new(AtomicU64::new(0));
    let progress_downloaded = Arc::clone(&downloaded);
    let finished_downloaded = Arc::clone(&downloaded);
    let bytes = update
        .download(
            |chunk_len, total| {
                let downloaded =
                    progress_downloaded.fetch_add(chunk_len as u64, Ordering::Relaxed)
                        + chunk_len as u64;
                emit_progress(&app, "downloading", downloaded, total.unwrap_or(0), &version);
            },
            || {
                let downloaded = finished_downloaded.load(Ordering::Relaxed);
                emit_progress(&app, "installing", downloaded, downloaded, &version);
            },
        )
        .await
        .map_err(|error| {
            tracing::warn!(version = %version, error = %error, "failed to download staged update");
            error.to_string()
        })?;

    let slot = stage_package(&root, &version, staged_target(), &bytes)?;
    let mut state = read_state(&root)?;
    if let Some(existing) = state.pending.take() {
        if existing.version != slot.version {
            mark_failed(&mut state, existing.version, "replaced by newer staged update");
        }
    }
    state.pending = Some(slot.clone());
    write_state(&root, &state)?;
    let downloaded = downloaded.load(Ordering::Relaxed);
    emit_progress(&app, "ready", downloaded, downloaded, &version);

    tracing::info!(
        version = %slot.version,
        launch_path = %slot.launch_path,
        "staged update downloaded and prepared"
    );
    Ok(result_from_state("ready", &state, Some(slot.version), None))
}

#[tauri::command]
pub fn staged_restart_to_update(app: AppHandle) -> Result<(), String> {
    let root = staged_root(&app)?;
    let mut state = read_state(&root)?;
    let Some(slot) = state.pending.clone() else {
        return Err("没有已准备好的更新".into());
    };

    if !slot_exists(&slot) {
        state.pending = None;
        mark_failed(&mut state, slot.version, "staged update files are missing");
        write_state(&root, &state)?;
        return Err("更新文件不存在，请重新下载".into());
    }

    increment_pending_attempt(&mut state)?;
    write_state(&root, &state)?;
    launch_slot(&slot)?;
    tracing::info!(version = %slot.version, "restarting into staged update");
    app.cleanup_before_exit();
    std::process::exit(0);
}

pub fn forward_to_staged_version_if_needed(app: &AppHandle) -> Result<(), String> {
    if staged_child_version_arg().is_some() {
        return Ok(());
    }

    let root = staged_root(app)?;
    let mut state = read_state(&root)?;
    let current = current_version();

    if let Some(pending) = state.pending.clone() {
        if !slot_exists(&pending) {
            tracing::warn!(
                version = %pending.version,
                launch_path = %pending.launch_path,
                "pending staged update files are missing"
            );
            state.pending = None;
            mark_failed(&mut state, pending.version, "staged update files are missing");
            write_state(&root, &state)?;
        } else if version_is_newer(&pending.version, current) {
            if pending.launch_attempts >= MAX_PENDING_LAUNCH_ATTEMPTS {
                tracing::warn!(
                    version = %pending.version,
                    attempts = pending.launch_attempts,
                    "pending staged update exceeded launch attempts; rolling back"
                );
                state.pending = None;
                mark_failed(&mut state, pending.version, "launch attempts exceeded");
                write_state(&root, &state)?;
            } else {
                increment_pending_attempt(&mut state)?;
                let pending = state.pending.clone().expect("pending exists after increment");
                write_state(&root, &state)?;
                launch_slot(&pending)?;
                tracing::info!(version = %pending.version, "forwarding to pending staged update");
                app.cleanup_before_exit();
                std::process::exit(0);
            }
        }
    }

    if let Some(active) = state.active.clone() {
        if !slot_exists(&active) {
            tracing::warn!(
                version = %active.version,
                launch_path = %active.launch_path,
                "active staged update files are missing; falling back to bundled app"
            );
            state.active = None;
            mark_failed(&mut state, active.version, "active staged files are missing");
            write_state(&root, &state)?;
        } else if version_is_newer(&active.version, current) {
            launch_slot(&active)?;
            tracing::info!(version = %active.version, "forwarding to active staged version");
            app.cleanup_before_exit();
            std::process::exit(0);
        }
    }

    Ok(())
}

pub fn confirm_current_staged_launch(app: &AppHandle) -> Result<(), String> {
    let Some(version) = staged_child_version_arg() else {
        return Ok(());
    };

    if version != current_version() {
        tracing::warn!(
            expected_version = %version,
            actual_version = current_version(),
            "staged child version argument does not match current package version"
        );
        return Ok(());
    }

    let root = staged_root(app)?;
    let mut state = read_state(&root)?;
    let Some(pending) = state.pending.clone() else {
        return Ok(());
    };

    if pending.version != version {
        return Ok(());
    }

    let previous = state.active.take();
    let mut active = pending;
    active.launch_attempts = 0;
    state.previous = previous;
    state.active = Some(active.clone());
    state.pending = None;
    write_state(&root, &state)?;
    cleanup_old_versions(&root, &state);

    tracing::info!(
        version = %active.version,
        launch_path = %active.launch_path,
        "confirmed staged update launch"
    );
    Ok(())
}

fn staged_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let endpoint = Url::parse(STAGED_UPDATE_ENDPOINT).map_err(|error| error.to_string())?;
    app.updater_builder()
        .target(staged_target())
        .endpoints(vec![endpoint])
        .map_err(|error| error.to_string())?
        .build()
        .map_err(|error| error.to_string())
}

fn emit_progress(app: &AppHandle, phase: &'static str, downloaded: u64, total: u64, version: &str) {
    let payload = StagedUpdateProgress {
        phase,
        downloaded,
        total,
        version: version.to_string(),
    };
    if let Err(error) = app.emit("staged-update-progress", payload) {
        tracing::debug!(error = %error, "failed to emit staged update progress");
    }
}

fn result_from_state(
    status: &str,
    state: &StagedUpdateState,
    version: Option<String>,
    notes: Option<String>,
) -> StagedUpdateResult {
    StagedUpdateResult {
        status: status.to_string(),
        current_version: current_version().to_string(),
        version,
        pending_version: state.pending.as_ref().map(|slot| slot.version.clone()),
        active_version: state.active.as_ref().map(|slot| slot.version.clone()),
        notes,
    }
}

fn ready_pending_slot(state: &StagedUpdateState) -> Option<&StagedVersionSlot> {
    state.pending.as_ref().filter(|slot| slot_exists(slot))
}

fn staged_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join(STAGED_ROOT_DIR))
}

pub(crate) fn state_path(root: &Path) -> PathBuf {
    root.join(STATE_FILE)
}

pub(crate) fn versions_dir(root: &Path) -> PathBuf {
    root.join(VERSIONS_DIR)
}

pub(crate) fn read_state(root: &Path) -> Result<StagedUpdateState, String> {
    let path = state_path(root);
    let Ok(data) = fs::read_to_string(&path) else {
        return Ok(StagedUpdateState::default());
    };
    match serde_json::from_str(&data) {
        Ok(state) => Ok(state),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "failed to parse staged update state; falling back to bundled app"
            );
            Ok(StagedUpdateState::default())
        }
    }
}

pub(crate) fn write_state(root: &Path, state: &StagedUpdateState) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let path = state_path(root);
    let temp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?;
    fs::write(&temp, data).map_err(|error| error.to_string())?;
    fs::rename(&temp, &path).map_err(|error| error.to_string())
}

fn stage_package(
    root: &Path,
    version: &str,
    target: &'static str,
    bytes: &[u8],
) -> Result<StagedVersionSlot, String> {
    validate_version(version)?;
    let version_dir = version_dir_name(version)?;
    let versions = versions_dir(root);
    fs::create_dir_all(&versions).map_err(|error| error.to_string())?;

    let temp_dir = versions.join(format!("{version_dir}.tmp"));
    let final_dir = versions.join(&version_dir);
    remove_dir_all_within(&versions, &temp_dir)?;
    remove_dir_all_within(&versions, &final_dir)?;
    fs::create_dir_all(&temp_dir).map_err(|error| error.to_string())?;

    extract_package(bytes, &temp_dir)?;
    let temp_launch_path = locate_launch_path(&temp_dir)?;
    let relative_launch_path = temp_launch_path
        .strip_prefix(&temp_dir)
        .map_err(|error| error.to_string())?
        .to_path_buf();

    fs::rename(&temp_dir, &final_dir).map_err(|error| error.to_string())?;
    let launch_path = final_dir.join(relative_launch_path);
    Ok(StagedVersionSlot {
        version: version.to_string(),
        launch_path: launch_path.to_string_lossy().into_owned(),
        target: target.to_string(),
        installed_at: Utc::now().to_rfc3339(),
        launch_attempts: 0,
    })
}

#[cfg(target_os = "windows")]
fn extract_package(bytes: &[u8], dest: &Path) -> Result<(), String> {
    extract_zip(bytes, dest)
}

#[cfg(target_os = "macos")]
fn extract_package(bytes: &[u8], dest: &Path) -> Result<(), String> {
    extract_tar_gz(bytes, dest)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn extract_package(_bytes: &[u8], _dest: &Path) -> Result<(), String> {
    Err("当前平台不支持 staged update".into())
}

#[cfg(target_os = "windows")]
fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|error| error.to_string())?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| format!("更新包包含不安全路径: {}", file.name()))?;
        let out_path = safe_join(dest, &enclosed)?;

        if file.is_dir() {
            fs::create_dir_all(&out_path).map_err(|error| error.to_string())?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut output = fs::File::create(&out_path).map_err(|error| error.to_string())?;
        io::copy(&mut file, &mut output).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let relative = entry.path().map_err(|error| error.to_string())?.to_path_buf();
        let out_path = safe_join(dest, &relative)?;
        let entry_type = entry.header().entry_type();

        if entry_type.is_dir() {
            fs::create_dir_all(&out_path).map_err(|error| error.to_string())?;
        } else if entry_type.is_file() {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            entry.unpack(&out_path).map_err(|error| error.to_string())?;
        } else if entry_type.is_symlink() {
            let link_name = entry
                .link_name()
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("更新包包含空符号链接: {}", relative.display()))?;
            let parent = out_path.parent().unwrap_or(dest);
            let _ = safe_join(parent, &link_name)?;
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::os::unix::fs::symlink(&link_name, &out_path)
                .map_err(|error| error.to_string())?;
        } else {
            return Err(format!(
                "更新包包含不支持的条目类型: {}",
                relative.to_string_lossy()
            ));
        }
    }
    Ok(())
}

pub(crate) fn safe_join(base: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.is_absolute() {
        return Err(format!("更新包包含绝对路径: {}", relative.display()));
    }

    let mut output = PathBuf::from(base);
    for component in relative.components() {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!("更新包包含上级路径: {}", relative.display()));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("更新包包含不安全路径: {}", relative.display()));
            }
        }
    }
    Ok(output)
}

fn locate_launch_path(root: &Path) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        locate_windows_executable(root)
    }
    #[cfg(target_os = "macos")]
    {
        locate_macos_app_bundle(root)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = root;
        Err("当前平台不支持 staged update".into())
    }
}

#[cfg(target_os = "windows")]
fn locate_windows_executable(root: &Path) -> Result<PathBuf, String> {
    let mut stack = vec![root.to_path_buf()];
    let mut fallback = None;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            let is_exe = path
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
            if !is_exe {
                continue;
            }

            let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
            if stem.eq_ignore_ascii_case("tempo") {
                return Ok(path);
            }
            fallback.get_or_insert(path);
        }
    }

    fallback.ok_or_else(|| "更新包中没有找到 Tempo.exe".into())
}

#[cfg(target_os = "macos")]
fn locate_macos_app_bundle(root: &Path) -> Result<PathBuf, String> {
    let mut stack = vec![root.to_path_buf()];
    let mut fallback = None;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            if path
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|ext| ext == "app")
            {
                let name = path.file_stem().and_then(OsStr::to_str).unwrap_or_default();
                if name == "Tempo" {
                    return Ok(path);
                }
                fallback.get_or_insert(path);
            } else {
                stack.push(path);
            }
        }
    }

    fallback.ok_or_else(|| "更新包中没有找到 Tempo.app".into())
}

fn launch_slot(slot: &StagedVersionSlot) -> Result<(), String> {
    let launch_path = PathBuf::from(&slot.launch_path);
    if !slot_exists(slot) {
        return Err(format!("版本 {} 的启动文件不存在", slot.version));
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg("-n")
            .arg(&launch_path)
            .arg("--args")
            .arg(STAGED_CHILD_ARG)
            .arg(format!("{STAGED_VERSION_ARG}{}", slot.version))
            .spawn()
            .map_err(|error| error.to_string())?;
        drop(status);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let child = Command::new(&launch_path)
            .arg(STAGED_CHILD_ARG)
            .arg(format!("{STAGED_VERSION_ARG}{}", slot.version))
            .spawn()
            .map_err(|error| error.to_string())?;
        drop(child);
    }

    Ok(())
}

fn slot_exists(slot: &StagedVersionSlot) -> bool {
    let path = PathBuf::from(&slot.launch_path);
    #[cfg(target_os = "macos")]
    {
        path.is_dir()
    }
    #[cfg(not(target_os = "macos"))]
    {
        path.is_file()
    }
}

fn increment_pending_attempt(state: &mut StagedUpdateState) -> Result<(), String> {
    let Some(pending) = state.pending.as_mut() else {
        return Err("没有已准备好的更新".into());
    };
    pending.launch_attempts = pending.launch_attempts.saturating_add(1);
    Ok(())
}

fn mark_failed(state: &mut StagedUpdateState, version: String, reason: &'static str) {
    state.failed.push(FailedStagedVersion {
        version,
        reason: reason.to_string(),
        failed_at: Utc::now().to_rfc3339(),
    });
}

fn cleanup_old_versions(root: &Path, state: &StagedUpdateState) {
    let versions = versions_dir(root);
    let Ok(entries) = fs::read_dir(&versions) else {
        return;
    };
    let keep = [
        state.active.as_ref().map(|slot| slot.version.as_str()),
        state.previous.as_ref().map(|slot| slot.version.as_str()),
        state.pending.as_ref().map(|slot| slot.version.as_str()),
    ];

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if keep.iter().flatten().any(|version| *version == name) {
            continue;
        }
        if let Err(error) = remove_dir_all_within(&versions, &path) {
            tracing::debug!(
                path = %path.display(),
                error = %error,
                "failed to cleanup old staged version"
            );
        }
    }
}

fn remove_dir_all_within(root: &Path, target: &Path) -> Result<(), String> {
    if !target.exists() {
        return Ok(());
    }
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let canonical_target = target.canonicalize().map_err(|error| error.to_string())?;
    if canonical_target == canonical_root || !canonical_target.starts_with(&canonical_root) {
        return Err(format!("拒绝删除 staged 目录之外的路径: {}", target.display()));
    }
    fs::remove_dir_all(&canonical_target).map_err(|error| error.to_string())
}

fn validate_version(version: &str) -> Result<(), String> {
    parse_version(version).map(|_| ())
}

fn version_dir_name(version: &str) -> Result<String, String> {
    validate_version(version)?;
    if !version
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_')
    {
        return Err(format!("版本号包含不安全字符: {version}"));
    }
    Ok(version.to_string())
}

pub(crate) fn version_is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Ok(candidate), Ok(current)) => candidate > current,
        _ => candidate > current,
    }
}

fn parse_version(version: &str) -> Result<Version, String> {
    Version::parse(version.trim_start_matches('v')).map_err(|error| error.to_string())
}

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn staged_child_version_arg() -> Option<String> {
    std::env::args()
        .find_map(|arg| arg.strip_prefix(STAGED_VERSION_ARG).map(str::to_string))
        .filter(|_| std::env::args().any(|arg| arg == STAGED_CHILD_ARG))
}

fn staged_target() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x86_64-staged"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "windows-aarch64-staged"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "darwin-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "darwin-x86_64"
    }
    #[cfg(not(any(
        all(target_os = "windows", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(target_os = "macos", any(target_arch = "x86_64", target_arch = "aarch64"))
    )))]
    {
        "unsupported"
    }
}

#[cfg(test)]
pub(crate) mod test_api {
    pub(crate) use super::{
        read_state, safe_join, state_path, version_is_newer, versions_dir, write_state,
        StagedUpdateState, StagedVersionSlot,
    };
}
