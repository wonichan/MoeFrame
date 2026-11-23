//! 工单 10：产品加载路径的真实文件测试（临时目录、小型真实图片）。
mod common;

use moe_frame::loading::{
    append_skip_log, load_first_available, load_image, DecodeLimits, DisplayMode, LoadRequest,
    PRODUCT_LIMITS,
};
use std::{fs, path::PathBuf};

fn save_solid(path: &PathBuf, width: u32, height: u32) {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([120, 40, 200]));
    image.save(path).unwrap();
}

fn request(target: [u32; 2], mode: DisplayMode) -> LoadRequest {
    LoadRequest::product(target, mode)
}

#[test]
fn product_limits_are_the_confirmed_values() {
    assert_eq!(PRODUCT_LIMITS.max_width, 12_000);
    assert_eq!(PRODUCT_LIMITS.max_height, 12_000);
    assert_eq!(PRODUCT_LIMITS.max_alloc, 96 * 1024 * 1024);
}

#[test]
fn fit_scales_the_whole_image_into_the_target() {
    let root = common::temp_root("moeframe-loading-test", "fit");
    let path = root.join("wide.png");
    save_solid(&path, 100, 50);
    let image = load_image(&path, request([40, 40], DisplayMode::Fit)).unwrap();
    assert_eq!(image.source_pixels, [100, 50]);
    assert_eq!(image.display_pixels, [40, 20]);
    assert_eq!(image.rgba.len(), 40 * 20 * 4);
}

#[test]
fn fill_covers_the_target_and_crops_the_overflow() {
    let root = common::temp_root("moeframe-loading-test", "fill");
    let path = root.join("wide.png");
    save_solid(&path, 100, 50);
    let image = load_image(&path, request([40, 40], DisplayMode::Fill)).unwrap();
    assert_eq!(image.scaled_pixels, [80, 40]);
    assert_eq!(image.display_pixels, [40, 40]);
    assert_eq!(image.rgba.len(), 40 * 40 * 4);
}

#[test]
fn every_supported_format_loads_including_gif_first_frame() {
    let root = common::temp_root("moeframe-loading-test", "formats");
    for ext in ["jpg", "jpeg", "png", "webp", "bmp", "gif"] {
        let path = root.join(format!("sample.{ext}"));
        save_solid(&path, 24, 16);
        let image = load_image(&path, request([24, 16], DisplayMode::Fit))
            .unwrap_or_else(|e| panic!("{ext} should load: {e}"));
        assert_eq!(image.display_pixels, [24, 16], "{ext}");
    }
}

#[test]
fn corrupt_and_disguised_files_are_rejected_with_the_path() {
    let root = common::temp_root("moeframe-loading-test", "bad");
    let corrupt = root.join("broken.png");
    fs::write(&corrupt, b"this is not a png").unwrap();
    let error = load_image(&corrupt, request([40, 40], DisplayMode::Fit)).unwrap_err();
    assert_eq!(error.path, corrupt);

    let disguised = root.join("actually-png.jpg");
    save_solid(&root.join("real.png"), 16, 16);
    fs::copy(root.join("real.png"), &disguised).unwrap();
    let error = load_image(&disguised, request([40, 40], DisplayMode::Fit)).unwrap_err();
    assert!(
        error.reason.contains("mismatch"),
        "disguised file should report the mismatch: {}",
        error.reason
    );
}

#[test]
fn over_limit_images_are_rejected_by_output_limits() {
    let root = common::temp_root("moeframe-loading-test", "limits");
    let path = root.join("big.png");
    save_solid(&path, 200, 200);
    let tight = LoadRequest {
        target_pixels: [40, 40],
        mode: DisplayMode::Fit,
        limits: DecodeLimits {
            max_width: 12_000,
            max_height: 12_000,
            max_alloc: 1_000,
        },
    };
    assert!(load_image(&path, tight).is_err());
    let narrow = LoadRequest {
        target_pixels: [40, 40],
        mode: DisplayMode::Fit,
        limits: DecodeLimits {
            max_width: 100,
            max_height: 12_000,
            max_alloc: 96 * 1024 * 1024,
        },
    };
    assert!(load_image(&path, narrow).is_err());
    // The same image loads fine under the product limits.
    assert!(load_image(&path, request([40, 40], DisplayMode::Fit)).is_ok());
}

#[test]
fn first_available_skips_failures_and_reports_them() {
    let root = common::temp_root("moeframe-loading-test", "first");
    let bad = root.join("bad.png");
    fs::write(&bad, b"junk").unwrap();
    let good = root.join("good.png");
    save_solid(&good, 20, 10);
    let outcome = load_first_available(
        &[bad.clone(), good.clone()],
        request([20, 20], DisplayMode::Fit),
    );
    assert_eq!(outcome.image.unwrap().path, good);
    assert_eq!(outcome.skipped.len(), 1);
    assert_eq!(outcome.skipped[0].path, bad);

    let outcome = load_first_available(
        std::slice::from_ref(&bad),
        request([20, 20], DisplayMode::Fit),
    );
    assert!(outcome.image.is_none());
    assert_eq!(outcome.skipped.len(), 1);

    let outcome = load_first_available(&[], request([20, 20], DisplayMode::Fit));
    assert!(outcome.image.is_none());
    assert!(outcome.skipped.is_empty());
}

#[test]
fn skip_log_appends_path_and_reason_lines() {
    let root = common::temp_root("moeframe-loading-test", "log");
    let bad = root.join("bad.png");
    fs::write(&bad, b"junk").unwrap();
    let outcome = load_first_available(&[bad], request([20, 20], DisplayMode::Fit));
    let log = root.join("moe_frame.log");
    append_skip_log(&log, &outcome.skipped).unwrap();
    append_skip_log(&log, &outcome.skipped).unwrap();
    let text = fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "each call appends: {text}");
    assert!(lines[0].contains("bad.png"), "{text}");
    assert!(lines[0].contains("skipped"), "{text}");
}
