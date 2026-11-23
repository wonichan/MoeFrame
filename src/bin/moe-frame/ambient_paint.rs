//! 无方向组转场的绘制（切换动画专题工单 03）：百叶帘 / 墨晕 /
//! 像素故障。转场状态、墙钟推进、完成回执与纹理转正都在共享
//! 骨架（transition.rs，工单 02）；本文件只负责这三类在
//! `paint_transition` 分发里的落笔，纯计算（条带、扇形、块序、
//! 抖动）在库 transition_ambient，tests 钉住。三种都不分方向：
//! 本文件的函数签名里没有方向参数，上一张/下一张同一套画法。

use super::*;
use moe_frame::transition_ambient as ambient;

/// 墨晕扇形的羽化外环相对主环的半径倍率：外环顶点全透明，
/// 主环到外环之间 alpha 渐降，晕边因此是软的。
const INK_FEATHER_SCALE: f32 = 1.12;

/// 灰度 tint：1.0 原样，越小越暗（egui tint 是乘法）。
fn gray_tint(shade: f32) -> egui::Color32 {
    let v = (shade.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(v, v, v)
}

/// [u0, v0, u1, v1] → egui UV 矩形。
fn uv_rect(uv: [f32; 4]) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(uv[0], uv[1]), egui::pos2(uv[2], uv[3]))
}

fn full_uv() -> egui::Rect {
    uv_rect([0.0, 0.0, 1.0, 1.0])
}

/// 百叶帘：旧图垫底（fit 居中不拉伸），新图 8 条竖帘自左至右
/// 错峰从顶部落下——每条是同一张新图纹理的 UV 子区域，条内
/// 明暗随揭示进度由暗到亮（上沿翻下的翻转感）。
pub(crate) fn paint_blinds(
    painter: &egui::Painter,
    p: f32,
    old: &egui::TextureHandle,
    new: &egui::TextureHandle,
    stage: egui::Rect,
) {
    let painter = painter.with_clip_rect(stage);
    painter.image(
        old.id(),
        crate::transition::fit_rect(stage, old.size_vec2()),
        full_uv(),
        egui::Color32::WHITE,
    );
    let stage_layout = to_layout_rect(stage);
    for strip in ambient::blinds_strips(stage_layout, p) {
        painter.image(
            new.id(),
            to_egui_rect(strip.dest),
            uv_rect(strip.uv),
            gray_tint(strip.shade),
        );
    }
}

/// 墨晕：旧图先按批准近似处理（压暗 + 渐强灰纱，冒充去色），
/// 新图以画面中心为圆心用扇形 Mesh（单纹理）圆形扩散晕开，
/// 最外一环羽化到全透明。
pub(crate) fn paint_ink(
    painter: &egui::Painter,
    p: f32,
    old: &egui::TextureHandle,
    new: &egui::TextureHandle,
    stage: egui::Rect,
) {
    let painter = painter.with_clip_rect(stage);
    let stage_layout = to_layout_rect(stage);
    let fan = ambient::ink_fan(stage_layout, p);
    let old_rect = crate::transition::fit_rect(stage, old.size_vec2());
    painter.image(old.id(), old_rect, full_uv(), gray_tint(fan.old_shade));
    if fan.veil_alpha > 0 {
        painter.rect_filled(
            old_rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(128, 128, 128, fan.veil_alpha),
        );
    }
    if fan.radius > 0.0 {
        painter.add(ink_fan_shape(&fan, stage_layout, new.id()));
    }
}

/// 像素故障：旧图在开头约 120ms 内整体错位抖动（窗口外恒零，
/// 库函数定案），新图按 12×8 块以转场种子洗定的顺序逐块拼齐
/// ——每块是同一张新图纹理的 UV 子区域。
pub(crate) fn paint_pixel(
    painter: &egui::Painter,
    p: f32,
    elapsed_ms: u64,
    seed: u64,
    old: &egui::TextureHandle,
    new: &egui::TextureHandle,
    stage: egui::Rect,
) {
    let painter = painter.with_clip_rect(stage);
    let jitter = ambient::pixel_jitter(elapsed_ms, seed);
    let old_rect = crate::transition::fit_rect(stage, old.size_vec2())
        .translate(egui::vec2(jitter[0], jitter[1]));
    painter.image(old.id(), old_rect, full_uv(), egui::Color32::WHITE);
    let stage_layout = to_layout_rect(stage);
    let order = ambient::pixel_order(seed);
    let visible = ambient::pixel_visible_count(p);
    for &block in &order[..visible] {
        let (dest, uv) = ambient::pixel_block(stage_layout, block as usize);
        painter.image(
            new.id(),
            to_egui_rect(dest),
            uv_rect(uv),
            egui::Color32::WHITE,
        );
    }
}

/// 墨晕的扇形 Mesh（单 Mesh 单纹理）：圆心顶点 + 主环（不透明）
/// + 羽化外环（全透明），三角扇与羽化四边形带一次拼成。
fn ink_fan_shape(
    fan: &ambient::InkFan,
    stage: LayoutRect,
    texture: egui::TextureId,
) -> egui::Shape {
    let segments = ambient::INK_SEGMENTS;
    let mut mesh = egui::Mesh::with_texture(texture);
    let center_uv = [
        (fan.center[0] - stage.left) / stage.width(),
        (fan.center[1] - stage.top) / stage.height(),
    ];
    mesh.vertices.push(egui::epaint::Vertex {
        pos: egui::pos2(fan.center[0], fan.center[1]),
        uv: egui::pos2(center_uv[0], center_uv[1]),
        color: egui::Color32::WHITE,
    });
    // 主环顶点下标 1..=segments，羽化外环紧随其后。
    for ring_scale in [1.0, INK_FEATHER_SCALE] {
        let alpha = if ring_scale == 1.0 { 255 } else { 0 };
        for i in 0..segments {
            let (pos, uv) = ambient::ink_ring_point(stage, fan, i, ring_scale);
            mesh.vertices.push(egui::epaint::Vertex {
                pos: egui::pos2(pos[0], pos[1]),
                uv: egui::pos2(uv[0], uv[1]),
                color: egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
            });
        }
    }
    let inner = |i: usize| (1 + i % segments) as u32;
    let outer = |i: usize| (1 + segments + i % segments) as u32;
    for i in 0..segments {
        mesh.indices.extend_from_slice(&[0, inner(i), inner(i + 1)]);
        mesh.indices.extend_from_slice(&[
            inner(i),
            outer(i),
            outer(i + 1),
            inner(i),
            outer(i + 1),
            inner(i + 1),
        ]);
    }
    egui::Shape::mesh(mesh)
}
