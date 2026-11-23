//! 切换动画无方向组的纯计算（切换动画专题工单 03）：百叶帘条带、
//! 墨晕扇形、像素块序与抖动。其中旧图 fit 矩形、墙钟进度与
//! ease-out 曲线是五种动画共用的唯一实现，宿主有方向组
//! （工单 02 的卷页/抽卡）也经这里取，不另存副本。这里只算
//! 「进度 → 几何/UV/明暗」，不碰 egui：宿主拿结论落笔（同纹理子
//! 区域绘制，spec 第 8 节），计算本身在 tests/transition_ambient.rs
//! 钉住。三种动画都不分方向（spec 第 4 节），本模块无方向参数。

use crate::frame_layout::LayoutRect;

/// 百叶帘的竖帘条数（spec 第 2 节冻结值）。
pub const BLINDS_STRIPS: usize = 8;
/// 百叶帘单条的揭示时长占总时长的比例；其余比例是自左至右的
/// 错峰总跨度，最后一条在进度 1 时恰好播满。
const BLINDS_REVEAL_SPAN: f32 = 0.65;

/// 进度钳制到 0..=1。
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// 三次 ease-out：起步快、落位缓，五种动画共用同一条手感曲线。
/// 全仓唯一实现（评审收敛）：宿主有方向组（transition.rs 的
/// 卷页）也经这里取曲线，不在宿主另存一份。
pub fn ease_out_cubic(p: f32) -> f32 {
    let rest = 1.0 - clamp01(p);
    1.0 - rest * rest * rest
}

/// 墙钟进度：已流逝 / 总时长，钳到 0..=1。掉帧时直接跳进度、
/// 不延长总时长（spec 第 8 节）；总时长为 0 视为立即完成。
pub fn transition_progress(elapsed_ms: u64, duration_ms: u64) -> f32 {
    if duration_ms == 0 {
        return 1.0;
    }
    clamp01(elapsed_ms as f32 / duration_ms as f32)
}

/// 旧图在新舞台里的 fit 矩形：按旧图宽高比在舞台内取最大内接
/// 矩形并居中——绝不拉伸（spec 第 7 节）。宽高比非法（<=0 或
/// 非有限）时退回舞台本身。
pub fn fit_rect(stage: LayoutRect, aspect: f32) -> LayoutRect {
    if aspect <= 0.0 || !aspect.is_finite() {
        return stage;
    }
    let stage_aspect = stage.width() / stage.height();
    let (width, height) = if aspect >= stage_aspect {
        (stage.width(), stage.width() / aspect)
    } else {
        (stage.height() * aspect, stage.height())
    };
    let left = stage.left + (stage.width() - width) / 2.0;
    let top = stage.top + (stage.height() - height) / 2.0;
    LayoutRect::new(left, top, left + width, top + height)
}

/// 墨晕扇形的分段数：64 段多边形肉眼即圆（探路笔记定案）。
pub const INK_SEGMENTS: usize = 64;
/// 墨晕灰纱的最强透明度：盖在旧图上模拟去色（用户 2026-11-22
/// 批准的近似——egui tint 是乘法、做不出真去色，用渐强灰纱 +
/// 压暗代替），满格时也不全遮旧图。
const INK_VEIL_MAX_ALPHA: u8 = 110;
/// 墨晕满格时旧图的压暗下限（tint 灰度倍率）。
const INK_OLD_SHADE_FLOOR: f32 = 0.65;

/// 墨晕在给定进度的扇形参数：圆心恒在画面中心，半径按 ease-out
/// 增长、满格覆盖四角；灰纱与旧图压暗随进度同步加深。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InkFan {
    pub center: [f32; 2],
    pub radius: f32,
    /// 盖在旧图上的灰纱透明度（去色近似的一半）。
    pub veil_alpha: u8,
    /// 旧图的 tint 灰度倍率（去色近似的另一半：压暗）。
    pub old_shade: f32,
}

/// 墨晕扇形参数。半径以舞台半对角线为满格值，保证四角盖全。
pub fn ink_fan(stage: LayoutRect, progress: f32) -> InkFan {
    let progress = clamp01(progress);
    let half_diagonal = (stage.width().powi(2) + stage.height().powi(2)).sqrt() / 2.0;
    InkFan {
        center: [
            (stage.left + stage.right) / 2.0,
            (stage.top + stage.bottom) / 2.0,
        ],
        radius: ease_out_cubic(progress) * half_diagonal,
        veil_alpha: (progress * INK_VEIL_MAX_ALPHA as f32).round() as u8,
        old_shade: 1.0 - (1.0 - INK_OLD_SHADE_FLOOR) * progress,
    }
}

/// 墨晕扇形环上一点：返回落点坐标与该点在舞台内的归一化 UV
/// （单纹理取样——整幅新图贴在扇形 Mesh 上）。`ring_scale` 1.0
/// 是主环；羽化外环传 >1 的值取同角度的外圈点（UV 随点走，
/// 超出舞台时由宿主的舞台裁剪收住）。
pub fn ink_ring_point(
    stage: LayoutRect,
    fan: &InkFan,
    index: usize,
    ring_scale: f32,
) -> ([f32; 2], [f32; 2]) {
    let angle = index as f32 / INK_SEGMENTS as f32 * std::f32::consts::TAU;
    let radius = fan.radius * ring_scale;
    let pos = [
        fan.center[0] + radius * angle.cos(),
        fan.center[1] + radius * angle.sin(),
    ];
    let uv = [
        (pos[0] - stage.left) / stage.width(),
        (pos[1] - stage.top) / stage.height(),
    ];
    (pos, uv)
}

/// 像素故障的网格列数 / 行数 / 总块数（spec 第 2 节冻结值）。
pub const PIXEL_COLS: usize = 12;
pub const PIXEL_ROWS: usize = 8;
pub const PIXEL_BLOCKS: usize = PIXEL_COLS * PIXEL_ROWS;
/// 像素故障的抖动窗口：只在开头约 120ms 内抖旧图（spec 冻结值）。
pub const PIXEL_JITTER_MS: u64 = 120;
/// 抖动幅度上限（逻辑像素）：错位要读得出，但不能把旧图抖出舞台。
const PIXEL_JITTER_MAX_PX: f32 = 4.0;

/// 确定性伪随机（xorshift64*）：与 frame_paint 的木纹同思路——
/// 同一种子逐帧同序（每帧重洗会闪烁），种子由宿主在转场开始时
/// 现取，故每条动画的块序都不同（spec：每条重新洗、不要求跨
/// 进程可复现）。
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// [0, bound) 取样（bound 极小，取模偏差无碍）。
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// 像素故障的块出现顺序：96 块的 Fisher–Yates 洗牌。种子为 0 时
/// 换一个非零初值（xorshift 在 0 处不动点）。
pub fn pixel_order(seed: u64) -> [u8; PIXEL_BLOCKS] {
    let mut order = [0u8; PIXEL_BLOCKS];
    for (i, slot) in order.iter_mut().enumerate() {
        *slot = i as u8;
    }
    let mut rng = XorShift(if seed == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        seed
    });
    for i in (1..PIXEL_BLOCKS).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        order.swap(i, j);
    }
    order
}

/// 给定进度下已拼齐的块数：随进度线性增长，满格恰好全覆盖。
pub fn pixel_visible_count(progress: f32) -> usize {
    (clamp01(progress) * PIXEL_BLOCKS as f32).floor() as usize
}

/// 一块的落点与源 UV 子区域（行优先编号，同一张新图纹理切分）。
pub fn pixel_block(stage: LayoutRect, index: usize) -> (LayoutRect, [f32; 4]) {
    let col = (index % PIXEL_COLS) as f32;
    let row = (index / PIXEL_COLS) as f32;
    let block_w = stage.width() / PIXEL_COLS as f32;
    let block_h = stage.height() / PIXEL_ROWS as f32;
    let dest = LayoutRect::new(
        stage.left + col * block_w,
        stage.top + row * block_h,
        stage.left + (col + 1.0) * block_w,
        stage.top + (row + 1.0) * block_h,
    );
    let uv = [
        col / PIXEL_COLS as f32,
        row / PIXEL_ROWS as f32,
        (col + 1.0) / PIXEL_COLS as f32,
        (row + 1.0) / PIXEL_ROWS as f32,
    ];
    (dest, uv)
}

/// 旧图的抖动偏移：只活在开头 PIXEL_JITTER_MS 内，窗口一过恒为
/// 零。窗口内按约 33ms 一帧从种子派生——同帧多次问结果一致
/// （是抖动不是每问一次跳一次），帧与帧之间跳变。
pub fn pixel_jitter(elapsed_ms: u64, seed: u64) -> [f32; 2] {
    if elapsed_ms >= PIXEL_JITTER_MS {
        return [0.0, 0.0];
    }
    let frame = elapsed_ms / 33;
    let mut rng =
        XorShift(seed ^ frame.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA076_1D64_78BD_642F);
    let unit = |rng: &mut XorShift| (rng.next() >> 40) as f32 / (1u64 << 24) as f32;
    [
        (unit(&mut rng) * 2.0 - 1.0) * PIXEL_JITTER_MAX_PX,
        (unit(&mut rng) * 2.0 - 1.0) * PIXEL_JITTER_MAX_PX,
    ]
}

/// 百叶帘的一条已揭示部分：落点（舞台子矩形）、源 UV 子区域
/// （同一张新图纹理）、明暗（翻转感：未满格时压暗，满格回 1）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlindStrip {
    pub dest: LayoutRect,
    pub uv: [f32; 4],
    pub shade: f32,
}

/// 百叶帘在给定进度的全部已揭示条带：恰 8 条竖帘自左至右错峰，
/// 每条从舞台顶部向下揭示（上沿翻下）。进度 0 时一条都没有；
/// 进度 1 时 8 条无缝拼满舞台且 shade 全回 1。
pub fn blinds_strips(stage: LayoutRect, progress: f32) -> Vec<BlindStrip> {
    let progress = clamp01(progress);
    let strip_width = stage.width() / BLINDS_STRIPS as f32;
    let mut strips = Vec::with_capacity(BLINDS_STRIPS);
    for i in 0..BLINDS_STRIPS {
        let stagger = (1.0 - BLINDS_REVEAL_SPAN) * i as f32 / (BLINDS_STRIPS - 1) as f32;
        let local = clamp01((progress - stagger) / BLINDS_REVEAL_SPAN);
        if local <= 0.0 {
            continue;
        }
        let eased = ease_out_cubic(local);
        let left = stage.left + strip_width * i as f32;
        let dest = LayoutRect::new(
            left,
            stage.top,
            left + strip_width,
            stage.top + stage.height() * eased,
        );
        strips.push(BlindStrip {
            dest,
            uv: [
                i as f32 / BLINDS_STRIPS as f32,
                0.0,
                (i + 1) as f32 / BLINDS_STRIPS as f32,
                eased,
            ],
            shade: 0.55 + 0.45 * eased,
        });
    }
    strips
}
