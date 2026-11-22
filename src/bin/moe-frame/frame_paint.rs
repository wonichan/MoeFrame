//! 内置画框绘制（V2 相框外观专题工单 02）：三层（框体 / 卡纸 /
//! 画面）+ 内沿细线 + 自绘软阴影。绘制矩形全部来自库 frame_layout
//! 的布局接缝，本文件只定颜色与画法：框体颜色按预设合成，简易
//! 木纹以框带内的同心细线冒充，不引入图片素材。

use super::*;

/// 卡纸颜色：默认米白（spec 定案）。
const MAT_COLOR: egui::Color32 = egui::Color32::from_rgb(246, 240, 224);
/// 内沿细线：紧贴画面边界的半透明暗线（裱画时画芯与卡纸之间的
/// 那道压痕），任何预设下都成立。
const INNER_EDGE_ALPHA: u8 = 96;

/// 框体预设的三色：（基色，亮部，暗部）——亮/暗用于框带内的同心
/// 细线与内外沿的明暗线，合成简易木纹与倒角感。
fn preset_colors(preset: FramePreset) -> (egui::Color32, egui::Color32, egui::Color32) {
    match preset {
        FramePreset::Walnut => (
            egui::Color32::from_rgb(107, 74, 47),
            egui::Color32::from_rgb(146, 108, 70),
            egui::Color32::from_rgb(74, 49, 29),
        ),
        FramePreset::Oak => (
            egui::Color32::from_rgb(198, 158, 102),
            egui::Color32::from_rgb(226, 192, 138),
            egui::Color32::from_rgb(164, 126, 74),
        ),
        FramePreset::Black => (
            egui::Color32::from_rgb(26, 26, 28),
            egui::Color32::from_rgb(62, 62, 66),
            egui::Color32::from_rgb(8, 8, 10),
        ),
        FramePreset::White => (
            egui::Color32::from_rgb(240, 238, 233),
            egui::Color32::from_rgb(255, 255, 255),
            egui::Color32::from_rgb(206, 202, 192),
        ),
    }
}

impl FrameApp {
    /// 内置画框模式的一帧绘制：阴影与三层都在窗口矩形内完成——
    /// 窗口尺寸即外框尺寸，窗外没有可绘制的像素，自绘软阴影落在
    /// 框体外沿内侧的扩散带上（布局接缝 shadow_layers 产出层序）。
    /// `frame` 是生效外观携带的生效参数：内置模式下是用户参数，
    /// 自定义回退时是默认参数——本函数不读宿主保存的用户参数，
    /// 画出来的永远是裁决给的那一份。
    pub(crate) fn paint_builtin_frame(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        frame: BuiltinFrame,
    ) {
        let layers = builtin_layers(
            to_layout_rect(rect),
            AppearanceMode::Builtin.insets(frame),
            frame.frame_width,
        );
        let (base, light, dark) = preset_colors(frame.preset);
        // 框体：基色铺满，再在框带内画两道同心细线冒充木纹走向，
        // 外沿亮线、内沿暗线收出倒角感（框宽太小时细线自动挤在
        // 带内，不越界到卡纸）。
        painter.rect_filled(to_egui_rect(layers.body), 0.0, base);
        if frame.frame_width >= 3.0 {
            for (fraction, color) in [(1.0 / 3.0, dark), (2.0 / 3.0, light)] {
                let inset = frame.frame_width * fraction;
                painter.rect_stroke(
                    to_egui_rect(layers.body.shrink_uniform(inset)),
                    0.0,
                    egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 70),
                    ),
                    egui::StrokeKind::Inside,
                );
            }
        }
        // 卡纸：宽为 0 时 mat 与 image 重合，跳过填充即退化两层。
        if frame.mat_width > 0.0 {
            painter.rect_filled(to_egui_rect(layers.mat), 0.0, MAT_COLOR);
        }
        // 画面：纹理只落画面区域（解码口径与此一致）；无图时提示
        // 文字居中于画面区域——卡纸在场用深色字，否则沿用浅色字。
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
        // 自绘软阴影：扩散层沿框体外沿内侧逐层以内侧描边画出，
        // 边缘最深、向内淡出；只在内置模式出现（极简/自定义不经
        // 本函数）。画在三层之上、内沿线之下，阴影带因此压在框体
        // 与卡纸的外侧部分，不碰画面本体（带宽 12px 小于常规
        // 框宽 + 卡纸宽之和）。
        for (layer, alpha) in shadow_layers(layers.body) {
            painter.rect_stroke(
                to_egui_rect(layer),
                0.0,
                egui::Stroke::new(SHADOW_STEP_PX, egui::Color32::from_black_alpha(alpha)),
                egui::StrokeKind::Inside,
            );
        }
        // 内沿细线：沿画面边界贴内侧一周。
        painter.rect_stroke(
            image_rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(INNER_EDGE_ALPHA)),
            egui::StrokeKind::Inside,
        );
    }
}
