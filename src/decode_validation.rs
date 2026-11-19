//! 工单 09 的真实文件加载接缝；实验 Limits 没有产品默认值。

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::{
    fmt,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    Fit,
    Fill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeLimits {
    pub max_width: u32,
    pub max_height: u32,
    pub max_alloc: u64,
}
impl DecodeLimits {
    fn image_limits(self) -> Limits {
        let mut limits = Limits::no_limits();
        limits.max_image_width = Some(self.max_width);
        limits.max_image_height = Some(self.max_height);
        limits.max_alloc = Some(self.max_alloc);
        limits
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeRequest {
    pub target_pixels: [u32; 2],
    pub mode: DisplayMode,
    pub limits: DecodeLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodePhase {
    Opening,
    Decoding,
    Resizing,
    FullResolutionReleased,
    DisplayReady,
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
impl DecodeError {
    fn new(path: &Path, reason: impl fmt::Display) -> Self {
        Self {
            path: path.to_owned(),
            reason: reason.to_string(),
        }
    }
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.reason)
    }
}
impl std::error::Error for DecodeError {}

/// 返回的 RGBA 仅为显示尺寸，原尺寸 DynamicImage 不离开此入口。
pub fn load_display(
    path: &Path,
    request: DecodeRequest,
    mut observe: impl FnMut(DecodePhase),
) -> Result<DisplayImage, DecodeError> {
    let fail = |reason: String| DecodeError::new(path, reason);
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
        return Err(fail(
            "physical target and experimental Limits must be positive".into(),
        ));
    }
    observe(DecodePhase::Opening);
    let file = File::open(path).map_err(|e| fail(e.to_string()))?;
    let encoded_bytes = file.metadata().map_err(|e| fail(e.to_string()))?.len();
    if encoded_bytes > request.limits.max_alloc {
        return Err(fail("encoded file exceeds experimental max_alloc".into()));
    }
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
    let mut limits = request.limits.image_limits();
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
    let [dw, dh] = match request.mode {
        DisplayMode::Fit
            if u64::from(tw) * u64::from(height) <= u64::from(th) * u64::from(width) =>
        {
            [
                tw,
                (u64::from(height) * u64::from(tw) / u64::from(width)).max(1) as u32,
            ]
        }
        DisplayMode::Fit => [
            (u64::from(width) * u64::from(th) / u64::from(height)).max(1) as u32,
            th,
        ],
        DisplayMode::Fill
            if u64::from(tw) * u64::from(height) >= u64::from(th) * u64::from(width) =>
        {
            [
                tw,
                u32::try_from((u64::from(height) * u64::from(tw)).div_ceil(u64::from(width)))
                    .map_err(|e| fail(e.to_string()))?,
            ]
        }
        DisplayMode::Fill => [
            u32::try_from((u64::from(width) * u64::from(th)).div_ceil(u64::from(height)))
                .map_err(|e| fail(e.to_string()))?,
            th,
        ],
    };
    let coverage_bytes = u64::from(dw)
        .checked_mul(u64::from(dh))
        .and_then(|n| n.checked_mul(u64::from(decoder.color_type().bytes_per_pixel().max(4))));
    if !matches!(coverage_bytes, Some(n) if n <= request.limits.max_alloc && usize::try_from(n).is_ok())
    {
        return Err(fail(
            "coverage buffer exceeds experimental max_alloc".into(),
        ));
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

// Catch only decoder-owned operations, not host observer bugs. This does not recover abort/OOM.
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
            DecodeError::new(path, format!("decoder panic: {message}"))
        })?
        .map_err(|error| DecodeError::new(path, error))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeRole {
    Current,
    Next,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CycleEvent {
    Phase {
        path: PathBuf,
        role: DecodeRole,
        phase: DecodePhase,
    },
    Prepared {
        path: PathBuf,
        role: DecodeRole,
        source_pixels: [u32; 2],
        scaled_pixels: [u32; 2],
        display_pixels: [u32; 2],
        rgba_bytes: usize,
    },
    Rejected {
        error: DecodeError,
        role: DecodeRole,
    },
    Retired {
        path: PathBuf,
        other_owners_at_retirement: usize,
    },
}

/// 快照只共享当前/下一张显示数据，不包含原尺寸图或历史缓存。
#[derive(Clone, Debug, Default)]
pub struct CycleSnapshot {
    pub current: Option<Arc<DisplayImage>>,
    pub next: Option<Arc<DisplayImage>>,
}
impl CycleSnapshot {
    pub fn resident_rgba_bytes(&self) -> usize {
        self.current
            .iter()
            .chain(self.next.iter())
            .map(|image| image.rgba.len())
            .sum()
    }
}

/// 真实轮播接缝；宿主在单个工作线程调用，快照可送给 GUI。
pub struct ValidationCycle {
    paths: Vec<PathBuf>,
    cursor: usize,
    request: DecodeRequest,
    current: Option<Arc<DisplayImage>>,
    next: Option<Arc<DisplayImage>>,
}
impl ValidationCycle {
    pub fn new(paths: Vec<PathBuf>, request: DecodeRequest) -> Self {
        Self {
            paths,
            cursor: 0,
            request,
            current: None,
            next: None,
        }
    }

    pub fn snapshot(&self) -> CycleSnapshot {
        CycleSnapshot {
            current: self.current.clone(),
            next: self.next.clone(),
        }
    }

    pub fn prime(&mut self, mut observe: impl FnMut(CycleEvent)) -> CycleSnapshot {
        if self.current.is_none() {
            self.current = self.load_available(DecodeRole::Current, &mut observe);
        }
        if self.current.is_some() && self.next.is_none() {
            self.next = self.load_available(DecodeRole::Next, &mut observe);
        }
        self.snapshot()
    }

    pub fn advance(&mut self, mut observe: impl FnMut(CycleEvent)) -> CycleSnapshot {
        Self::retire(&mut self.current, &mut observe);
        self.current = self.next.take();
        if self.current.is_none() {
            self.current = self.load_available(DecodeRole::Current, &mut observe);
        }
        if self.current.is_some() {
            self.next = self.load_available(DecodeRole::Next, &mut observe);
        }
        self.snapshot()
    }

    /// Remove both cycle slots; release of caller snapshots is the caller's responsibility.
    pub fn clear(&mut self, mut observe: impl FnMut(CycleEvent)) {
        Self::retire(&mut self.current, &mut observe);
        Self::retire(&mut self.next, &mut observe);
    }

    fn retire(slot: &mut Option<Arc<DisplayImage>>, observe: &mut impl FnMut(CycleEvent)) {
        if let Some(image) = slot.take() {
            let path = image.path.clone();
            let other_owners_at_retirement = Arc::strong_count(&image) - 1;
            drop(image);
            // This retires the cycle slot. A snapshot owner may still retain the display buffer.
            observe(CycleEvent::Retired {
                path,
                other_owners_at_retirement,
            });
        }
    }

    fn load_available(
        &mut self,
        role: DecodeRole,
        observe: &mut impl FnMut(CycleEvent),
    ) -> Option<Arc<DisplayImage>> {
        // At most one traversal per request: all-bad input cannot spin forever or retain errors.
        for _ in 0..self.paths.len() {
            let path = self.paths[self.cursor].clone();
            self.cursor = (self.cursor + 1) % self.paths.len();
            match load_display(&path, self.request, |phase| {
                observe(CycleEvent::Phase {
                    path: path.clone(),
                    role,
                    phase,
                });
            }) {
                Ok(image) => {
                    observe(CycleEvent::Prepared {
                        path: image.path.clone(),
                        role,
                        source_pixels: image.source_pixels,
                        scaled_pixels: image.scaled_pixels,
                        display_pixels: image.display_pixels,
                        rgba_bytes: image.rgba.len(),
                    });
                    return Some(Arc::new(image));
                }
                Err(error) => observe(CycleEvent::Rejected { error, role }),
            }
        }
        None
    }
}
