//! 布局接缝（V2 相框外观专题工单 02 / 03）：画框绘制的全部矩形由
//! 这里的纯函数产出，宿主只照单绘制。内置画框的三层（框体 /
//! 卡纸 / 画面）、自绘软阴影的扩散层与自定义相框图的九宫格矩形
//! 都在这里（spec 定案：一处统一产出，共用 LayoutRect 口径）。
//!
//! 坐标是逻辑像素的平面矩形，不依赖 egui 类型：几何状态机给出的
//! 画面区域口径（窗口 − 四边内距）在这里逐项复现，画面层矩形与
//! 几何口径必须一致，纹理才与解码目标对齐。
//!
//! 工单 04 起，每帧「画哪一层、面板铺不铺底、空状态提示怎么落」
//! 也由这里的 paint_plan 一次裁决：异形的约束（自定义生效时绝不
//! 混入内置层与阴影、透明处不铺底）钉在这张计划表上，宿主不再
//! 自行拼条件。

use crate::{
    config::{AppearanceMode, BuiltinFrame},
    frame_art::FrameArtAnalysis,
    window_geom::Insets,
};

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

/// 九宫格的一块：源矩形（相框图原始像素）与目标矩形（窗口逻辑
/// 坐标），与内置三层共用 LayoutRect 口径——宿主只有一份矩形
/// 转换，不存在第二套数组口径。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliceRegion {
    pub src: LayoutRect,
    pub dst: LayoutRect,
}

/// 九宫格布局：八个绘制区域（顺序为左上、上、右上、左、右、
/// 左下、下、右下）加洞区目标矩形；洞区不绘制（插画在下层），
/// 洞区矩形供宿主摆放插画与空状态提示。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NineSliceLayout {
    pub factor: f32,
    pub regions: [SliceRegion; 8],
    pub hole_dst: LayoutRect,
}

/// 按洞把相框图切成九宫格：四角按缩放系数等比绘制、四边只沿
/// 长度方向拉伸。缩放系数只由宽度决定（窗口内宽 ÷ 相框图原宽），
/// 因此外框变高时四角与边厚不变、只有纵向边变长。分析本身在
/// frame_art，本函数只吃分析结论产出矩形。
pub fn nine_slice_layout(analysis: &FrameArtAnalysis, outer: LayoutRect) -> NineSliceLayout {
    let factor = analysis.scale_factor(outer.width());
    let insets = analysis.insets();
    let (art_w, art_h) = (analysis.art_size[0] as f32, analysis.art_size[1] as f32);
    let hole_src = LayoutRect::new(
        insets.left as f32,
        insets.top as f32,
        (analysis.art_size[0] - insets.right) as f32,
        (analysis.art_size[1] - insets.bottom) as f32,
    );
    let hole_dst = outer.shrink(insets.scaled(factor));
    let region = |src: LayoutRect, dst: LayoutRect| SliceRegion { src, dst };
    NineSliceLayout {
        factor,
        regions: [
            region(
                LayoutRect::new(0.0, 0.0, hole_src.left, hole_src.top),
                LayoutRect::new(outer.left, outer.top, hole_dst.left, hole_dst.top),
            ),
            region(
                LayoutRect::new(hole_src.left, 0.0, hole_src.right, hole_src.top),
                LayoutRect::new(hole_dst.left, outer.top, hole_dst.right, hole_dst.top),
            ),
            region(
                LayoutRect::new(hole_src.right, 0.0, art_w, hole_src.top),
                LayoutRect::new(hole_dst.right, outer.top, outer.right, hole_dst.top),
            ),
            region(
                LayoutRect::new(0.0, hole_src.top, hole_src.left, hole_src.bottom),
                LayoutRect::new(outer.left, hole_dst.top, hole_dst.left, hole_dst.bottom),
            ),
            region(
                LayoutRect::new(hole_src.right, hole_src.top, art_w, hole_src.bottom),
                LayoutRect::new(hole_dst.right, hole_dst.top, outer.right, hole_dst.bottom),
            ),
            region(
                LayoutRect::new(0.0, hole_src.bottom, hole_src.left, art_h),
                LayoutRect::new(outer.left, hole_dst.bottom, hole_dst.left, outer.bottom),
            ),
            region(
                LayoutRect::new(hole_src.left, hole_src.bottom, hole_src.right, art_h),
                LayoutRect::new(hole_dst.left, hole_dst.bottom, hole_dst.right, outer.bottom),
            ),
            region(
                LayoutRect::new(hole_src.right, hole_src.bottom, art_w, art_h),
                LayoutRect::new(hole_dst.right, hole_dst.bottom, outer.right, outer.bottom),
            ),
        ],
        hole_dst,
    }
}

/// 生效外观：把「外观模式 × 相框图是否生效 × 用户内置参数」一次
/// 裁决成宿主各处共用的结论——绘制计划、生效内距与内置绘制参数
/// 都只消费这一份结论，「自定义未生效即回退内置」的规则只在这里
/// 编码一次。
///
/// 回退的口径是内置**默认**画框（胡桃木 + 默认宽，spec 定案），
/// 不是用户当前的内置参数：回退是运行期状态，只读不改已保存的
/// 设置；用户把相框图修好后自定义即恢复，用户参数全程无痕。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectiveAppearance {
    /// 极简：不画任何画框层，内距全 0（V1 行为）。
    Minimal,
    /// 按内置画框绘制，携带生效参数：内置模式是用户参数，
    /// 自定义回退时是默认参数。
    Builtin(BuiltinFrame),
    /// 自定义相框图生效：画框层只有九宫格，内距来自分析结果
    /// 按窗口宽度缩放（宿主在该分支另行给出）。
    CustomArt,
}

/// 生效外观裁决：自定义且相框图分析成功才走相框图，其余自定义
/// 一律回退内置默认画框。
pub fn effective_appearance(
    appearance: AppearanceMode,
    custom_active: bool,
    frame: BuiltinFrame,
) -> EffectiveAppearance {
    match appearance {
        AppearanceMode::Minimal => EffectiveAppearance::Minimal,
        AppearanceMode::Builtin => EffectiveAppearance::Builtin(frame),
        AppearanceMode::Custom if custom_active => EffectiveAppearance::CustomArt,
        AppearanceMode::Custom => EffectiveAppearance::Builtin(BuiltinFrame::default()),
    }
}

impl EffectiveAppearance {
    /// 本结论的静态四边内距：内置按生效参数（单边 = 框宽 +
    /// 卡纸宽），极简全 0。自定义生效时这里也是全 0——其生效
    /// 内距是分析结果，宿主在 CustomArt 分支换成缩放后的检测
    /// 内距，不经此方法。
    pub fn insets(self) -> Insets {
        match self {
            EffectiveAppearance::Builtin(frame) => AppearanceMode::Builtin.insets(frame),
            EffectiveAppearance::Minimal | EffectiveAppearance::CustomArt => Insets::ZERO,
        }
    }
}

/// 一帧要画的画框层（工单 04 的绘制计划）：三选一，互斥——尤其
/// 自定义相框图生效时只有九宫格一层，自绘软阴影只随内置层出现，
/// 异形因此不可能混入阴影。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePaint {
    /// 不画任何画框层（极简：插画贴边铺满整窗）。
    None,
    /// 内置画框三层 + 自绘软阴影（宿主照 builtin_layers 与
    /// shadow_layers 绘制）。
    Builtin,
    /// 自定义相框图九宫格八区（洞区不画、插画在下层），无阴影。
    CustomArt,
}

/// 无图（空状态）时提示文字的落点（工单 04 的绘制计划）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintPaint {
    /// 有图：不画提示。
    None,
    /// 由内置画框绘制内部处理：背景是自家卡纸 / 框体，字色按
    /// 卡纸在场与否在绘制层内选。
    InFrame,
    /// 提示浮在画面区域中央（极简是整窗、自定义生效时是洞）：
    /// outline 为真时浅字衬一层深色描影——自定义空状态的背景
    /// 是透出的桌面、明暗未知，不衬影在浅色桌面上会看不清。
    Floating { outline: bool },
}

/// 一帧的绘制计划：宿主先问计划再照单绘制。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaintPlan {
    /// 面板是否铺不透明底：只有极简铺黑底保持 V1 视觉；内置由
    /// 框体层自铺满整窗，自定义生效时必须透明——洞与不规则外
    /// 轮廓的透明像素要直接透出桌面，铺底即假镂空。
    pub panel_opaque: bool,
    pub frame_paint: FramePaint,
    pub hint: HintPaint,
}

/// 绘制计划裁决（生效外观 × 是否有图）：回退与否已在
/// effective_appearance 里裁决完毕，这里只把结论翻成画框层、
/// 面板与提示三项；生效外观是内置（含回退）时画框层与面板口径
/// 与内置模式逐项一致，自定义生效时相框图是唯一的画框层。
pub fn paint_plan(effective: EffectiveAppearance, has_image: bool) -> PaintPlan {
    let frame_paint = match effective {
        EffectiveAppearance::Builtin(_) => FramePaint::Builtin,
        EffectiveAppearance::CustomArt => FramePaint::CustomArt,
        EffectiveAppearance::Minimal => FramePaint::None,
    };
    let hint = if has_image {
        HintPaint::None
    } else {
        match frame_paint {
            FramePaint::Builtin => HintPaint::InFrame,
            FramePaint::CustomArt => HintPaint::Floating { outline: true },
            FramePaint::None => HintPaint::Floating { outline: false },
        }
    };
    PaintPlan {
        panel_opaque: matches!(frame_paint, FramePaint::None),
        frame_paint,
        hint,
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
