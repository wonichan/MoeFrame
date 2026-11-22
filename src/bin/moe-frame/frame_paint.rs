//! 内置画框绘制（V2 相框外观专题工单 02，观感重做）：三层
//! （框体 / 卡纸 / 画面）的落点与全部光影几何来自库
//! frame_layout 的着色计划（builtin_shading），本文件只定颜色
//! 与落笔法。深度线索按真实裱画的次序组织：框带截面明暗
//! （方向光 + 造型线 + 木纹细纹）、框体投在卡纸上的影、卡纸
//! 内口的斜切面、卡纸投在画面上的影。不引入图片素材。

use super::*;

/// 卡纸颜色：默认米白（spec 定案）。
const MAT_COLOR: egui::Color32 = egui::Color32::from_rgb(246, 240, 224);
/// 卡纸斜切面颜色：比卡纸面更近白——45° 切面受光的那一面。
const BEVEL_COLOR: egui::Color32 = egui::Color32::from_rgb(253, 251, 245);
/// 内沿细线：紧贴画面边界的半透明暗线（画芯与卡纸斜切面之间
/// 的压痕），任何预设下都成立。
const INNER_EDGE_ALPHA: u8 = 96;
/// 截面渐变的细分条数：14px 框带约 1.2px 一条，条带边界低于
/// 可辨阈值。
const EDGE_STRIPS: usize = 12;
/// 贴边投影的描边环数：深度 5–7px 时环宽不足 1px，淡出连续。
const FADE_RINGS: usize = 8;
/// 每条边的木纹细纹条数。
const GRAIN_STREAKS: usize = 14;
/// 角部弧段的采样数（四分之一圆）：半径 20 时每段约 1.6px，
/// 折线感低于可辨阈值。
const ARC_SEGMENTS: usize = 16;

/// 四角的弧起止方向（单位向量，y 向下），顺序左上、右上、
/// 右下、左下：每个角自一条边的切向转到相邻边的切向。截面
/// 弧带、角部木纹、扇形填充三处角部落笔共用这一份方向表
/// （圆角工单 01 评审修复抽出）。
const CORNER_DIRS: [((f32, f32), (f32, f32)); 4] = [
    ((-1.0, 0.0), (0.0, -1.0)),
    ((0.0, -1.0), (1.0, 0.0)),
    ((1.0, 0.0), (0.0, 1.0)),
    ((0.0, 1.0), (-1.0, 0.0)),
];

/// 一个角的圆弧几何：圆心 + 归一化后的起始角与扫角（起止
/// 方向换成弧度，扫角取正向）。三处角部落笔共用：给半径与
/// 角向分位区间即可采样折线点，角度归一化只在此处做一次。
#[derive(Clone, Copy)]
struct CornerArc {
    center: (f32, f32),
    start: f32,
    sweep: f32,
}

impl CornerArc {
    /// 由某个角边界的四角圆心与角序号组装（序号与
    /// CORNER_DIRS 同序）。
    fn of(centers: &[(f32, f32); 4], index: usize) -> Self {
        let (d0, d1) = CORNER_DIRS[index];
        let start = d0.1.atan2(d0.0);
        let mut end = d1.1.atan2(d1.0);
        while end < start {
            end += std::f32::consts::TAU;
        }
        Self {
            center: centers[index],
            start,
            sweep: end - start,
        }
    }

    /// 沿半径 radius、在角向分位 [from, to] 内采样 segments
    /// 段折线点（两端都含）。
    fn points(&self, radius: f32, from: f32, to: f32, segments: usize) -> Vec<egui::Pos2> {
        (0..=segments)
            .map(|s| {
                let a =
                    self.start + self.sweep * (from + (to - from) * (s as f32 / segments as f32));
                egui::pos2(
                    self.center.0 + radius * a.cos(),
                    self.center.1 + radius * a.sin(),
                )
            })
            .collect()
    }
}

/// 框体预设的基色：截面明暗、造型线与木纹都由基色按亮度倍率
/// 合成，不再单列亮/暗色。
fn preset_base(preset: FramePreset) -> egui::Color32 {
    match preset {
        FramePreset::Walnut => egui::Color32::from_rgb(107, 74, 47),
        FramePreset::Oak => egui::Color32::from_rgb(198, 158, 102),
        FramePreset::Black => egui::Color32::from_rgb(26, 26, 28),
        FramePreset::White => egui::Color32::from_rgb(240, 238, 233),
    }
}

/// 按亮度倍率调色：RGB 乘倍率并钳到 0..=255。只喂不透明的
/// 预设基色，透明度另行经 with_alpha 叠加。
fn shade(color: egui::Color32, tone: f32) -> egui::Color32 {
    let scale = |v: u8| (v as f32 * tone).clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(scale(color.r()), scale(color.g()), scale(color.b()), 255)
}

/// 给不透明色叠一层透明度。
fn with_alpha(color: egui::Color32, alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// 档位插值：分位 frac 处的亮度倍率（相邻档位间线性）。
fn tone_at(stops: &[(f32, f32); 4], frac: f32) -> f32 {
    for pair in stops.windows(2) {
        let (f0, t0) = pair[0];
        let (f1, t1) = pair[1];
        if frac <= f1 {
            let span = f1 - f0;
            let local = if span > 0.0 { (frac - f0) / span } else { 0.0 };
            return t0 + (t1 - t0) * local.clamp(0.0, 1.0);
        }
    }
    stops[3].1
}

/// 一条边的截面渐变：沿带宽方向（外沿 → 内沿）细分条带，每条
/// 按中心分位取档位插值的颜色落笔。下边、右边条带顺序反转
/// ——分位方向永远是外沿 → 内沿，与边的朝向无关。条带只画直
/// 边段：角部由 paint_corner_arcs 沿弧落笔，条带伸进角区会
/// 戳出外沿圆弧（圆角工单 01）。
fn paint_edge(
    painter: &egui::Painter,
    edge: &EdgeShade,
    base: egui::Color32,
    shading: &BuiltinShading,
) {
    let rect = edge.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let body = shading.layers.body;
    let outer_radius = shading.corners.outer.radius;
    let inner_radius = shading
        .corners
        .frame_inner
        .map_or(0.0, |boundary| boundary.radius);
    // 直边段范围：上 / 下边避开外沿圆弧的切点，左 / 右边避开
    // 外沿与框体内沿两条圆弧的切点（取更靠内侧的一个）。
    let (x0, x1) = (body.left + outer_radius, body.right - outer_radius);
    let (y0, y1) = (
        (body.top + outer_radius).max(rect.top + inner_radius),
        (body.bottom - outer_radius).min(rect.bottom - inner_radius),
    );
    for i in 0..EDGE_STRIPS {
        let f0 = i as f32 / EDGE_STRIPS as f32;
        let f1 = (i + 1) as f32 / EDGE_STRIPS as f32;
        let color = shade(base, tone_at(&edge.stops, (f0 + f1) / 2.0));
        let strip = match edge.edge {
            FrameEdge::Top => LayoutRect::new(
                x0,
                rect.top + rect.height() * f0,
                x1,
                rect.top + rect.height() * f1,
            ),
            FrameEdge::Bottom => LayoutRect::new(
                x0,
                rect.bottom - rect.height() * f1,
                x1,
                rect.bottom - rect.height() * f0,
            ),
            FrameEdge::Left => LayoutRect::new(
                rect.left + rect.width() * f0,
                y0,
                rect.left + rect.width() * f1,
                y1,
            ),
            FrameEdge::Right => LayoutRect::new(
                rect.right - rect.width() * f1,
                y0,
                rect.right - rect.width() * f0,
                y1,
            ),
        };
        if strip.width() > 0.0 && strip.height() > 0.0 {
            painter.rect_filled(to_egui_rect(strip), 0.0, color);
        }
    }
}

/// 四角的截面渐变（圆角工单 01）：以窗口外沿圆心为心、按条带
/// 分位逐圈描弧，颜色与归属边的直边段同档位——角的归属与直
/// 边铺法一致（上边带管上两角、下边带管下两角），截面明暗
/// 因此沿弧连续。逐带的弧半径由布局接缝给出
/// （corner_strip_radii，与直边条带同分位），本函数只照单
/// 描弧。弧的内径止于外沿半径 − 框宽：再往里是卡纸层自己
/// 的圆角填充，不归框带。
fn paint_corner_arcs(painter: &egui::Painter, shading: &BuiltinShading, base: egui::Color32) {
    let outer = shading.corners.outer;
    let frame_width = shading.layers.mat.left - shading.layers.body.left;
    if outer.radius <= 0.0 || frame_width <= 0.0 {
        return;
    }
    let owners = [
        &shading.edges[0],
        &shading.edges[0],
        &shading.edges[2],
        &shading.edges[2],
    ];
    let strips = shading.corner_strip_radii.len() as f32;
    for (ci, owner) in owners.iter().enumerate() {
        let arc = CornerArc::of(&outer.centers, ci);
        for (i, &radius) in shading.corner_strip_radii.iter().enumerate() {
            if radius <= 0.0 {
                continue;
            }
            let fm = (i as f32 + 0.5) / strips;
            let color = shade(base, tone_at(&owner.stops, fm));
            painter.add(egui::Shape::line(
                arc.points(radius, 0.0, 1.0, ARC_SEGMENTS),
                egui::Stroke::new(frame_width / strips, color),
            ));
        }
    }
}

/// 角部木纹（圆角工单 01）：四角没有直边可拉线，细纹改沿弧
/// 走向——在框带宽度内取随机分位定半径、在弧长内取随机起止
/// 描短弧，分布参数与直边细纹一致（暗纹居多、透明度 10–26）。
/// 只服务木质预设（调用点判断），种子独立于直边细纹。
fn paint_corner_grain(painter: &egui::Painter, shading: &BuiltinShading, base: egui::Color32) {
    let outer = shading.corners.outer;
    let frame_width = shading.layers.mat.left - shading.layers.body.left;
    if outer.radius <= 0.0 || frame_width <= 0.0 {
        return;
    }
    let mut grain = Grain(0x27D4_EB2F);
    for ci in 0..4 {
        let arc = CornerArc::of(&outer.centers, ci);
        for _ in 0..5 {
            let frac = grain.range(0.08, 0.92);
            let start = grain.range(0.0, 0.5);
            let len = grain.range(0.25, 1.0 - start);
            let dark = grain.range(0.0, 1.0) < 0.62;
            let tone = if dark {
                grain.range(0.78, 0.90)
            } else {
                grain.range(1.10, 1.22)
            };
            let alpha = grain.range(10.0, 26.0) as u8;
            let radius = outer.radius - frac * frame_width;
            if radius <= 1.0 {
                continue;
            }
            let points = arc.points(radius, start, start + len, 6);
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(1.0, with_alpha(shade(base, tone), alpha)),
            ));
        }
    }
}

/// 确定性伪随机（xorshift32，固定种子）：木纹细纹的位置与明暗
/// 只由序号决定、逐帧不变——纹理是合成的，但不能闪烁。
struct Grain(u32);

impl Grain {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// [lo, hi) 均匀取样。
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        let unit = (self.next() >> 8) as f32 / (1u32 << 24) as f32;
        lo + (hi - lo) * unit
    }
}

/// 木纹细纹：沿每条边的长度方向拉出的低透明短线，跨带宽散布、
/// 明暗交替，读作纹理而非条纹。只服务木质预设（胡桃木 /
/// 原木）；黑、白是漆面，没有木纹。
fn paint_grain(painter: &egui::Painter, shading: &BuiltinShading, base: egui::Color32) {
    let mut grain = Grain(0x9E37_79B9);
    // 细纹只落在直边段内（圆角工单 01）：角区沿弧的纹理走向
    // 由弧带本身承载，直线细纹伸进角区会戳出外沿圆弧、画到
    // 桌面上去。钳制范围与截面条带的直边段同口径。
    let body = shading.layers.body;
    let outer_radius = shading.corners.outer.radius;
    let inner_radius = shading
        .corners
        .frame_inner
        .map_or(0.0, |boundary| boundary.radius);
    for edge in &shading.edges {
        let rect = edge.rect;
        if rect.width() < 8.0 || rect.height() < 8.0 {
            continue;
        }
        let horizontal = matches!(edge.edge, FrameEdge::Top | FrameEdge::Bottom);
        let (length, thickness) = if horizontal {
            (rect.width(), rect.height())
        } else {
            (rect.height(), rect.width())
        };
        for _ in 0..GRAIN_STREAKS {
            let frac = grain.range(0.08, 0.92);
            let start = grain.range(0.0, 0.5);
            let len = grain.range(0.25, 1.0 - start);
            let dark = grain.range(0.0, 1.0) < 0.62;
            let tone = if dark {
                grain.range(0.78, 0.90)
            } else {
                grain.range(1.10, 1.22)
            };
            let alpha = grain.range(10.0, 26.0) as u8;
            let stroke = egui::Stroke::new(1.0, with_alpha(shade(base, tone), alpha));
            let (p1, p2) = if horizontal {
                let y = rect.top + thickness * frac;
                let x0 = (rect.left + length * start).max(body.left + outer_radius);
                let x1 = (rect.left + length * (start + len)).min(body.right - outer_radius);
                (egui::pos2(x0, y), egui::pos2(x1, y))
            } else {
                let x = rect.left + thickness * frac;
                let y0 = (rect.top + length * start)
                    .max((body.top + outer_radius).max(rect.top + inner_radius));
                let y1 = (rect.top + length * (start + len))
                    .min((body.bottom - outer_radius).min(rect.bottom - inner_radius));
                (egui::pos2(x, y0), egui::pos2(x, y1))
            };
            if (p2.x - p1.x).abs() + (p2.y - p1.y).abs() >= 2.0 {
                painter.line_segment([p1, p2], stroke);
            }
        }
    }
}

/// 造型线：外沿压线（暗）、凹槽（更暗）、内唇（亮），都是 1px
/// 内侧描边；带透明度落笔，避免在渐变上压出硬界。描边半径
/// 取布局接缝按分位插值给出的值，线在角部沿弧连续（圆角
/// 工单 01）。
fn paint_profile(painter: &egui::Painter, profile: &ProfileLines, base: egui::Color32) {
    let line = |rect: LayoutRect, radius: f32, tone: f32, alpha: u8| {
        painter.rect_stroke(
            to_egui_rect(rect),
            radius,
            egui::Stroke::new(1.0, with_alpha(shade(base, tone), alpha)),
            egui::StrokeKind::Inside,
        );
    };
    line(profile.outer_rim, profile.outer_radius, 0.55, 170);
    if let (Some(groove), Some(radius)) = (profile.groove, profile.groove_radius) {
        line(groove, radius, 0.50, 130);
    }
    line(profile.lip, profile.lip_radius, 1.22, 150);
}

/// 贴边投影：逐层内收的描边环，环宽 = 深度 / 环数，每环透明度
/// 按环中心分位线性淡出——边界最深、向内消失。框影与画面影
/// 共用这一落笔法，只是落点与深浅由布局接缝分别给出。环的
/// 圆角半径随内收量同步递减，影在角部沿弧连续（圆角工单 01）。
fn paint_fade_band(painter: &egui::Painter, band: &FadeBand) {
    if band.depth <= 0.0 || band.max_alpha == 0 {
        return;
    }
    let step = band.depth / FADE_RINGS as f32;
    for i in 0..FADE_RINGS {
        let frac = (i as f32 + 0.5) / FADE_RINGS as f32;
        let alpha = (band.max_alpha as f32 * (1.0 - frac)) as u8;
        let inset = step * i as f32;
        painter.rect_stroke(
            to_egui_rect(band.rect.shrink_uniform(inset)),
            (band.radius - inset).max(0.0),
            egui::Stroke::new(step, egui::Color32::from_black_alpha(alpha)),
            egui::StrokeKind::Inside,
        );
    }
}

/// 卡纸斜切面：外沿矩形与画面之间的近白色一圈。以四条带落笔
/// 而非整幅填充——空状态下画面区域保持卡纸面原色，只露切面
/// 这一圈。条带只画直边段：角部的环扇由画面角部修补在插画
/// 落笔后补齐（圆角工单 01），条带伸进角区会盖住圆弧口径。
fn paint_bevel(painter: &egui::Painter, outer: LayoutRect, image: LayoutRect, image_radius: f32) {
    let (x0, x1) = (image.left + image_radius, image.right - image_radius);
    let (y0, y1) = (image.top + image_radius, image.bottom - image_radius);
    let strips = [
        LayoutRect::new(x0, outer.top, x1, image.top),
        LayoutRect::new(x0, image.bottom, x1, outer.bottom),
        LayoutRect::new(outer.left, y0, image.left, y1),
        LayoutRect::new(image.right, y0, outer.right, y1),
    ];
    for strip in strips {
        if strip.width() > 0.0 && strip.height() > 0.0 {
            painter.rect_filled(to_egui_rect(strip), 0.0, BEVEL_COLOR);
        }
    }
}

/// 一个角的扇形填充参数（圆角工单 01）：角的圆弧几何、矩形
/// 角点、外弧半径、可选的内弧半径（在场时填两弧之间的环
/// 扇）与填充色，打包成一份落笔参数。
struct CornerSector {
    arc: CornerArc,
    corner: (f32, f32),
    outer_radius: f32,
    inner_radius: Option<f32>,
    color: egui::Color32,
}

/// 一个角的扇形填充路径（圆角工单 01）：外弧与角点围成的
/// 区域；给了内弧半径时是两弧之间的环扇。路径 = 角点 → 沿
/// 外弧自一侧切点到另一侧切点 →（环扇时）沿内弧反向回到
/// 起点一侧 → 闭合。弧以采样折线近似，填充走路径填充（可
/// 处理弧内凹的形状）。
fn fill_corner_sector(painter: &egui::Painter, sector: CornerSector) {
    if sector.outer_radius <= 0.0 {
        return;
    }
    let mut points = vec![egui::pos2(sector.corner.0, sector.corner.1)];
    points.extend(
        sector
            .arc
            .points(sector.outer_radius, 0.0, 1.0, ARC_SEGMENTS),
    );
    if let Some(inner) = sector.inner_radius {
        let mut inner_points = sector.arc.points(inner, 0.0, 1.0, ARC_SEGMENTS);
        inner_points.reverse();
        points.extend(inner_points);
    }
    painter.add(egui::Shape::Path(egui::epaint::PathShape {
        points,
        closed: true,
        fill: sector.color,
        stroke: egui::epaint::PathStroke::NONE,
    }));
}

/// 画面四角的覆盖修补（圆角工单 01，覆盖式内角）：插画按矩形
/// 落笔后，角尖的方形区域还压在内沿弧线之外——按布局接缝的
/// 画面边缘圆心与半径，把每个角在弧外的部分重新盖回它应有
/// 的层：卡纸在场时弧外是卡纸面、斜切面外沿与画面边缘之间
/// 是斜切面环扇；卡纸缺席时整片是框带收尾（归属边的内唇
/// 档位色，与角弧带的内圈同色）。空状态无插画可压，本修补
/// 只补斜切面条带在角区留下的缺口，颜色与周围一致、无痕。
fn paint_image_corner_patches(
    painter: &egui::Painter,
    shading: &BuiltinShading,
    base: egui::Color32,
) {
    let image = shading.layers.image;
    let boundary = shading.corners.image;
    if boundary.radius <= 0.0 {
        return;
    }
    let corners = [
        (image.left, image.top),
        (image.right, image.top),
        (image.right, image.bottom),
        (image.left, image.bottom),
    ];
    for (ci, &corner) in corners.iter().enumerate() {
        let arc = CornerArc::of(&boundary.centers, ci);
        match &shading.mat {
            Some(mat) => {
                fill_corner_sector(
                    painter,
                    CornerSector {
                        arc,
                        corner,
                        outer_radius: mat.bevel_radius,
                        inner_radius: None,
                        color: MAT_COLOR,
                    },
                );
                fill_corner_sector(
                    painter,
                    CornerSector {
                        arc,
                        corner,
                        outer_radius: mat.bevel_radius,
                        inner_radius: Some(boundary.radius),
                        color: BEVEL_COLOR,
                    },
                );
            }
            None => {
                let owner = if ci < 2 {
                    &shading.edges[0]
                } else {
                    &shading.edges[2]
                };
                let color = shade(base, owner.stops[3].1);
                fill_corner_sector(
                    painter,
                    CornerSector {
                        arc,
                        corner,
                        outer_radius: boundary.radius,
                        inner_radius: None,
                        color,
                    },
                );
            }
        }
    }
}

impl FrameApp {
    /// 内置画框模式的一帧绘制：落点与光影几何全部来自布局
    /// 接缝的着色计划，本函数只按预设选色并照单落笔——框带
    /// 截面渐变打底（先以基色圆角整幅铺底兜住条带接缝），直
    /// 边段与四角弧段、木纹与造型线随后，卡纸面、框影、斜切
    /// 面、画面、画面四角修补、画面影、内沿线依次叠上。
    /// `frame` 是生效外观携带的生效参数：内置
    /// 模式下是用户参数，自定义回退时是默认参数——本函数不读
    /// 宿主保存的用户参数，画出来的永远是裁决给的那一份。
    pub(crate) fn paint_builtin_frame(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        frame: BuiltinFrame,
    ) {
        let shading = builtin_shading(
            to_layout_rect(rect),
            AppearanceMode::Builtin.insets(frame),
            frame.frame_width,
            frame.mat_width,
        );
        let layers = shading.layers;
        let base = preset_base(frame.preset);
        // 框体：基色以圆角整幅铺底（外沿圆弧之外透出桌面），
        // 截面渐变直边段与四角弧段随后覆上（圆角工单 01）。
        painter.rect_filled(
            to_egui_rect(layers.body),
            shading.corners.outer.radius,
            base,
        );
        for edge in &shading.edges {
            paint_edge(painter, edge, base, &shading);
        }
        paint_corner_arcs(painter, &shading, base);
        if matches!(frame.preset, FramePreset::Walnut | FramePreset::Oak) {
            paint_grain(painter, &shading, base);
            paint_corner_grain(painter, &shading, base);
        }
        if let Some(profile) = &shading.profile {
            paint_profile(painter, profile, base);
        }
        // 卡纸：宽为 0 时 mat 与 image 重合，跳过整组卡纸着色
        // 即退化两层（着色计划同口径缺席）。卡纸填充的圆角取
        // 框体内沿半径；框宽为 0 时卡纸外沿即窗口外沿，取外
        // 沿半径。
        if let Some(mat) = &shading.mat {
            let mat_radius = shading
                .corners
                .frame_inner
                .map_or(shading.corners.outer.radius, |boundary| boundary.radius);
            painter.rect_filled(to_egui_rect(layers.mat), mat_radius, MAT_COLOR);
            if let Some(cast) = &mat.cast_shadow {
                paint_fade_band(painter, cast);
            }
            paint_bevel(
                painter,
                mat.bevel,
                layers.image,
                shading.corners.image.radius,
            );
        }
        // 画面：纹理只落画面区域（解码口径与此一致）；无图时
        // 提示文字居中于画面区域——卡纸在场用深色字，否则沿用
        // 浅色字。
        let image_rect = to_egui_rect(layers.image);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        if let Some(texture) = &self.texture {
            painter.image(texture.id(), image_rect, uv, egui::Color32::WHITE);
        } else {
            let color = if frame.mat_width > 0.0 {
                egui::Color32::from_rgb(105, 95, 80)
            } else {
                egui::Color32::from_rgb(200, 200, 200)
            };
            painter.text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                "右键打开设置，选择图库文件夹",
                egui::FontId::proportional(15.0),
                color,
            );
        }
        // 画面四角的覆盖修补：插画的方形角尖盖回内沿弧外应有
        // 的层（卡纸面 / 斜切面环扇 / 框带收尾），插画本体不
        // 裁剪、解码与落点矩形不变（圆角工单 01）。
        paint_image_corner_patches(painter, &shading, base);
        // 卡纸投在画面上的影：压在画面边缘向内淡出，空状态下
        // 压在卡纸面上读作卡纸开口的内影，同样成立。
        if let Some(mat) = &shading.mat {
            paint_fade_band(painter, &mat.image_shadow);
        }
        // 内沿细线：沿画面边界贴内侧一周（圆角半径取画面边缘
        // 边界），与斜切面共同收出画芯的压痕。
        painter.rect_stroke(
            image_rect,
            shading.corners.image.radius,
            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(INNER_EDGE_ALPHA)),
            egui::StrokeKind::Inside,
        );
    }
}
