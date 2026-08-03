//! Composite Dock / taskbar icons for standalone plugin windows: Tempo platform icon with a
//! plugin badge in the corner (matching the main panel's default plugin glyph when unset).

use std::path::Path;

use image::imageops::FilterType;
use image::{DynamicImage, Rgba, RgbaImage};
use tauri::image::Image;
use tauri::{AppHandle, WebviewWindow};

use super::loader::resolve_contribution_icon_relative_path;
use super::manifest::PluginManifest;

const BADGE_SCALE: f32 = 0.40;
const BADGE_INSET: f32 = 0.04;
const BADGE_BACKING_PAD: f32 = 0.10;
const MAX_ICON_BYTES: u64 = 256 * 1024;

/// Apply a platform icon + plugin badge to a standalone plugin window. Failures are logged and
/// ignored so window creation is never blocked on icon decoding.
pub fn apply_standalone_window_icon(
    app: &AppHandle,
    window: &WebviewWindow,
    package_root: &Path,
    manifest: &PluginManifest,
    app_id: &str,
) {
    let Some(contribution) = manifest
        .contributes
        .apps
        .iter()
        .find(|candidate| candidate.id == app_id)
    else {
        return;
    };
    let icon_rel =
        resolve_contribution_icon_relative_path(contribution.icon.as_ref(), manifest);
    let Some(composite) = build_composite_icon(app, package_root, icon_rel.as_deref()) else {
        return;
    };
    crate::logging::debug_if_err(
        window.set_icon(composite),
        "set standalone plugin window icon",
    );
}

fn build_composite_icon(
    app: &AppHandle,
    package_root: &Path,
    icon_rel: Option<&str>,
) -> Option<Image<'static>> {
    let platform = app.default_window_icon()?.clone();
    let width = platform.width();
    let height = platform.height();
    if width == 0 || height == 0 {
        return None;
    }

    let mut base = image_from_rgba(platform.rgba(), width, height)?;
    let badge_size = ((width.min(height) as f32) * BADGE_SCALE)
        .round()
        .max(16.0) as u32;
    let badge = load_badge_image(package_root, icon_rel, badge_size)
        .unwrap_or_else(|| render_default_plugin_badge(badge_size));

    let inset = ((width.min(height) as f32) * BADGE_INSET).round() as i64;
    let x = width as i64 - badge_size as i64 - inset;
    let y = height as i64 - badge_size as i64 - inset;
    draw_badge_backing(&mut base, x, y, badge_size);
    overlay_rgba(&mut base, x, y, &badge);
    Some(image_to_tauri(base))
}

fn image_from_rgba(rgba: &[u8], width: u32, height: u32) -> Option<RgbaImage> {
    if rgba.len() != (width * height * 4) as usize {
        return None;
    }
    RgbaImage::from_raw(width, height, rgba.to_vec())
}

fn image_to_tauri(image: RgbaImage) -> Image<'static> {
    let (width, height) = image.dimensions();
    Image::new_owned(image.into_raw(), width, height)
}

fn load_badge_image(package_root: &Path, icon_rel: Option<&str>, size: u32) -> Option<RgbaImage> {
    let rel = icon_rel?;
    let path = resolve_package_icon_path(package_root, rel)?;
    if path.metadata().ok()?.len() > MAX_ICON_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    decode_icon_bytes(&bytes, &path, size)
}

fn resolve_package_icon_path(package_root: &Path, rel_path: &str) -> Option<std::path::PathBuf> {
    let canonical_root = package_root.canonicalize().ok()?;
    let candidate = package_root.join(rel_path);
    let canonical_path = candidate.canonicalize().ok()?;
    if canonical_path.starts_with(&canonical_root) && canonical_path.is_file() {
        Some(canonical_path)
    } else {
        None
    }
}

fn decode_icon_bytes(bytes: &[u8], path: &Path, size: u32) -> Option<RgbaImage> {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "svg" {
        return rasterize_svg(bytes, size);
    }
    let image = image::load_from_memory(bytes).ok()?;
    Some(resize_square(image, size))
}

fn rasterize_svg(bytes: &[u8], size: u32) -> Option<RgbaImage> {
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / tree.size().height().max(tree.size().width()).max(1.0);
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut rgba = pixmap.data().to_vec();
    unpremultiply_rgba(&mut rgba);
    RgbaImage::from_raw(size, size, rgba)
}

fn unpremultiply_rgba(rgba: &mut [u8]) {
    for chunk in rgba.chunks_mut(4) {
        let alpha = chunk[3];
        if alpha == 0 || alpha == 255 {
            continue;
        }
        let factor = 255.0 / alpha as f32;
        for channel in &mut chunk[..3] {
            *channel = ((*channel as f32) * factor).round().clamp(0.0, 255.0) as u8;
        }
    }
}

fn resize_square(image: DynamicImage, size: u32) -> RgbaImage {
    image
        .resize_exact(size, size, FilterType::Triangle)
        .to_rgba8()
}

/// Default plugin badge: rounded tile with an application-window glyph (matches panel fallback).
fn render_default_plugin_badge(size: u32) -> RgbaImage {
    let mut image = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    let s = size as f32;
    let radius = s * 0.22;
    let bg = Rgba([250, 250, 250, 255]);
    let border = Rgba([228, 228, 231, 255]);
    let glyph = Rgba([113, 113, 122, 255]);

    fill_rounded_rect(&mut image, 0.0, 0.0, s, s, radius, bg);
    stroke_rounded_rect(&mut image, 0.6, 0.6, s - 1.2, s - 1.2, radius, border, 1.2);

    let pad = s * 0.24;
    let inner_w = s - pad * 2.0;
    let inner_h = s - pad * 2.0;
    let inner_r = inner_w * 0.16;
    stroke_rounded_rect(
        &mut image,
        pad,
        pad,
        inner_w,
        inner_h,
        inner_r,
        glyph,
        (s * 0.07).max(1.2),
    );
    let title_y = pad + inner_h * 0.22;
    draw_line(
        &mut image,
        pad + inner_w * 0.12,
        title_y,
        pad + inner_w * 0.88,
        title_y,
        (s * 0.06).max(1.0),
        glyph,
    );
    image
}

fn draw_badge_backing(base: &mut RgbaImage, x: i64, y: i64, badge_size: u32) {
    let pad = (badge_size as f32 * BADGE_BACKING_PAD).round() as i64;
    let diameter = badge_size as i64 + pad * 2;
    let cx = x + badge_size as i64 / 2;
    let cy = y + badge_size as i64 / 2;
    let radius = diameter as f32 / 2.0;
    let backing = Rgba([255, 255, 255, 235]);
    let shadow = Rgba([15, 23, 42, 45]);

    for dy in -radius as i64..=radius as i64 {
        for dx in -radius as i64..=radius as i64 {
            let px = cx + dx;
            let py = cy + dy;
            let dist = ((dx * dx + dy * dy) as f32).sqrt();
            if dist > radius + 1.5 {
                continue;
            }
            let alpha = if dist > radius - 1.0 {
                ((radius + 1.5 - dist) / 2.5).clamp(0.0, 1.0)
            } else {
                1.0
            };
            blend_pixel(base, px, py, shadow, alpha * 0.55);
            blend_pixel(base, px, py, backing, alpha);
        }
    }
}

fn overlay_rgba(base: &mut RgbaImage, x: i64, y: i64, overlay: &RgbaImage) {
    let (overlay_w, overlay_h) = overlay.dimensions();
    for oy in 0..overlay_h {
        for ox in 0..overlay_w {
            let pixel = overlay.get_pixel(ox, oy);
            if pixel[3] == 0 {
                continue;
            }
            blend_pixel(base, x + ox as i64, y + oy as i64, *pixel, 1.0);
        }
    }
}

fn blend_pixel(image: &mut RgbaImage, x: i64, y: i64, color: Rgba<u8>, factor: f32) {
    if x < 0 || y < 0 {
        return;
    }
    let (width, height) = image.dimensions();
    if x >= width as i64 || y >= height as i64 {
        return;
    }
    let dst = image.get_pixel_mut(x as u32, y as u32);
    let src_a = (color[3] as f32 / 255.0) * factor;
    if src_a <= 0.0 {
        return;
    }
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0 {
        *dst = Rgba([0, 0, 0, 0]);
        return;
    }
    for index in 0..3 {
        dst[index] = ((color[index] as f32 * src_a + dst[index] as f32 * dst_a * (1.0 - src_a))
            / out_a)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn fill_rounded_rect(
    image: &mut RgbaImage,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    color: Rgba<u8>,
) {
    for py in 0..image.height() {
        for px in 0..image.width() {
            if inside_rounded_rect(px as f32, py as f32, x, y, width, height, radius) {
                image.put_pixel(px, py, color);
            }
        }
    }
}

fn stroke_rounded_rect(
    image: &mut RgbaImage,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    color: Rgba<u8>,
    stroke: f32,
) {
    for py in 0..image.height() {
        for px in 0..image.width() {
            let inside = inside_rounded_rect(px as f32, py as f32, x, y, width, height, radius);
            let outside = !inside_rounded_rect(
                px as f32,
                py as f32,
                x + stroke,
                y + stroke,
                width - stroke * 2.0,
                height - stroke * 2.0,
                (radius - stroke).max(0.0),
            );
            if inside && outside {
                image.put_pixel(px, py, color);
            }
        }
    }
}

fn inside_rounded_rect(
    px: f32,
    py: f32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
) -> bool {
    if px < x || py < y || px >= x + width || py >= y + height {
        return false;
    }
    let radius = radius.min(width / 2.0).min(height / 2.0);
    let corners = [
        (x + radius, y + radius),
        (x + width - radius, y + radius),
        (x + radius, y + height - radius),
        (x + width - radius, y + height - radius),
    ];
    for (cx, cy) in corners {
        if (px < x + radius || px > x + width - radius)
            && (py < y + radius || py > y + height - radius)
        {
            let dx = px - cx;
            let dy = py - cy;
            if dx * dx + dy * dy > radius * radius {
                return false;
            }
        }
    }
    true
}

fn draw_line(
    image: &mut RgbaImage,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    color: Rgba<u8>,
) {
    let radius = thickness / 2.0;
    let min_x = x0.min(x1) - radius;
    let max_x = x0.max(x1) + radius;
    let min_y = y0.min(y1) - radius;
    let max_y = y0.max(y1) + radius;
    for py in min_y.floor().max(0.0) as u32..max_y.ceil().min(image.height() as f32) as u32 {
        for px in min_x.floor().max(0.0) as u32..max_x.ceil().min(image.width() as f32) as u32 {
            let fx = px as f32 + 0.5;
            let fy = py as f32 + 0.5;
            let dist = distance_point_to_segment(fx, fy, x0, y0, x1, y1);
            if dist <= radius {
                image.put_pixel(px, py, color);
            }
        }
    }
}

fn distance_point_to_segment(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    if dx == 0.0 && dy == 0.0 {
        return ((px - x0).powi(2) + (py - y0).powi(2)).sqrt();
    }
    let t = ((px - x0) * dx + (py - y0) * dy) / (dx * dx + dy * dy);
    let t = t.clamp(0.0, 1.0);
    let proj_x = x0 + t * dx;
    let proj_y = y0 + t * dy;
    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_badge_is_opaque_in_the_center() {
        let badge = render_default_plugin_badge(64);
        let center = badge.get_pixel(32, 32);
        assert!(center[3] > 200);
    }

    #[test]
    fn composite_uses_platform_dimensions() {
        let platform = Image::new_owned(vec![255; 4 * 32 * 32], 32, 32);
        let base = image_from_rgba(platform.rgba(), platform.width(), platform.height()).unwrap();
        assert_eq!(base.dimensions(), (32, 32));
    }

    #[test]
    fn png_icons_decode_for_badges() {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&[0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255])
                .unwrap();
        }
        let path = std::path::Path::new("icons/test.png");
        let image = decode_icon_bytes(&bytes, path, 16).expect("png badge");
        assert_eq!(image.dimensions(), (16, 16));
    }
}
