use crate::builtin_plugins::settings::commands::{
    apply_shortcut_updates, main_panel_icon_data_url_from_bytes, normalize_main_panel_icon_data_url,
};
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

#[test]
fn main_panel_icon_accepts_only_small_png_data_urls() {
    let png = "data:image/png;base64,iVBORw0KGgo=";
    assert_eq!(normalize_main_panel_icon_data_url(png).unwrap(), png);
    assert_eq!(normalize_main_panel_icon_data_url("  ").unwrap(), "");
    assert!(normalize_main_panel_icon_data_url("data:image/jpeg;base64,/9j/").is_err());
    assert!(normalize_main_panel_icon_data_url("data:image/png;base64,bm90LXBuZw==").is_err());
}

#[test]
fn main_panel_icon_image_is_decoded_and_fitted_to_the_canvas() {
    use base64::Engine as _;
    use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    let mut source = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 1, Rgba([24, 160, 88, 255])))
        .write_to(&mut source, ImageFormat::Png)
        .expect("encode source png");
    let data_url =
        main_panel_icon_data_url_from_bytes(&source.into_inner()).expect("create icon data url");
    let output = base64::engine::general_purpose::STANDARD
        .decode(data_url.trim_start_matches("data:image/png;base64,"))
        .expect("decode output png");
    let image = image::load_from_memory(&output).expect("read output png");

    assert_eq!(image.dimensions(), (192, 192));
}
