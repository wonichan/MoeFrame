//! 自定义相框图分析（V2 相框外观专题工单 03）：把用户提供的
//! 相框图解码像素翻成「洞矩形 + 四边内距」，或带原因的拒绝。
//! 九宫格绘制矩形不在本模块：与内置三层一样由布局接缝
//! frame_layout 统一产出，本模块只做分析与加载。
//!
//! 检测规则只有一条（spec 定案）：从图片中心像素向外扩展，取
//! 包含中心的全透明矩形为洞；中心不透明即无效。格式限 PNG
//! （须有透明通道）。分析是纯函数：像素进、结论出，文件读取与
//! 解码在 `load_frame_art` 里一次做完，宿主不持有原图文件句柄。

use crate::window_geom::Insets;
use image::{ImageFormat, ImageReader, Limits};
use std::{fs::File, io::BufReader, path::Path};

/// 洞矩形（相框图原始像素坐标）：插画从这里露出来。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoleRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// 四边内距（相框图原始像素）：画框在每一边占的厚度，由洞矩形
/// 与图尺寸相减得到；缩放成窗口内距见 `FrameArtAnalysis::insets_at`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelInsets {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl PixelInsets {
    /// 按缩放系数换算成窗口逻辑内距。
    pub fn scaled(self, factor: f32) -> Insets {
        Insets {
            left: self.left as f32 * factor,
            top: self.top as f32 * factor,
            right: self.right as f32 * factor,
            bottom: self.bottom as f32 * factor,
        }
    }
}

/// 一次相框图分析的结论：图尺寸、洞矩形（内距由此推导）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameArtAnalysis {
    pub art_size: [u32; 2],
    pub hole: HoleRect,
}

impl FrameArtAnalysis {
    /// 四边内距（原始像素）：洞的左/上偏移与图右/下剩余。
    pub fn insets(&self) -> PixelInsets {
        PixelInsets {
            left: self.hole.x,
            top: self.hole.y,
            right: self.art_size[0] - self.hole.x - self.hole.width,
            bottom: self.art_size[1] - self.hole.y - self.hole.height,
        }
    }

    /// 框厚缩放系数：窗口内宽 ÷ 相框图原宽（spec 定案，框厚随
    /// 窗口大小成比例变化）。窗口宽未知或为 0 时按 1 处理。
    pub fn scale_factor(&self, window_inner_width: f32) -> f32 {
        if window_inner_width > 0.0 && self.art_size[0] > 0 {
            window_inner_width / self.art_size[0] as f32
        } else {
            1.0
        }
    }

    /// 给定窗口内宽时的生效内距（逻辑像素）：检测内距 × 缩放
    /// 系数，喂给几何状态机的就是这组值。
    pub fn insets_at(&self, window_inner_width: f32) -> Insets {
        self.insets().scaled(self.scale_factor(window_inner_width))
    }
}

/// 分析解码后的像素：`rgba` 是 RGBA8（每像素 4 字节，行优先），
/// `has_alpha` 是源图的颜色类型是否带透明通道（转成 RGBA 后
/// 人人都有 alpha 字节，无通道的源图必须在转换前判定）。
/// 成功返回洞与内距；失败返回给用户看的拒绝原因。
pub fn analyze_frame_art(
    width: u32,
    height: u32,
    rgba: &[u8],
    has_alpha: bool,
) -> Result<FrameArtAnalysis, String> {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return Err("相框图像素数据不完整".to_owned());
    }
    if !has_alpha {
        return Err("相框图没有透明通道，无法形成洞（请用带透明通道的 PNG）".to_owned());
    }
    let alpha_at = |x: u32, y: u32| rgba[((y * width + x) * 4) as usize + 3];
    let (cx, cy) = (width / 2, height / 2);
    if alpha_at(cx, cy) != 0 {
        return Err("相框图中心不是透明的，洞必须包含图片中心".to_owned());
    }
    // 从中心像素向外扩展：每次尝试把洞的一条边外推一像素，前提
    // 是新条带（在当前洞的横/纵范围内）整条全透明；四边都推不动
    // 时到达包含中心的最大透明矩形。
    let (mut left, mut right) = (cx, cx);
    let (mut top, mut bottom) = (cy, cy);
    loop {
        let mut grew = false;
        if top > 0 && (left..=right).all(|x| alpha_at(x, top - 1) == 0) {
            top -= 1;
            grew = true;
        }
        if bottom + 1 < height && (left..=right).all(|x| alpha_at(x, bottom + 1) == 0) {
            bottom += 1;
            grew = true;
        }
        if left > 0 && (top..=bottom).all(|y| alpha_at(left - 1, y) == 0) {
            left -= 1;
            grew = true;
        }
        if right + 1 < width && (top..=bottom).all(|y| alpha_at(right + 1, y) == 0) {
            right += 1;
            grew = true;
        }
        if !grew {
            break;
        }
    }
    Ok(FrameArtAnalysis {
        art_size: [width, height],
        hole: HoleRect {
            x: left,
            y: top,
            width: right - left + 1,
            height: bottom - top + 1,
        },
    })
}

/// 一次成功加载的相框图：分析结论与全分辨率 RGBA 像素（宿主
/// 上传为纹理后即可释放）。相框图只按路径读取、不复制进程序
/// 目录；每次启动与每次切到自定义都重新走一遍本函数。
#[derive(Debug)]
pub struct LoadedFrameArt {
    pub analysis: FrameArtAnalysis,
    pub rgba: Vec<u8>,
}

/// 读取并分析一张相框图：文件头必须是 PNG、必须带透明通道、
/// 洞检测必须通过；任一步失败返回给用户看的原因（宿主据此
/// 回退内置画框并在设置窗显示）。
pub fn load_frame_art(path: &Path) -> Result<LoadedFrameArt, String> {
    let file = File::open(path).map_err(|_| "相框图文件无法读取（不存在或被占用）".to_owned())?;
    let mut reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(|_| "相框图文件无法读取（不存在或被占用）".to_owned())?;
    if reader.format() != Some(ImageFormat::Png) {
        return Err("相框图必须是 PNG 格式".to_owned());
    }
    // 与产品图片加载同一套尺寸/分配上限，防止异常大图拖垮常驻进程。
    let mut limits = Limits::no_limits();
    limits.max_image_width = Some(crate::loading::PRODUCT_LIMITS.max_width);
    limits.max_image_height = Some(crate::loading::PRODUCT_LIMITS.max_height);
    limits.max_alloc = Some(crate::loading::PRODUCT_LIMITS.max_alloc);
    reader.limits(limits);
    let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reader.decode()));
    let image = match decoded {
        Ok(Ok(image)) => image,
        Ok(Err(_)) => return Err("相框图解码失败（文件可能已损坏）".to_owned()),
        Err(_) => return Err("相框图解码失败（文件可能已损坏）".to_owned()),
    };
    let has_alpha = image.color().has_alpha();
    let (width, height) = (image.width(), image.height());
    let rgba = image.to_rgba8().into_raw();
    let analysis = analyze_frame_art(width, height, &rgba, has_alpha)?;
    Ok(LoadedFrameArt { analysis, rgba })
}
