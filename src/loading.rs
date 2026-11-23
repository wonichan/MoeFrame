//! 产品图片解码：按物理像素目标生成显示数据，校验格式与解码输出上限，
//! 缩放后立即释放原图。正式相框和解码验证器共用本模块；验证器可显式传入
//! Limits 并观测解码阶段。尺寸算术归 `display`，显示数据的记账与决策归 `display_data`。

use crate::display::display_size_pixels;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::{
    fmt,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

pub use crate::display::DisplayMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeLimits {
    pub max_width: u32,
    pub max_height: u32,
    pub max_alloc: u64,
}

#[derive(Debug)]
pub struct DisplayImage {
    pub path: PathBuf,
    pub source_pixels: [u32; 2],
    pub scaled_pixels: [u32; 2],
    pub display_pixels: [u32; 2],
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub path: PathBuf,
    pub reason: String,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.reason)
    }
}

impl std::error::Error for DecodeError {}

/// 解码观测点；释放事件指原图已 drop，不代表进程工作集下降。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodePhase {
    Opening,
    Decoding,
    Resizing,
    FullResolutionReleased,
    DisplayReady,
}

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
    load_image_observed(path, request, |_| {})
}

pub(crate) fn load_image_observed(
    path: &Path,
    request: LoadRequest,
    mut observe: impl FnMut(DecodePhase),
) -> Result<DisplayImage, DecodeError> {
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
    observe(DecodePhase::Opening);
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
    let [dw, dh] = display_size_pixels(request.mode, request.target_pixels, [width, height])
        .map_err(|e| fail(e.to_string()))?;
    let coverage_bytes = u64::from(dw)
        .checked_mul(u64::from(dh))
        .and_then(|n| n.checked_mul(u64::from(decoder.color_type().bytes_per_pixel().max(4))));
    if !matches!(coverage_bytes, Some(n) if n <= request.limits.max_alloc && usize::try_from(n).is_ok())
    {
        return Err(fail("coverage buffer exceeds max_alloc".into()));
    }
    observe(DecodePhase::Decoding);
    let full = decoder_call(path, || DynamicImage::from_decoder(decoder))?;
    observe(DecodePhase::Resizing);
    let display = full.thumbnail_exact(dw, dh);
    drop(full);
    observe(DecodePhase::FullResolutionReleased);
    let display = match request.mode {
        DisplayMode::Fit => display,
        DisplayMode::Fill => display.crop_imm((dw - tw) / 2, (dh - th) / 2, tw, th),
    };
    let display_pixels = [display.width(), display.height()];
    let rgba = display.into_rgba8().into_raw();
    observe(DecodePhase::DisplayReady);
    Ok(DisplayImage {
        path: path.to_owned(),
        source_pixels: [width, height],
        scaled_pixels: [dw, dh],
        display_pixels,
        rgba,
    })
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
