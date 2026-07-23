use crate::clipboard_db::{
    add_snippet, add_snippet_group, count_clipboard_entries, delete_snippet_group, encode_rgba_png,
    get_clipboard_entry, get_snippet, insert_clipboard_text, set_clipboard_entry_pinned,
};
use crate::db::init_db;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir()
        .join(format!(
            "tempo-rust-test-{}-{}-{nanos}",
            test_name,
            std::process::id()
        ))
        .join("tempo.db")
}

fn cleanup_temp_db(path: &Path) {
    if let Some(parent) = path.parent() {
        drop(std::fs::remove_dir_all(parent));
    }
}

#[test]
fn clipboard_history_trim_keeps_pinned_and_latest_entries() {
    let path = temp_db_path("clipboard-trim");
    {
        let conn = init_db(&path).expect("init db");
        let first = insert_clipboard_text(&conn, "first", None, None, 10).expect("insert first");
        set_clipboard_entry_pinned(&conn, first.id, true).expect("pin first");
        let second = insert_clipboard_text(&conn, "second", None, None, 10).expect("insert second");
        let third = insert_clipboard_text(&conn, "third", None, None, 2).expect("insert third");

        assert_eq!(count_clipboard_entries(&conn, None), 2);
        assert!(get_clipboard_entry(&conn, first.id).is_some());
        assert!(get_clipboard_entry(&conn, second.id).is_none());
        assert!(get_clipboard_entry(&conn, third.id).is_some());
    }
    cleanup_temp_db(&path);
}

#[test]
fn deleting_snippet_group_unassigns_snippets() {
    let path = temp_db_path("snippet-group-delete");
    {
        let conn = init_db(&path).expect("init db");
        let group = add_snippet_group(&conn, "Work", None).expect("create group");
        let snippet =
            add_snippet(&conn, "Title", "Body", &[], Some(group.id), None, None).expect("create snippet");

        assert!(delete_snippet_group(&conn, group.id));

        let snippet = get_snippet(&conn, snippet.id).expect("load snippet");
        assert_eq!(snippet.group_id, None);
    }
    cleanup_temp_db(&path);
}

#[test]
fn encode_rgba_png_validates_input_size() {
    let png = encode_rgba_png(1, 1, &[255, 0, 0, 255]).expect("encode png");

    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(encode_rgba_png(2, 2, &[255, 0, 0, 255]).is_none());
}
