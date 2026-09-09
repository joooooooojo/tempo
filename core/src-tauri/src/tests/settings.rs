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
fn main_panel_icon_accepts_supported_data_urls() {
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    let mut source = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([24, 160, 88, 255])))
        .write_to(&mut source, ImageFormat::Png)
        .expect("encode source png");
    let data_url =
        main_panel_icon_data_url_from_bytes(&source.into_inner()).expect("create data url");

    assert_eq!(
        normalize_main_panel_icon_data_url(&data_url).unwrap(),
        data_url
    );
    assert_eq!(normalize_main_panel_icon_data_url("  ").unwrap(), "");
    assert!(
        normalize_main_panel_icon_data_url(&data_url.replace("image/png", "image/gif")).is_err()
    );
    assert!(normalize_main_panel_icon_data_url("data:image/png;base64,bm90LWltYWdl").is_err());
}

#[test]
fn main_panel_icon_preserves_source_format_and_dimensions() {
    use base64::Engine as _;
    use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    let mut source = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 1, Rgba([24, 160, 88, 255])))
        .write_to(&mut source, ImageFormat::Png)
        .expect("encode source png");
    let source = source.into_inner();
    let data_url = main_panel_icon_data_url_from_bytes(&source).expect("create icon data url");
    let output = base64::engine::general_purpose::STANDARD
        .decode(data_url.trim_start_matches("data:image/png;base64,"))
        .expect("decode output png");
    let image = image::load_from_memory(&output).expect("read output png");

    assert_eq!(output, source);
    assert_eq!(image.dimensions(), (2, 1));
}

#[test]
fn main_panel_icon_preserves_animated_gif_frames() {
    use base64::Engine as _;
    use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
    use image::{AnimationDecoder, Delay, Frame, Rgba, RgbaImage};
    use std::io::Cursor;

    let frames = [Rgba([255, 0, 0, 255]), Rgba([0, 255, 0, 255])].map(|color| {
        Frame::from_parts(
            RgbaImage::from_pixel(2, 2, color),
            0,
            0,
            Delay::from_numer_denom_ms(80, 1),
        )
    });
    let mut source = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut source);
        encoder
            .set_repeat(Repeat::Infinite)
            .expect("set GIF repeat");
        encoder.encode_frames(frames).expect("encode animated GIF");
    }

    let data_url = main_panel_icon_data_url_from_bytes(&source).expect("create GIF data url");
    assert!(data_url.starts_with("data:image/gif;base64,"));
    let output = base64::engine::general_purpose::STANDARD
        .decode(data_url.trim_start_matches("data:image/gif;base64,"))
        .expect("decode output GIF");
    let decoded = GifDecoder::new(Cursor::new(&output))
        .expect("decode GIF")
        .into_frames()
        .collect_frames()
        .expect("collect GIF frames");

    assert_eq!(output, source);
    assert_eq!(decoded.len(), 2);
}
