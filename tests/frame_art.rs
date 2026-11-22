//! V2 相框外观专题工单 03：相框图分析的接缝测试。
//!
//! 分析接缝以程序生成的像素夹具喂 `frame_art::analyze_frame_art`，
//! 断言洞矩形 / 四边内距 / 拒绝原因；加载接缝用真实文件验证 PNG
//! 限定与缺失文件的拒绝口径。九宫格布局已统一到布局接缝，其
//! 测试在 tests/frame_layout.rs。

mod common;

use common::temp_root;
use image::{DynamicImage, ImageBuffer, Rgba};
use moe_frame::frame_art::{analyze_frame_art, load_frame_art, HoleRect, PixelInsets};
use std::path::PathBuf;

/// 像素夹具：整图不透明，`hole` 给定时把该矩形（x, y, w, h）挖成全透明。
fn fixture(width: u32, height: u32, hole: Option<(u32, u32, u32, u32)>) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        rgba.extend_from_slice(&[120, 80, 40, 255]);
    }
    if let Some((hx, hy, hw, hh)) = hole {
        for y in hy..hy + hh {
            for x in hx..hx + hw {
                let idx = ((y * width + x) * 4) as usize;
                rgba[idx + 3] = 0;
            }
        }
    }
    rgba
}

fn set_alpha(rgba: &mut [u8], width: u32, x: u32, y: u32, alpha: u8) {
    let idx = ((y * width + x) * 4) as usize;
    rgba[idx + 3] = alpha;
}

#[test]
fn centered_hole_detected_with_symmetric_insets() {
    let rgba = fixture(100, 80, Some((30, 20, 40, 40)));
    let analysis = analyze_frame_art(100, 80, &rgba, true).unwrap();
    assert_eq!(analysis.art_size, [100, 80]);
    assert_eq!(
        analysis.hole,
        HoleRect {
            x: 30,
            y: 20,
            width: 40,
            height: 40
        }
    );
    assert_eq!(
        analysis.insets(),
        PixelInsets {
            left: 30,
            top: 20,
            right: 30,
            bottom: 20
        }
    );
}

#[test]
fn offset_hole_detected_with_asymmetric_insets() {
    // 洞偏左上但仍包含中心 (50,40)：四边内距各不相同。
    let rgba = fixture(100, 80, Some((10, 8, 60, 50)));
    let analysis = analyze_frame_art(100, 80, &rgba, true).unwrap();
    assert_eq!(
        analysis.insets(),
        PixelInsets {
            left: 10,
            top: 8,
            right: 30,
            bottom: 22
        }
    );
}

#[test]
fn polaroid_style_thick_bottom_is_kept() {
    let rgba = fixture(100, 100, Some((10, 10, 80, 50)));
    let analysis = analyze_frame_art(100, 100, &rgba, true).unwrap();
    assert_eq!(analysis.insets().bottom, 40);
    assert_eq!(analysis.insets().top, 10);
}

#[test]
fn opaque_center_is_rejected_with_reason() {
    let rgba = fixture(100, 80, None);
    let reason = analyze_frame_art(100, 80, &rgba, true).unwrap_err();
    assert!(reason.contains("中心"), "{reason}");
}

#[test]
fn missing_alpha_channel_is_rejected_with_reason() {
    let rgba = fixture(100, 80, Some((30, 20, 40, 40)));
    let reason = analyze_frame_art(100, 80, &rgba, false).unwrap_err();
    assert!(reason.contains("透明通道"), "{reason}");
}

#[test]
fn expansion_stops_at_an_opaque_speck_inside_the_transparent_area() {
    // 透明区本是 (30,20,40,40)，但 (50,25) 有一个不透明点：从中心
    // 向上扩展时，条带一旦横跨 x=50 就被该点挡住，洞顶停在 y=26。
    let mut rgba = fixture(100, 80, Some((30, 20, 40, 40)));
    set_alpha(&mut rgba, 100, 50, 25, 255);
    let analysis = analyze_frame_art(100, 80, &rgba, true).unwrap();
    assert_eq!(
        analysis.hole,
        HoleRect {
            x: 30,
            y: 26,
            width: 40,
            height: 34
        }
    );
}

#[test]
fn insets_scale_by_window_width_over_art_width() {
    let rgba = fixture(100, 80, Some((30, 20, 40, 40)));
    let analysis = analyze_frame_art(100, 80, &rgba, true).unwrap();
    let doubled = analysis.insets_at(200.0);
    assert_eq!(
        (doubled.left, doubled.top, doubled.right, doubled.bottom),
        (60.0, 40.0, 60.0, 40.0)
    );
    let halved = analysis.insets_at(50.0);
    assert_eq!(
        (halved.left, halved.top, halved.right, halved.bottom),
        (15.0, 10.0, 15.0, 10.0)
    );
}

/// 把 RGBA 夹具存成真实 PNG（供加载接缝使用）。
fn save_rgba_png(
    dir: &std::path::Path,
    name: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> PathBuf {
    let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgba.to_vec()).unwrap();
    let path = dir.join(name);
    DynamicImage::ImageRgba8(buffer).save(&path).unwrap();
    path
}

#[test]
fn load_accepts_a_png_with_a_transparent_hole() {
    let dir = temp_root("moeframe-frame-art", "load-ok");
    let rgba = fixture(100, 80, Some((30, 20, 40, 40)));
    let path = save_rgba_png(&dir, "frame.png", 100, 80, &rgba);
    let loaded = load_frame_art(&path).unwrap();
    assert_eq!(loaded.analysis.insets().left, 30);
    assert_eq!(loaded.rgba.len(), 100 * 80 * 4);
}

#[test]
fn load_rejects_a_missing_file() {
    let dir = temp_root("moeframe-frame-art", "load-missing");
    let reason = load_frame_art(&dir.join("nope.png")).unwrap_err();
    assert!(reason.contains("无法读取"), "{reason}");
}

#[test]
fn load_rejects_a_non_png_file() {
    let dir = temp_root("moeframe-frame-art", "load-jpeg");
    let rgba = fixture(100, 80, Some((30, 20, 40, 40)));
    let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(100, 80, rgba.to_vec()).unwrap();
    let path = dir.join("frame.jpg");
    DynamicImage::ImageRgba8(buffer)
        .to_rgb8()
        .save(&path)
        .unwrap();
    let reason = load_frame_art(&path).unwrap_err();
    assert!(reason.contains("PNG"), "{reason}");
}

#[test]
fn load_rejects_a_png_without_a_transparent_hole() {
    let dir = temp_root("moeframe-frame-art", "load-opaque");
    let rgba = fixture(100, 80, None);
    let path = save_rgba_png(&dir, "frame.png", 100, 80, &rgba);
    let reason = load_frame_art(&path).unwrap_err();
    assert!(reason.contains("中心"), "{reason}");
}

#[test]
fn load_rejects_a_png_without_alpha() {
    let dir = temp_root("moeframe-frame-art", "load-rgb");
    let buffer = ImageBuffer::from_fn(100, 80, |_, _| image::Rgb([10u8, 20, 30]));
    let path = dir.join("frame.png");
    DynamicImage::ImageRgb8(buffer).save(&path).unwrap();
    let reason = load_frame_art(&path).unwrap_err();
    assert!(reason.contains("透明通道"), "{reason}");
}
