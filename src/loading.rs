//! 正式相框的图片加载（工单 10）：后台解码一张图并缩放到窗口物理像素需求。
//!
//! 与工单 09 验证接缝 `decode_validation::load_display` 的关系：验证接缝保持冻结作为实测证据；
//! 本模块是产品路径，差异有二——(1) 使用用户 2026-11-20 确认的产品 Limits，不再是实验输入；
//! (2) 不做编码文件大小预检，超限由 image 的 Limits 在解码输出侧拒绝（父规格：「以 Limits
//! 拒绝过大输出」）。其余规则一致：扩展名与文件头必须一致、全分辨率数据只短暂持有、
//! 适应/填充的尺寸计算相同、解码器 panic 转为带路径的拒绝。

use crate::decode_validation::{DecodeError, DecodeLimits, DisplayImage, DisplayMode};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::{fs::File, io::BufReader, path::Path};

/// 用户 2026-11-20 确认的产品 Limits（见 `.agents/notes/implemented/process/2026-11-20-product-limits-peak-budget.md`）。
pub const PRODUCT_LIMITS: DecodeLimits = DecodeLimits {
    max_width: 12_000,
    max_height: 12_000,
    max_alloc: 96 * 1024 * 1024,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadRequest {
    /// 窗口物理像素需求 [宽, 高]。
    pub target_pixels: [u32; 2],
    pub mode: DisplayMode,
    pub limits: DecodeLimits,
}

impl LoadRequest {
    pub fn product(target_pixels: [u32; 2], mode: DisplayMode) -> Self {
        Self {
            target_pixels,
            mode,
            limits: PRODUCT_LIMITS,
        }
    }
}

/// 解码 `path` 并缩放为显示数据；返回的 RGBA 仅为显示尺寸，全分辨率图不离开本函数。
pub fn load_image(path: &Path, request: LoadRequest) -> Result<DisplayImage, DecodeError> {
    let fail = |reason: String| DecodeError {
        path: path.to_owned(),
        reason,
    };
    if cfg!(panic = "abort") {
        return Err(fail(
            "decoder recovery requires panic=unwind; abort/OOM cannot be caught".into(),
        ));
    }
    if request.target_pixels.contains(&0)
        || request.limits.max_width == 0
        || request.limits.max_height == 0
        || request.limits.max_alloc == 0
    {
        return Err(fail("physical target and Limits must be positive".into()));
    }
    let file = File::open(path).map_err(|e| fail(e.to_string()))?;
    let format = ImageFormat::from_path(path).map_err(|e| fail(e.to_string()))?;
    let mut reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(|e| fail(e.to_string()))?;
    let header_format = reader
        .format()
        .ok_or_else(|| fail("unrecognized image header".into()))?;
    if header_format != format {
        return Err(fail(format!(
            "extension/header mismatch: extension={format:?}; header={header_format:?}"
        )));
    }
    let mut limits = image_limits(request.limits);
    reader.limits(limits.clone());
    let mut decoder = decoder_call(path, || reader.into_decoder())?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return Err(fail("zero source dimensions".into()));
    }
    // into_decoder does not reserve the output; mirror ImageReader::decode before from_decoder.
    limits
        .reserve(decoder.total_bytes())
        .map_err(|e| fail(e.to_string()))?;
    decoder_call(path, || decoder.set_limits(limits))?;
    let [tw, th] = request.target_pixels;
    let [dw, dh] =
        display_size([width, height], request.target_pixels, request.mode).map_err(&fail)?;
    let coverage_bytes = u64::from(dw)
        .checked_mul(u64::from(dh))
        .and_then(|n| n.checked_mul(u64::from(decoder.color_type().bytes_per_pixel().max(4))));
    if !matches!(coverage_bytes, Some(n) if n <= request.limits.max_alloc && usize::try_from(n).is_ok())
    {
        return Err(fail("coverage buffer exceeds max_alloc".into()));
    }
    let full = decoder_call(path, || DynamicImage::from_decoder(decoder))?;
    let display = full.thumbnail_exact(dw, dh);
    drop(full);
    let display = match request.mode {
        DisplayMode::Fit => display,
        DisplayMode::Fill => display.crop_imm((dw - tw) / 2, (dh - th) / 2, tw, th),
    };
    let display_pixels = [display.width(), display.height()];
    let rgba = display.into_rgba8().into_raw();
    Ok(DisplayImage {
        path: path.to_owned(),
        source_pixels: [width, height],
        scaled_pixels: [dw, dh],
        display_pixels,
        rgba,
    })
}

/// 按显示模式计算缩放后（填充时为裁剪前覆盖）尺寸。
fn display_size(source: [u32; 2], target: [u32; 2], mode: DisplayMode) -> Result<[u32; 2], String> {
    let [width, height] = source;
    let [tw, th] = target;
    match mode {
        DisplayMode::Fit
            if u64::from(tw) * u64::from(height) <= u64::from(th) * u64::from(width) =>
        {
            Ok([
                tw,
                (u64::from(height) * u64::from(tw) / u64::from(width)).max(1) as u32,
            ])
        }
        DisplayMode::Fit => Ok([
            (u64::from(width) * u64::from(th) / u64::from(height)).max(1) as u32,
            th,
        ]),
        DisplayMode::Fill
            if u64::from(tw) * u64::from(height) >= u64::from(th) * u64::from(width) =>
        {
            Ok([
                tw,
                u32::try_from((u64::from(height) * u64::from(tw)).div_ceil(u64::from(width)))
                    .map_err(|e| e.to_string())?,
            ])
        }
        DisplayMode::Fill => Ok([
            u32::try_from((u64::from(width) * u64::from(th)).div_ceil(u64::from(height)))
                .map_err(|e| e.to_string())?,
            th,
        ]),
    }
}

fn image_limits(limits: DecodeLimits) -> Limits {
    let mut image_limits = Limits::no_limits();
    image_limits.max_image_width = Some(limits.max_width);
    image_limits.max_image_height = Some(limits.max_height);
    image_limits.max_alloc = Some(limits.max_alloc);
    image_limits
}

// Catch only decoder-owned operations, not host bugs. This does not recover abort/OOM.
fn decoder_call<T>(
    path: &Path,
    decode: impl FnOnce() -> image::ImageResult<T>,
) -> Result<T, DecodeError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(decode))
        .map_err(|payload| {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic payload");
            DecodeError {
                path: path.to_owned(),
                reason: format!("decoder panic: {message}"),
            }
        })?
        .map_err(|error| DecodeError {
            path: path.to_owned(),
            reason: error.to_string(),
        })
}

/// 一次加载的结果：第一张可显示图（若有）与沿途跳过的候选及原因。
pub struct LoadOutcome {
    pub image: Option<DisplayImage>,
    pub skipped: Vec<DecodeError>,
}

/// 按候选顺序寻找第一张可显示图；每个候选至多尝试一次，全失败不循环。
pub fn load_first_available(
    candidates: &[std::path::PathBuf],
    request: LoadRequest,
) -> LoadOutcome {
    let mut skipped = Vec::new();
    for path in candidates {
        match load_image(path, request) {
            Ok(image) => {
                return LoadOutcome {
                    image: Some(image),
                    skipped,
                }
            }
            Err(error) => skipped.push(error),
        }
    }
    LoadOutcome {
        image: None,
        skipped,
    }
}

/// 把跳过记录追加到日志文件（父规格固定位置 `moe_frame.log`）；一行一条，含路径与原因。
pub fn append_skip_log(log_path: &Path, skipped: &[DecodeError]) -> std::io::Result<()> {
    if skipped.is_empty() {
        return Ok(());
    }
    use std::io::Write;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    for error in skipped {
        writeln!(
            file,
            "{unix_ms}\tskipped\t{}\t{}",
            error.path.display(),
            error.reason
        )?;
    }
    Ok(())
}
