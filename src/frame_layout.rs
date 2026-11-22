//! 布局接缝（V2 相框外观专题工单 02）：画框绘制的全部矩形由这里的
//! 纯函数产出，宿主只照单绘制。内置画框的三层（框体 / 卡纸 / 画面）
//! 与自绘软阴影的扩散层都在这里；工单 03 的九宫格矩形也将落在这里。
//!
//! 坐标是逻辑像素的平面矩形，不依赖 egui 类型：几何状态机给出的
//! 画面区域口径（窗口 − 四边内距）在这里逐项复现，画面层矩形与
//! 几何口径必须一致，纹理才与解码目标对齐。

use crate::window_geom::Insets;

/// 平面矩形（逻辑像素）：布局接缝的输入输出口径。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl LayoutRect {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// 按四边内距内收：结果是画面区域（与几何口径一致）。
    pub fn shrink(&self, insets: Insets) -> Self {
        Self {
            left: self.left + insets.left,
            top: self.top + insets.top,
            right: self.right - insets.right,
            bottom: self.bottom - insets.bottom,
        }
    }

    /// 四边等量内收。
    pub fn shrink_uniform(&self, amount: f32) -> Self {
        self.shrink(Insets {
            left: amount,
            top: amount,
            right: amount,
            bottom: amount,
        })
    }
}

/// 内置画框的三层绘制矩形：框体层即窗口本身，卡纸层是框体内侧的
/// 区域（卡纸带 + 画面），画面层是插画落点。宿主的画法是逐层填
/// 满：框体色填 body、卡纸色填 mat、纹理填 image、内沿细线沿
/// image 边界贴内侧描画。卡纸宽为 0 时 mat 与 image 重合，三层
/// 退化为两层，宿主跳过卡纸填充即可。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuiltinLayers {
    pub body: LayoutRect,
    pub mat: LayoutRect,
    pub image: LayoutRect,
}

/// 内置画框布局：卡纸层按框宽均匀内收，画面层按四边内距内收
/// （内置模式内距四边相等 = 框宽 + 卡纸宽，逐边口径与几何一致）。
pub fn builtin_layers(window: LayoutRect, insets: Insets, frame_width: f32) -> BuiltinLayers {
    BuiltinLayers {
        body: window,
        mat: window.shrink_uniform(frame_width),
        image: window.shrink(insets),
    }
}

/// 自绘软阴影的层数（逻辑像素）：阴影总扩散带宽 = 层数 × 步长。
const SHADOW_STEPS: usize = 6;
/// 自绘软阴影的单层内收步长（逻辑像素）：宿主逐层描边的宽度
/// 与此一致，相邻两层的矩形恰好首尾相接。
pub const SHADOW_STEP_PX: f32 = 2.0;
/// 每层的黑色透明度（0–255），由外向内递减：最外层最深，贴近
/// 框体内部时淡出。
const SHADOW_ALPHAS: [u8; SHADOW_STEPS] = [64, 48, 36, 26, 18, 12];

/// 自绘软阴影（仅内置画框模式）：窗口尺寸即外框尺寸，窗外没有
/// 可绘制的像素，阴影落在框体外沿内侧的扩散带上——多层矩形从
/// 窗口边缘逐层内收、透明度逐层递减，宿主按返回顺序以单层宽度
/// 的内侧描边逐层画出，得到边缘最深、向内淡出的柔和扩散。
/// 返回（矩形，该层描边的黑色透明度），最外层在前。
pub fn shadow_layers(window: LayoutRect) -> Vec<(LayoutRect, u8)> {
    (0..SHADOW_STEPS)
        .map(|step| {
            (
                window.shrink_uniform(step as f32 * SHADOW_STEP_PX),
                SHADOW_ALPHAS[step],
            )
        })
        .collect()
}
