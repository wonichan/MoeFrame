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
/// ——分位方向永远是外沿 → 内沿，与边的朝向无关。
fn paint_edge(painter: &egui::Painter, edge: &EdgeShade, base: egui::Color32) {
    let rect = edge.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    for i in 0..EDGE_STRIPS {
        let f0 = i as f32 / EDGE_STRIPS as f32;
        let f1 = (i + 1) as f32 / EDGE_STRIPS as f32;
        let color = shade(base, tone_at(&edge.stops, (f0 + f1) / 2.0));
        let strip = match edge.edge {
            FrameEdge::Top => LayoutRect::new(
                rect.left,
                rect.top + rect.height() * f0,
                rect.right,
                rect.top + rect.height() * f1,
            ),
            FrameEdge::Bottom => LayoutRect::new(
                rect.left,
                rect.bottom - rect.height() * f1,
                rect.right,
                rect.bottom - rect.height() * f0,
            ),
            FrameEdge::Left => LayoutRect::new(
                rect.left + rect.width() * f0,
                rect.top,
                rect.left + rect.width() * f1,
                rect.bottom,
            ),
            FrameEdge::Right => LayoutRect::new(
                rect.right - rect.width() * f1,
                rect.top,
                rect.right - rect.width() * f0,
                rect.bottom,
            ),
        };
        painter.rect_filled(to_egui_rect(strip), 0.0, color);
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
                (
                    egui::pos2(rect.left + length * start, y),
                    egui::pos2(rect.left + length * (start + len), y),
                )
            } else {
                let x = rect.left + thickness * frac;
                (
                    egui::pos2(x, rect.top + length * start),
                    egui::pos2(x, rect.top + length * (start + len)),
                )
            };
            painter.line_segment([p1, p2], stroke);
        }
    }
}

/// 造型线：外沿压线（暗）、凹槽（更暗）、内唇（亮），都是 1px
/// 内侧描边；带透明度落笔，避免在渐变上压出硬界。
fn paint_profile(painter: &egui::Painter, profile: &ProfileLines, base: egui::Color32) {
    let line = |rect: LayoutRect, tone: f32, alpha: u8| {
        painter.rect_stroke(
            to_egui_rect(rect),
            0.0,
            egui::Stroke::new(1.0, with_alpha(shade(base, tone), alpha)),
            egui::StrokeKind::Inside,
        );
    };
    line(profile.outer_rim, 0.55, 170);
    if let Some(groove) = profile.groove {
        line(groove, 0.50, 130);
    }
    line(profile.lip, 1.22, 150);
}

/// 贴边投影：逐层内收的描边环，环宽 = 深度 / 环数，每环透明度
/// 按环中心分位线性淡出——边界最深、向内消失。框影与画面影
/// 共用这一落笔法，只是落点与深浅由布局接缝分别给出。
fn paint_fade_band(painter: &egui::Painter, band: &FadeBand) {
    if band.depth <= 0.0 || band.max_alpha == 0 {
        return;
    }
    let step = band.depth / FADE_RINGS as f32;
    for i in 0..FADE_RINGS {
        let frac = (i as f32 + 0.5) / FADE_RINGS as f32;
        let alpha = (band.max_alpha as f32 * (1.0 - frac)) as u8;
        painter.rect_stroke(
            to_egui_rect(band.rect.shrink_uniform(step * i as f32)),
            0.0,
            egui::Stroke::new(step, egui::Color32::from_black_alpha(alpha)),
            egui::StrokeKind::Inside,
        );
    }
}

/// 卡纸斜切面：外沿矩形与画面之间的近白色一圈。以四条带落笔
/// 而非整幅填充——空状态下画面区域保持卡纸面原色，只露切面
/// 这一圈。
fn paint_bevel(painter: &egui::Painter, outer: LayoutRect, image: LayoutRect) {
    let strips = [
        LayoutRect::new(outer.left, outer.top, outer.right, image.top),
        LayoutRect::new(outer.left, image.bottom, outer.right, outer.bottom),
        LayoutRect::new(outer.left, image.top, image.left, image.bottom),
        LayoutRect::new(image.right, image.top, outer.right, image.bottom),
    ];
    for strip in strips {
        if strip.width() > 0.0 && strip.height() > 0.0 {
            painter.rect_filled(to_egui_rect(strip), 0.0, BEVEL_COLOR);
        }
    }
}

impl FrameApp {
    /// 内置画框模式的一帧绘制：落点与光影几何全部来自布局
    /// 接缝的着色计划，本函数只按预设选色并照单落笔——框带
    /// 截面渐变打底（先以基色整幅铺底兜住条带接缝），木纹与
    /// 造型线随后，卡纸面、框影、斜切面、画面、画面影、内沿
    /// 线依次叠上。`frame` 是生效外观携带的生效参数：内置
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
        // 框体：基色整幅铺底，截面渐变分边覆上。
        painter.rect_filled(to_egui_rect(layers.body), 0.0, base);
        for edge in &shading.edges {
            paint_edge(painter, edge, base);
        }
        if matches!(frame.preset, FramePreset::Walnut | FramePreset::Oak) {
            paint_grain(painter, &shading, base);
        }
        if let Some(profile) = &shading.profile {
            paint_profile(painter, profile, base);
        }
        // 卡纸：宽为 0 时 mat 与 image 重合，跳过整组卡纸着色
        // 即退化两层（着色计划同口径缺席）。
        if let Some(mat) = &shading.mat {
            painter.rect_filled(to_egui_rect(layers.mat), 0.0, MAT_COLOR);
            if let Some(cast) = &mat.cast_shadow {
                paint_fade_band(painter, cast);
            }
            paint_bevel(painter, mat.bevel, layers.image);
        }
        // 画面：纹理只落画面区域（解码口径与此一致）；无图时
        // 提示文字居中于画面区域——卡纸在场用深色字，否则沿用
        // 浅色字。
        let image_rect = to_egui_rect(layers.image);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        if self.texture.is_some() {
            self.paint_crossfade(painter, image_rect, uv);
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
        // 卡纸投在画面上的影：压在画面边缘向内淡出，空状态下
        // 压在卡纸面上读作卡纸开口的内影，同样成立。
        if let Some(mat) = &shading.mat {
            paint_fade_band(painter, &mat.image_shadow);
        }
        // 内沿细线：沿画面边界贴内侧一周，与斜切面共同收出
        // 画芯的压痕。
        painter.rect_stroke(
            image_rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(INNER_EDGE_ALPHA)),
            egui::StrokeKind::Inside,
        );
    }
}
