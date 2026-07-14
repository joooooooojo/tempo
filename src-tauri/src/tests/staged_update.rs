use crate::staged_update::test_api::{
    read_state, safe_join, state_path, version_is_newer, versions_dir, write_state,
    StagedUpdateState, StagedVersionSlot,
};
use std::path::Path;

#[test]
fn safe_join_accepts_normal_relative_paths() {
    let base = Path::new("C:/Tempo");
    let joined = safe_join(base, Path::new("Tempo/Tempo.exe")).expect("safe path");

    assert!(joined.ends_with(Path::new("Tempo/Tempo.exe")));
}

#[test]
fn safe_join_rejects_parent_paths() {
    let err = safe_join(Path::new("C:/Tempo"), Path::new("../evil.exe"))
        .expect_err("parent path should be rejected");

    assert!(err.contains("上级路径"));
}

#[test]
fn version_comparison_uses_semver_ordering() {
    assert!(version_is_newer("1.0.10", "1.0.9"));
    assert!(!version_is_newer("1.0.9", "1.0.10"));
    assert!(version_is_newer("v1.1.0", "1.0.99"));
}

#[test]
fn state_round_trips_to_json_file() {
    let root = unique_temp_dir("tempo-staged-state");
    let state = StagedUpdateState {
        active: Some(slot("1.0.6", "C:/Tempo/versions/1.0.6/Tempo.exe")),
        pending: Some(slot("1.0.7", "C:/Tempo/versions/1.0.7/Tempo.exe")),
        previous: None,
        failed: Vec::new(),
    };

    write_state(&root, &state).expect("write state");
    let loaded = read_state(&root).expect("read state");

    assert!(state_path(&root).exists());
    assert_eq!(loaded.active.unwrap().version, "1.0.6");
    assert_eq!(loaded.pending.unwrap().version, "1.0.7");
    cleanup_temp_dir(&root);
}

#[test]
fn corrupt_state_file_falls_back_to_default_state() {
    let root = unique_temp_dir("tempo-staged-corrupt-state");
    std::fs::write(state_path(&root), b"{not json").expect("write corrupt state");

    let loaded = read_state(&root).expect("read corrupt state");

    assert!(loaded.active.is_none());
    assert!(loaded.pending.is_none());
    cleanup_temp_dir(&root);
}

#[test]
fn versions_dir_stays_under_staged_root() {
    let root = Path::new("C:/Users/example/AppData/Tempo/staged-updates");
    let versions = versions_dir(root);

    assert!(versions.starts_with(root));
    assert!(versions.ends_with("versions"));
}

fn slot(version: &str, launch_path: &str) -> StagedVersionSlot {
    StagedVersionSlot {
        version: version.to_string(),
        launch_path: launch_path.to_string(),
        target: "windows-x86_64-staged".into(),
        installed_at: "2026-07-14T00:00:00Z".into(),
        launch_attempts: 0,
    }
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn cleanup_temp_dir(path: &Path) {
    let temp = std::env::temp_dir();
    assert!(path.starts_with(&temp));
    let _ = std::fs::remove_dir_all(path);
}
