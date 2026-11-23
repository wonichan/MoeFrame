//! 常驻的解码接缝等价测试（深挖工单 06/07）。验证 adapter 和正式解码入口
//! 接同一组请求，对比尺寸、像素和拒绝原因。每组都钉住接受或拒绝方向，
//! 避免共享实现同时退化而未被发现。绘制保留小数，解码按整数像素取整。

mod common;

use common::temp_root;
use image::{DynamicImage, ImageBuffer, Rgb};
use moe_frame::decode_validation::load_display;
use moe_frame::display::display_size;
use moe_frame::loading::{
    load_image, DecodeError, DecodeLimits, DisplayImage, DisplayMode, LoadRequest,
};
use std::path::{Path, PathBuf};

const GENEROUS: DecodeLimits = DecodeLimits {
    max_width: 12_000,
    max_height: 12_000,
    max_alloc: 96 * 1024 * 1024,
};
const TIGHT_DIMS: DecodeLimits = DecodeLimits {
    max_width: 100,
    max_height: 100,
    max_alloc: 96 * 1024 * 1024,
};

/// 生成一张渐变测试图并按扩展名保存（格式由扩展名决定）。
fn save_sample(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let buffer = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 233) as u8])
    });
    let path = dir.join(name);
    DynamicImage::ImageRgb8(buffer).save(&path).unwrap();
    path
}

/// 同一样本、同一请求分别走两个公开入口。
fn both(
    path: &Path,
    target: [u32; 2],
    mode: DisplayMode,
    limits: DecodeLimits,
) -> (
    Result<DisplayImage, DecodeError>,
    Result<DisplayImage, DecodeError>,
) {
    let request = LoadRequest {
        target_pixels: target,
        mode,
        limits,
    };
    (
        load_display(path, request, |_| {}),
        load_image(path, request),
    )
}

fn assert_both_accept(path: &Path, target: [u32; 2], mode: DisplayMode, limits: DecodeLimits) {
    let (validation, product) = both(path, target, mode, limits);
    let ctx = format!(
        "{} target={target:?} mode={mode:?} limits={limits:?}",
        path.display()
    );
    let v = validation.unwrap_or_else(|error| panic!("validator must accept {ctx}: {error}"));
    let p = product.unwrap_or_else(|error| panic!("product must accept {ctx}: {error}"));
    assert_images_equal(&v, &p, &ctx);
}

fn assert_images_equal(validation: &DisplayImage, product: &DisplayImage, ctx: &str) {
    assert_eq!(validation.path, product.path, "path: {ctx}");
    assert_eq!(
        validation.source_pixels, product.source_pixels,
        "source_pixels: {ctx}"
    );
    assert_eq!(
        validation.scaled_pixels, product.scaled_pixels,
        "scaled_pixels: {ctx}"
    );
    assert_eq!(
        validation.display_pixels, product.display_pixels,
        "display_pixels: {ctx}"
    );
    assert_eq!(validation.rgba, product.rgba, "rgba bytes: {ctx}");
}

#[test]
fn formats_and_aspects_agree_across_limits() {
    let dir = temp_root("decode-equivalence", "formats");
    let samples = [
        save_sample(&dir, "wide.png", 64, 48),
        save_sample(&dir, "tall.jpg", 48, 64),
        save_sample(&dir, "square.bmp", 50, 50),
        save_sample(&dir, "wide.webp", 64, 48),
        save_sample(&dir, "tall.gif", 48, 64),
    ];
    for path in &samples {
        for mode in [DisplayMode::Fit, DisplayMode::Fill] {
            for target in [[100, 100], [72, 40], [40, 72]] {
                assert_both_accept(path, target, mode, GENEROUS);
                assert_both_accept(path, target, mode, TIGHT_DIMS);
            }
        }
    }
}

#[test]
fn fill_crop_boundary_sizes_agree() {
    let dir = temp_root("decode-equivalence", "fill-boundary");
    // 源比例在目标比例（100x100）两侧与恰好相等处，覆盖 Fill 两分支与等号边界。
    for (name, w, h) in [
        ("land.png", 200, 100),
        ("port.png", 100, 200),
        ("even.png", 100, 100),
        ("just_wide.png", 101, 100),
        ("just_tall.png", 100, 101),
        ("almost.png", 199, 100),
    ] {
        let path = save_sample(&dir, name, w, h);
        for mode in [DisplayMode::Fit, DisplayMode::Fill] {
            assert_both_accept(&path, [100, 100], mode, GENEROUS);
            assert_both_accept(&path, [37, 73], mode, GENEROUS);
        }
    }
}

/// 同一实现下，两端都必须拒绝，并给出相同的路径和原因。
fn assert_both_reject(path: &Path, target: [u32; 2], mode: DisplayMode, limits: DecodeLimits) {
    let (validation, product) = both(path, target, mode, limits);
    let ctx = format!(
        "{} target={target:?} mode={mode:?} limits={limits:?}",
        path.display()
    );
    let Err(v) = validation else {
        panic!("validator must reject: {ctx}");
    };
    let Err(p) = product else {
        panic!("product must reject: {ctx}");
    };
    assert_eq!(v, p, "rejections: {ctx}");
    assert_eq!(v.path, path);
    assert!(!v.reason.is_empty(), "missing rejection reason: {ctx}");
}

#[test]
fn over_limit_and_corrupt_inputs_agree() {
    let dir = temp_root("decode-equivalence", "rejects");
    let wide = save_sample(&dir, "over_width.png", 300, 50);
    let tall = save_sample(&dir, "over_height.png", 50, 300);
    let big = save_sample(&dir, "big.bmp", 400, 400);
    let extreme = save_sample(&dir, "extreme.png", 2000, 20);
    let good = save_sample(&dir, "good.png", 64, 48);
    let corrupt = dir.join("corrupt.png");
    std::fs::write(&corrupt, b"not an image").unwrap();
    let raw = std::fs::read(&good).unwrap();
    let truncated = dir.join("truncated.png");
    // 截到 35 字节：签名与 IHDR 已过、图像数据未至，两侧都必然解码失败。
    std::fs::write(&truncated, &raw[..35]).unwrap();
    let disguised = dir.join("disguised.png");
    save_sample(&dir, "real.jpg", 64, 48);
    std::fs::copy(dir.join("real.jpg"), &disguised).unwrap();

    for mode in [DisplayMode::Fit, DisplayMode::Fill] {
        // 源宽/源高超限：解码器 Limits 在两侧同口径拒绝。
        assert_both_reject(&wide, [100, 100], mode, TIGHT_DIMS);
        assert_both_reject(&tall, [100, 100], mode, TIGHT_DIMS);
        // BMP 的解码输出预留超 max_alloc，两侧同为拒绝。
        let alloc_tight = DecodeLimits {
            max_width: 1_000,
            max_height: 1_000,
            max_alloc: 100_000,
        };
        assert_both_reject(&big, [100, 100], mode, alloc_tight);
        // 填充覆盖缓冲超 max_alloc；同一文件在适应模式下可以显示。
        let coverage_tight = DecodeLimits {
            max_width: 12_000,
            max_height: 12_000,
            max_alloc: 1_000_000,
        };
        if mode == DisplayMode::Fill {
            assert_both_reject(&extreme, [100, 100], mode, coverage_tight);
        } else {
            assert_both_accept(&extreme, [100, 100], mode, coverage_tight);
        }
        // 损坏、截断、扩展名与文件头不一致：两侧同为拒绝。
        assert_both_reject(&corrupt, [100, 100], mode, GENEROUS);
        assert_both_reject(&truncated, [100, 100], mode, GENEROUS);
        assert_both_reject(&disguised, [100, 100], mode, GENEROUS);
    }
}

#[test]
fn guard_rejections_agree() {
    let dir = temp_root("decode-equivalence", "guards");
    let good = save_sample(&dir, "good.png", 64, 48);
    for mode in [DisplayMode::Fit, DisplayMode::Fill] {
        assert_both_reject(&good, [0, 100], mode, GENEROUS);
        assert_both_reject(&good, [100, 0], mode, GENEROUS);
        let zero_limits = DecodeLimits {
            max_width: 0,
            max_height: 100,
            max_alloc: 1024,
        };
        assert_both_reject(&good, [100, 100], mode, zero_limits);
    }
}

/// Limits constrain decoded output, not the bytes in the encoded file.
#[test]
fn encoded_length_above_max_alloc_does_not_reject_bounded_output() {
    let dir = temp_root("decode-equivalence", "encoded-length");
    let padded = save_sample(&dir, "padded.png", 8, 8);
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&padded)
            .unwrap();
        file.write_all(&[0; 4096]).unwrap();
    }
    // max_alloc 取 2048：高于显示覆盖缓冲（16x16x4=1024）与解码预留，
    // 低于填充后的编码长度（约 4.3KB）——只有编码预检这一道门能拦住它。
    let mid_alloc = DecodeLimits {
        max_width: 12_000,
        max_height: 12_000,
        max_alloc: 2_048,
    };
    assert!(std::fs::metadata(&padded).unwrap().len() > mid_alloc.max_alloc);
    let (validation, product) = both(&padded, [16, 16], DisplayMode::Fit, mid_alloc);
    let validation = validation.expect("validator must accept: decoded output is within limits");
    let product = product.expect("product must accept: decoded output is within limits");
    assert_eq!(validation.source_pixels, [8, 8]);
    assert_eq!(validation.scaled_pixels, [16, 16]);
    assert_eq!(validation.display_pixels, [16, 16]);
    assert_images_equal(&validation, &product, "encoded length above max_alloc");
}

/// 同一个缩放比例，绘制保留小数，解码按模式取整。
#[test]
fn decode_integer_sizes_match_painting_float_sizes_within_rounding() {
    let dir = temp_root("decode-equivalence", "sizes");
    let sources = [
        (3, 2),
        (2, 3),
        (100, 99),
        (99, 100),
        (640, 427),
        (427, 640),
        (1000, 10),
        (10, 1000),
        (50, 50),
    ];
    let targets = [[100, 100], [480, 320], [320, 480], [37, 73]];
    for (w, h) in sources {
        let path = save_sample(&dir, &format!("s{w}x{h}.png"), w, h);
        for target in targets {
            for mode in [DisplayMode::Fit, DisplayMode::Fill] {
                let (validation, product) = both(&path, target, mode, GENEROUS);
                let v = validation.unwrap();
                let p = product.unwrap();
                assert_eq!(v.scaled_pixels, p.scaled_pixels);
                let int_size = v.scaled_pixels;
                let float_size = display_size(
                    mode,
                    [target[0] as f32, target[1] as f32],
                    [w as f32, h as f32],
                );
                // 驱动边：目标边即缩放边，两侧只差 f32 乘除的表示误差
                // （<0.01px）；派生边：按模式校验取整方向，同样容忍表示误差。
                // 源与目标恰同比例时两边都等于目标，任取一边作驱动边，
                // 另一边按派生边校验（差为 0，落在归类内）。
                let (driving, derived) = if int_size[0] == target[0] {
                    (0, 1)
                } else {
                    (1, 0)
                };
                let driving_diff = (int_size[driving] as f32 - float_size[driving]).abs();
                assert!(
                    driving_diff < 0.01,
                    "driving side beyond float error: source={w}x{h} target={target:?} \
                     mode={mode:?} int={int_size:?} float={float_size:?}"
                );
                let diff = match mode {
                    DisplayMode::Fit => float_size[derived] - int_size[derived] as f32,
                    DisplayMode::Fill => int_size[derived] as f32 - float_size[derived],
                };
                let clamped =
                    mode == DisplayMode::Fit && float_size[derived] < 1.0 && int_size[derived] == 1;
                assert!(
                    clamped || (-0.01..1.0).contains(&diff),
                    "rounding out of class: source={w}x{h} target={target:?} mode={mode:?} \
                     int={int_size:?} float={float_size:?} diff={diff}"
                );
            }
        }
    }
}
