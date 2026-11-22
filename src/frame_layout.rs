//! 布局接缝（V2 相框外观专题工单 02 / 03）：画框绘制的全部矩形由
//! 这里的纯函数产出，宿主只照单绘制。内置画框的三层（框体 /
//! 卡纸 / 画面）与着色计划（截面明暗 / 造型线 / 贴边投影 / 卡纸
//! 斜切面）、自定义相框图的九宫格矩形都在这里（spec 定案：一处
//! 统一产出，共用 LayoutRect 口径）。
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
/// 区域（卡纸带 + 画面），画面层是插画落点。三层只是落点口径：
/// 框带按截面着色分边落笔、卡纸面带投影与斜切面，着色几何见
/// builtin_shading。卡纸宽为 0 时 mat 与 image 重合，三层退化
/// 为两层，宿主跳过卡纸填充即可。
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
    /// 内置画框三层 + 着色计划（宿主照 builtin_shading 绘制）。
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

/// 画框的四条边（着色计划口径）。光自左上：方向光倍率上 >
/// 左 > 右 > 下（见 builtin_shading）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameEdge {
    Top,
    Left,
    Bottom,
    Right,
}

/// 一条边的截面着色：带矩形 + 沿带宽方向（外沿 → 内沿）的明暗
/// 档位。档位是（分位，亮度倍率）：宿主按预设基色乘倍率、在
/// 相邻档位间细分条带插值落笔，得到无硬界的截面明暗。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeShade {
    pub edge: FrameEdge,
    pub rect: LayoutRect,
    pub stops: [(f32, f32); 4],
}

/// 框带造型线的落笔矩形（宿主以 1px 内侧描边画出）：外沿压线、
/// 凹槽（窄框放不下时缺席）、紧贴卡纸层边界的内唇亮线。造型
/// 集中在带内侧——真实画框截面的凹凸多在内唇一带，外沿只压
/// 一道暗线收边。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileLines {
    pub outer_rim: LayoutRect,
    pub groove: Option<LayoutRect>,
    pub lip: LayoutRect,
}

/// 一道贴边淡出的影：以 rect 的边界为最深处，向 rect 内部淡出
/// depth 距离，边界处黑色透明度 max_alpha。宿主以逐层内收的
/// 描边环落笔，四边均匀。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FadeBand {
    pub rect: LayoutRect,
    pub depth: f32,
    pub max_alpha: u8,
}

/// 卡纸着色：框体投在卡纸上的影（框宽为 0 时无框可投，缺席）、
/// 卡纸内口的斜切面、卡纸投在画面上的影。斜切面以外沿矩形
/// 给出：外沿 = 画面外扩 bevel_width，宿主画外沿与画面之间的
/// 一圈近白色，即 45° 切面受光的那一面。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatShading {
    pub cast_shadow: Option<FadeBand>,
    pub bevel: LayoutRect,
    pub bevel_width: f32,
    pub image_shadow: FadeBand,
}

/// 内置画框的着色计划：三层矩形 + 框带截面着色 + 造型线 + 卡纸
/// 着色，全部纯数据，宿主只照单落笔（颜色按预设在宿主侧合成）。
/// 深度线索来自截面明暗、两道贴边投影与卡纸斜切面——窗口尺寸
/// 即外框尺寸、窗外无可绘制像素，投影只能落在相邻层面上（框
/// 影落卡纸、卡纸影落画面），不存在游离的灰带。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuiltinShading {
    pub layers: BuiltinLayers,
    pub edges: [EdgeShade; 4],
    pub profile: Option<ProfileLines>,
    pub mat: Option<MatShading>,
}

/// 截面明暗走向（分位，基础倍率）：外沿略暗 → 中段隆起受光 →
/// 凹槽压暗 → 内唇最亮，是木框截面最常见的明暗次序。
const PROFILE_STOPS: [(f32, f32); 4] = [(0.0, 0.84), (0.55, 1.05), (0.82, 0.76), (1.0, 1.10)];
/// 框影深度（逻辑像素，spec 口径 6–8）：贴框体内沿在卡纸上
/// 淡出的距离，钳到卡纸宽内。
const CAST_SHADOW_DEPTH: f32 = 7.0;
/// 框影边界透明度：框体厚，影实。
const CAST_SHADOW_ALPHA: u8 = 58;
/// 斜切面宽（逻辑像素）：卡纸内口 45° 切面受光的一圈，钳到
/// 卡纸宽内。
const BEVEL_WIDTH: f32 = 2.0;
/// 画面影深度（逻辑像素，spec 口径 4–6）：卡纸投在画面上的影。
const IMAGE_SHADOW_DEPTH: f32 = 5.0;
/// 画面影边界透明度：卡纸薄，影淡于框影。
const IMAGE_SHADOW_ALPHA: u8 = 46;
/// 造型线成立的最小框宽：低于此带内只剩截面渐变本身。
const PROFILE_MIN_FRAME_WIDTH: f32 = 2.0;
/// 凹槽成立的最小框宽。
const GROOVE_MIN_FRAME_WIDTH: f32 = 5.0;

/// 内置画框着色计划：与 builtin_layers 同吃窗口矩形 + 四边
/// 内距，另需框宽与卡纸宽定着色几何。框宽为 0 时四边矩形退化
/// 为零面积（宿主跳过）、造型线缺席；卡纸宽为 0 时卡纸着色
/// 缺席，与三层退化口径一致。
pub fn builtin_shading(
    window: LayoutRect,
    insets: Insets,
    frame_width: f32,
    mat_width: f32,
) -> BuiltinShading {
    let layers = builtin_layers(window, insets, frame_width);
    let (body, mat_rect) = (layers.body, layers.mat);
    let edge = |edge: FrameEdge, rect: LayoutRect, light: f32| EdgeShade {
        edge,
        rect,
        stops: PROFILE_STOPS.map(|(frac, tone)| (frac, tone * light)),
    };
    // 方向光（光自左上）：上 1.07 > 左 1.02 > 右 0.95 > 下 0.89。
    // 上、下边通宽含四角，左、右边夹在中间，四边恰好铺满框带。
    let edges = [
        edge(
            FrameEdge::Top,
            LayoutRect::new(body.left, body.top, body.right, mat_rect.top),
            1.07,
        ),
        edge(
            FrameEdge::Left,
            LayoutRect::new(body.left, mat_rect.top, mat_rect.left, mat_rect.bottom),
            1.02,
        ),
        edge(
            FrameEdge::Bottom,
            LayoutRect::new(body.left, mat_rect.bottom, body.right, body.bottom),
            0.89,
        ),
        edge(
            FrameEdge::Right,
            LayoutRect::new(mat_rect.right, mat_rect.top, body.right, mat_rect.bottom),
            0.95,
        ),
    ];
    // 造型线落笔矩形（1px 内侧描边）：外沿线即窗口边；凹槽在
    // 内沿外 3px 起、内唇在内沿外 1px 起——描边向矩形内侧落，
    // 矩形内收量 = 带内距 − 线宽方向的偏移。
    let profile = (frame_width >= PROFILE_MIN_FRAME_WIDTH).then(|| ProfileLines {
        outer_rim: body,
        groove: (frame_width >= GROOVE_MIN_FRAME_WIDTH)
            .then(|| body.shrink_uniform(frame_width - 3.0)),
        lip: body.shrink_uniform(frame_width - 1.0),
    });
    let mat = (mat_width > 0.0).then(|| {
        let bevel_width = BEVEL_WIDTH.min(mat_width);
        MatShading {
            cast_shadow: (frame_width > 0.0).then(|| FadeBand {
                rect: mat_rect,
                depth: CAST_SHADOW_DEPTH.min(mat_width),
                max_alpha: CAST_SHADOW_ALPHA,
            }),
            bevel: layers.image.shrink_uniform(-bevel_width),
            bevel_width,
            image_shadow: FadeBand {
                rect: layers.image,
                depth: IMAGE_SHADOW_DEPTH,
                max_alpha: IMAGE_SHADOW_ALPHA,
            },
        }
    });
    BuiltinShading {
        layers,
        edges,
        profile,
        mat,
    }
}
