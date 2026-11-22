//! 深挖工单 06：双解码管线交叉等价测试。
//!
//! 同一组样本、同一组请求参数，分别经产品管线（`loading::load_image`）与
//! 工单 09 验证管线（`decode_validation::load_display`）加载，断言接受/拒绝
//! 结果与输出尺寸（源尺寸、缩放尺寸、显示尺寸、RGBA 字节）一致。已知的
//! 有意差异不在等价断言内，单独在
//! `encoded_precheck_is_the_known_difference` 中以「差异存在且形态如此」
//! 的方式钉住：验证接缝按编码字节长度预检拒绝，产品管线不预检、
//! 按解码输出判定。拒绝原因的文字措辞差异（如 "experimental" 有无）
//! 同样属已知差异：等价断言只断言两侧同为拒绝，不断言原因文本相等；
//! 已知差异测试则反过来把验证侧的原因原文钉死，以记录差异的形态。

mod common;

use common::temp_root;
use image::{DynamicImage, ImageBuffer, Rgb};
use moe_frame::decode_validation::{
    load_display, DecodeError, DecodeLimits, DecodeRequest, DisplayImage, DisplayMode,
};
use moe_frame::display::display_size;
use moe_frame::loading::{load_image, LoadRequest};
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

/// 同一样本、同一请求分别走两条管线。
fn both(
    path: &Path,
    target: [u32; 2],
    mode: DisplayMode,
    limits: DecodeLimits,
) -> (
    Result<DisplayImage, DecodeError>,
    Result<DisplayImage, DecodeError>,
) {
    let validation = load_display(
        path,
        DecodeRequest {
            target_pixels: target,
            mode,
            limits,
        },
        |_| {},
    );
    let product = load_image(
        path,
        LoadRequest {
            target_pixels: target,
            mode,
            limits,
        },
    );
    (validation, product)
}

/// 等价断言：接受/拒绝一致；接受时四项输出全等（含 RGBA 字节）；
/// 拒绝时只断言两侧同为拒绝（原因措辞属已知差异，不在此断言）。
fn assert_equivalent(path: &Path, target: [u32; 2], mode: DisplayMode, limits: DecodeLimits) {
    let (validation, product) = both(path, target, mode, limits);
    let ctx = format!(
        "{} target={target:?} mode={mode:?} limits={limits:?}",
        path.display()
    );
    match (validation, product) {
        (Ok(v), Ok(p)) => {
            assert_eq!(v.source_pixels, p.source_pixels, "source_pixels: {ctx}");
            assert_eq!(v.scaled_pixels, p.scaled_pixels, "scaled_pixels: {ctx}");
            assert_eq!(v.display_pixels, p.display_pixels, "display_pixels: {ctx}");
            assert_eq!(v.rgba, p.rgba, "rgba bytes: {ctx}");
        }
        (Err(v), Err(p)) => {
            assert!(
                !v.reason.is_empty() && !p.reason.is_empty(),
                "reasons: {ctx}"
            );
        }
        (v, p) => panic!(
            "accept/reject divergence: {ctx}: validation ok={} product ok={}",
            v.is_ok(),
            p.is_ok()
        ),
    }
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
                assert_equivalent(path, target, mode, GENEROUS);
                assert_equivalent(path, target, mode, TIGHT_DIMS);
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
            assert_equivalent(&path, [100, 100], mode, GENEROUS);
            assert_equivalent(&path, [37, 73], mode, GENEROUS);
        }
    }
}

/// 拒绝方向钉死：两侧必须同为拒绝（原因文本属已知差异，不断言相等）。
fn assert_both_reject(path: &Path, target: [u32; 2], mode: DisplayMode, limits: DecodeLimits) {
    let (validation, product) = both(path, target, mode, limits);
    let ctx = format!(
        "{} target={target:?} mode={mode:?} limits={limits:?}",
        path.display()
    );
    assert!(validation.is_err(), "validation must reject: {ctx}");
    assert!(product.is_err(), "product must reject: {ctx}");
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
        // 解码输出预留超 max_alloc：BMP 编码与原始字节同量级，两侧同为拒绝
        // （验证侧可能先在编码预检拒绝，原因归类不同、结果同为拒绝）。
        let alloc_tight = DecodeLimits {
            max_width: 1_000,
            max_height: 1_000,
            max_alloc: 100_000,
        };
        assert_both_reject(&big, [100, 100], mode, alloc_tight);
        // 填充覆盖缓冲超 max_alloc：预留可过、覆盖字节超限，两侧同为拒绝；
        // 适应模式下同一文件缩放后覆盖很小，两侧同为接受（等价断言覆盖）。
        let coverage_tight = DecodeLimits {
            max_width: 12_000,
            max_height: 12_000,
            max_alloc: 1_000_000,
        };
        if mode == DisplayMode::Fill {
            assert_both_reject(&extreme, [100, 100], mode, coverage_tight);
        } else {
            assert_equivalent(&extreme, [100, 100], mode, coverage_tight);
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

/// 已知差异（清单首项）：编码字节预检。同一文件、同一 Limits——验证接缝
/// 按编码长度（文件尾部填充计入）超过 max_alloc 拒绝，产品管线不预检、
/// 解码输出在限内则接受。差异以这个形态被钉住，而不是断言一致。
#[test]
fn encoded_precheck_is_the_known_difference() {
    let dir = temp_root("decode-equivalence", "known-diff");
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
    let (validation, product) = both(&padded, [16, 16], DisplayMode::Fit, mid_alloc);
    let error = validation.expect_err("validation seam must reject on encoded length");
    assert_eq!(error.reason, "encoded file exceeds experimental max_alloc");
    let image = product.expect("product pipeline must accept: decode output is within limits");
    assert_eq!(image.source_pixels, [8, 8]);
    assert_eq!(image.display_pixels, [16, 16]);

    // 同一文件在宽松 Limits 下两侧同为接受且输出全等：填充本身不构成差异。
    assert_equivalent(&padded, [100, 100], DisplayMode::Fit, GENEROUS);
    assert_equivalent(&padded, [100, 100], DisplayMode::Fill, GENEROUS);
}

/// 尺寸计算三份实现的交叉校验：解码侧两份整数版先由等价断言保证彼此
/// 一致（scaled_pixels），再与绘制侧浮点版（display::display_size）对照。
/// 取整归类：驱动边两侧恒等；派生边适应为向下取整（差在 [0,1)，
/// 浮点结果小于 1 时整数版钳到 1），填充为向上取整（差在 [0,1)）。
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
