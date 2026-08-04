use crate::builtin_plugins::settings::commands::apply_shortcut_updates;
use crate::db::Settings;
use crate::validate_shortcut_bindings;
use serde_json::json;

#[test]
fn assigning_an_existing_shortcut_keeps_both_values() {
    let mut settings = Settings {
        shortcut_main_panel: "Control+Shift+F".into(),
        shortcut_clipboard_picker: "Control+Shift+V".into(),
        shortcut_snippet_picker: "Control+Shift+S".into(),
        ..Settings::default()
    };

    let changed = apply_shortcut_updates(
        &mut settings,
        &json!({ "shortcut_clipboard_picker": "Control+Shift+F" }),
    );

    assert!(changed);
    assert_eq!(settings.shortcut_main_panel, "Control+Shift+F");
    assert_eq!(settings.shortcut_clipboard_picker, "Control+Shift+F");
    assert_eq!(settings.shortcut_snippet_picker, "Control+Shift+S");
}

#[test]
fn empty_and_duplicate_shortcuts_are_valid_for_persistence() {
    let validated = validate_shortcut_bindings("", "Control+Shift+V", "")
        .expect("empty bindings should be valid");
    assert_eq!(validated, ("".into(), "Control+Shift+V".into(), "".into()));

    let duplicates = validate_shortcut_bindings(
        "Control+Shift+V",
        "Control+Shift+V",
        "Control+Shift+S",
    )
    .expect("duplicates should persist so UI can show conflict status");
    assert_eq!(
        duplicates,
        (
            "Control+Shift+V".into(),
            "Control+Shift+V".into(),
            "Control+Shift+S".into()
        )
    );
}
