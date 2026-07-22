use crate::commands::settings::apply_shortcut_updates;
use crate::db::Settings;
use crate::validate_shortcut_bindings;
use serde_json::json;

#[test]
fn assigning_an_existing_shortcut_clears_its_previous_owner() {
    let mut settings = Settings {
        shortcut_command_palette: "Control+Shift+F".into(),
        shortcut_clipboard_picker: "Control+Shift+V".into(),
        shortcut_snippet_picker: "Control+Shift+S".into(),
        ..Settings::default()
    };

    let changed = apply_shortcut_updates(
        &mut settings,
        &json!({ "shortcut_clipboard_picker": "Control+Shift+F" }),
    );

    assert!(changed);
    assert_eq!(settings.shortcut_command_palette, "");
    assert_eq!(settings.shortcut_clipboard_picker, "Control+Shift+F");
    assert_eq!(settings.shortcut_snippet_picker, "Control+Shift+S");
}

#[test]
fn empty_shortcuts_are_valid_but_duplicate_non_empty_shortcuts_are_not() {
    let validated = validate_shortcut_bindings("", "Control+Shift+V", "")
        .expect("empty bindings should be valid");
    assert_eq!(validated, ("".into(), "Control+Shift+V".into(), "".into()));

    assert!(
        validate_shortcut_bindings("Control+Shift+V", "Control+Shift+V", "Control+Shift+S",)
            .is_err()
    );
}
