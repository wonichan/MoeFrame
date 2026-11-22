//! 宿主转场渲染（切换动画专题工单 02）：核心在 `StartTransition`
//! 里把动画种类、方向与时长全部定死，宿主只按墙钟进度绘制，
//! 到期回送 `TransitionCompleted`——记账与纹理转正都等完成时刻。
//! 本文件是共享骨架 + 有方向组（卷页 / 抽卡）；无方向组（百叶
//! 帘 / 墨晕 / 像素故障）的落笔在 ambient_paint（工单 03），经
//! `paint_transition` 的分发补齐，时长与回执仍按骨架走。

use super::*;
use moe_frame::transition_ambient as ambient;

#[cfg(test)]
mod tests {
    use super::{card_frame, curl_frame, CardSide};
    use crate::egui::{pos2, Rect};
    use moe_frame::playlist::Direction;

    fn stage() -> Rect {
        Rect::from_min_max(pos2(10.0, 20.0), pos2(410.0, 320.0))
    }

    // 墙钟进度与 fit 矩形的断言在库 tests/transition_ambient.rs
    // 钉住（宿主 progress/fit_rect 只是类型转换适配，见下）。

    #[test]
    fn curl_reveals_from_the_direction_side() {
        // 下一张（Next）：揭示前沿自左向右推进，进度 0 时新图不可见、1 时全显。
        let start = curl_frame(Direction::Next, 0.0, stage());
        assert_eq!(start.reveal.width(), 0.0);
        assert_eq!(start.reveal.left(), stage().left());
        let mid = curl_frame(Direction::Next, 0.5, stage());
        // ease-out：半程时前沿已过中点。
        assert!(mid.edge_x > stage().center().x);
        assert_eq!(mid.reveal.right(), mid.edge_x);
        let end = curl_frame(Direction::Next, 1.0, stage());
        assert_eq!(end.reveal, stage());
        // 上一张（Prev）：镜像——揭示区贴右，前沿自右向左。
        let mid = curl_frame(Direction::Prev, 0.5, stage());
        assert!(mid.edge_x < stage().center().x);
        assert_eq!(mid.reveal.left(), mid.edge_x);
        assert_eq!(mid.reveal.right(), stage().right());
    }

    #[test]
    fn curl_darkens_the_old_image_slightly() {
        let start = curl_frame(Direction::Next, 0.0, stage());
        assert_eq!(start.old_brightness, 1.0);
        let end = curl_frame(Direction::Next, 1.0, stage());
        // 轻微变暗：终点亮度落在 (0.6, 0.9)，不是全黑也不是不变。
        assert!(end.old_brightness > 0.6 && end.old_brightness < 0.9);
    }

    #[test]
    fn card_flips_around_the_vertical_axis_in_two_halves() {
        // 前半：旧图在场，横向收拢（scale_x 1→0），高度不变（绕竖轴）。
        let start = card_frame(Direction::Next, 0.0);
        assert_eq!(start.side, CardSide::Old);
        assert_eq!(start.scale_x, 1.0);
        let quarter = card_frame(Direction::Next, 0.25);
        assert_eq!(quarter.side, CardSide::Old);
        assert!(quarter.scale_x > 0.0 && quarter.scale_x < 1.0);
        // 后半：新图展开（scale_x 0→1）。
        let three_quarter = card_frame(Direction::Next, 0.75);
        assert_eq!(three_quarter.side, CardSide::New);
        assert!(three_quarter.scale_x > 0.0 && three_quarter.scale_x < 1.0);
        let end = card_frame(Direction::Next, 1.0);
        assert_eq!(end.side, CardSide::New);
        assert_eq!(end.scale_x, 1.0);
        assert_eq!(end.tilt, 0.0);
        // 翻面中点两侧都不露图：恰在中点 scale_x 为 0。
        let mid_old = card_frame(Direction::Next, 0.5 - f32::EPSILON);
        assert!(mid_old.scale_x < 0.01);
    }

    #[test]
    fn card_tilt_reverses_with_direction() {
        let next = card_frame(Direction::Next, 0.25);
        let prev = card_frame(Direction::Prev, 0.25);
        assert!(next.tilt > 0.0);
        assert_eq!(prev.tilt, -next.tilt);
        // 展开后半程同理反号。
        let next = card_frame(Direction::Next, 0.75);
        let prev = card_frame(Direction::Prev, 0.75);
        assert_eq!(prev.tilt, -next.tilt);
    }
}

/// 宿主持有的在飞转场：种类/方向/时长全部来自核心的
/// `StartTransition` 事件，宿主不自选、不重抽随机。
pub(crate) struct HostTransition {
    pub(crate) kind: ConcreteAnimation,
    pub(crate) direction: Direction,
    pub(crate) started_ms: u64,
    pub(crate) duration_ms: u64,
    /// 像素故障的块序种子（工单 03）：开始时由起始时刻与种类
    /// 派生，每条动画不同；同条内逐帧不变（逐帧重洗会闪烁）。
    pub(crate) seed: u64,
}

impl HostTransition {
    pub(crate) fn new(
        kind: ConcreteAnimation,
        direction: Direction,
        started_ms: u64,
        duration_ms: u64,
    ) -> Self {
        let seed = started_ms.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ 0xD1B5_4A32_D192_ED03
            ^ (kind as u64 + 1).wrapping_mul(0xA076_1D64_78BD_642F);
        Self {
            kind,
            direction,
            started_ms,
            duration_ms,
            seed,
        }
    }

    /// 完成时刻（墙钟）：帧尾唤醒与到期判定共用。
    pub(crate) fn deadline_ms(&self) -> u64 {
        self.started_ms.saturating_add(self.duration_ms)
    }

    pub(crate) fn progress(&self, now_ms: u64) -> f32 {
        progress(self.started_ms, self.duration_ms, now_ms)
    }
}

/// 墙钟进度：宿主侧的 (started, now) 只是时间戳适配，数学走库
/// `transition_progress`（已流逝 / 总时长，钳到 [0, 1]；掉帧只
/// 跳进度、不延长总时长，spec 第 8 节；零时长视为立即完成）。
pub(crate) fn progress(started_ms: u64, duration_ms: u64, now_ms: u64) -> f32 {
    ambient::transition_progress(now_ms.saturating_sub(started_ms), duration_ms)
}

/// 把纹理按自身宽高比居中放进舞台的最大内接矩形：转场期间新
/// 旧图比例可不同，绝不拉伸（spec 第 7 节）。数学走库
/// `fit_rect`（LayoutRect + 宽高比口径），这里只做 egui 类型
/// 转换与舞台退化守卫；纹理尺寸非法时宽高比由库侧判退舞台。
pub(crate) fn fit_rect(stage: egui::Rect, tex_size: egui::Vec2) -> egui::Rect {
    if stage.width() <= 0.0 || stage.height() <= 0.0 {
        return stage;
    }
    to_egui_rect(ambient::fit_rect(
        to_layout_rect(stage),
        tex_size.x / tex_size.y,
    ))
}

/// 卷页一帧的几何（纯函数）：揭示区、卷边前沿横坐标、旧图亮度。
pub(crate) struct CurlFrame {
    pub(crate) reveal: egui::Rect,
    pub(crate) edge_x: f32,
    pub(crate) old_brightness: f32,
}

pub(crate) fn curl_frame(direction: Direction, p: f32, stage: egui::Rect) -> CurlFrame {
    let e = ambient::ease_out_cubic(p);
    let edge_x = match direction {
        Direction::Next => stage.left() + stage.width() * e,
        Direction::Prev => stage.right() - stage.width() * e,
    };
    let reveal = match direction {
        Direction::Next => egui::Rect::from_min_max(stage.min, egui::pos2(edge_x, stage.bottom())),
        Direction::Prev => egui::Rect::from_min_max(egui::pos2(edge_x, stage.top()), stage.max),
    };
    CurlFrame {
        reveal,
        edge_x,
        // 旧图同步轻微变暗：1.0 → 0.75。
        old_brightness: 1.0 - 0.25 * e,
    }
}

/// 抽卡当前在场的面：前半旧图翻走、后半新图翻正。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardSide {
    Old,
    New,
}

/// 抽卡一帧的几何（纯函数）：在场面、横向收拢比例（绕竖轴的
/// 仿射近似）、梯形倾斜量（逻辑像素，顶边偏移；符号随方向）。
pub(crate) struct CardFrame {
    pub(crate) side: CardSide,
    pub(crate) scale_x: f32,
    pub(crate) tilt: f32,
    pub(crate) brightness: f32,
}

pub(crate) fn card_frame(direction: Direction, p: f32) -> CardFrame {
    let p = p.clamp(0.0, 1.0);
    let sign = match direction {
        Direction::Next => 1.0,
        Direction::Prev => -1.0,
    };
    if p < 0.5 {
        let local = p * 2.0;
        let scale_x = (local * std::f32::consts::FRAC_PI_2).cos();
        CardFrame {
            side: CardSide::Old,
            scale_x,
            tilt: sign * (1.0 - scale_x),
            brightness: 0.55 + 0.45 * scale_x,
        }
    } else {
        let local = (p - 0.5) * 2.0;
        let scale_x = (local * std::f32::consts::FRAC_PI_2).sin();
        CardFrame {
            side: CardSide::New,
            scale_x,
            tilt: -sign * (1.0 - scale_x),
            brightness: 0.55 + 0.45 * scale_x,
        }
    }
}

fn gray_tint(brightness: f32) -> egui::Color32 {
    let v = (brightness.clamp(0.0, 1.0) * 255.0) as u8;
    egui::Color32::from_rgb(v, v, v)
}

/// 把一张纹理以任意四边形贴出（抽卡的梯形面）：单 Mesh、仿射
/// 插值，uv 满幅对应四角（左上/右上/右下/左下）。
fn paint_quad(
    painter: &egui::Painter,
    texture_id: egui::TextureId,
    corners: [egui::Pos2; 4],
    tint: egui::Color32,
) {
    let uv = [
        egui::pos2(0.0, 0.0),
        egui::pos2(1.0, 0.0),
        egui::pos2(1.0, 1.0),
        egui::pos2(0.0, 1.0),
    ];
    let mut mesh = egui::Mesh::with_texture(texture_id);
    for (pos, uv) in corners.iter().zip(uv) {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *pos,
            uv,
            color: tint,
        });
    }
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    painter.add(egui::Shape::mesh(mesh));
}

impl FrameApp {
    /// `StartTransition` 到达：置位宿主转场状态，不回送完成。
    /// 新图纹理此时已在 `incoming` 槽、旧图仍在 `texture` 槽，
    /// 两槽跨帧共存到完成时刻才转正（见 `advance_transition`）。
    /// 缺任一纹理（核心口径外）时退回工单 01 的秒回完成，不卡死。
    pub(crate) fn begin_transition(
        &mut self,
        ctx: &egui::Context,
        animation: ConcreteAnimation,
        direction: Direction,
        duration_ms: u64,
    ) {
        if self.texture.is_none() || self.incoming.is_none() {
            let effects = self.core.handle(Intent::TransitionCompleted {
                now_ms: now_ms(ctx),
            });
            self.dispatch(ctx, effects);
            return;
        }
        self.transition = Some(HostTransition::new(
            animation,
            direction,
            now_ms(ctx),
            duration_ms,
        ));
    }

    /// 每帧在绘制前推进转场：到期（墙钟）才回送完成，并在此刻
    /// 把 `incoming` 转正为当前纹理——转正时机从 `Reply::Loaded`
    /// 分支尾搬到这里，是提交点后移在宿主侧的落点；旧纹理随赋
    /// 值 drop 释放。回送的效果（含排队导航的新 Load）照常分发；
    /// 分发若又起新转场（同帧连环）则 incoming 已转正、无残留。
    pub(crate) fn advance_transition(&mut self, ctx: &egui::Context) {
        let finished = self
            .transition
            .as_ref()
            .is_some_and(|t| now_ms(ctx) >= t.deadline_ms());
        if !finished {
            return;
        }
        self.transition = None;
        let effects = self.core.handle(Intent::TransitionCompleted {
            now_ms: now_ms(ctx),
        });
        self.dispatch(ctx, effects);
        if self.incoming.is_some() {
            self.texture = self.incoming.take();
            self.pipeline.promote_incoming();
        }
        if self.core.view() == View::Empty {
            self.texture = None;
            self.pipeline.clear_current();
        }
    }

    /// 画舞台（图片区域）一帧：无转场时即现行行为（当前纹理满
    /// 幅）；转场时按种类分发绘制新旧两图。返回是否画出了图
    /// （调用方据此决定空状态提示）。只动图片区域，框体/卡纸/
    /// 相框图由调用方在本函数之外照常绘制。
    pub(crate) fn paint_stage(&self, painter: &egui::Painter, image_rect: egui::Rect) -> bool {
        let Some(transition) = &self.transition else {
            return match &self.texture {
                Some(texture) => {
                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    painter.image(texture.id(), image_rect, uv, egui::Color32::WHITE);
                    true
                }
                None => false,
            };
        };
        let (Some(old), Some(new)) = (&self.texture, &self.incoming) else {
            // 转场状态与纹理槽不一致（重扫等已在别处收尾）：不画，
            // 由 advance/收尾路径归位。
            return false;
        };
        let now = (painter.ctx().input(|input| input.time) * 1000.0) as u64;
        let p = transition.progress(now);
        let elapsed_ms = now.saturating_sub(transition.started_ms);
        paint_transition(painter, transition, p, elapsed_ms, old, new, image_rect);
        true
    }
}

/// 转场绘制分发：有方向组在本文件，无方向组三臂转 ambient_paint
/// （工单 03；三者无方向分支，direction 不传）。
fn paint_transition(
    painter: &egui::Painter,
    transition: &HostTransition,
    p: f32,
    elapsed_ms: u64,
    old: &egui::TextureHandle,
    new: &egui::TextureHandle,
    stage: egui::Rect,
) {
    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    match transition.kind {
        ConcreteAnimation::Curl => {
            let frame = curl_frame(transition.direction, p, stage);
            // 旧图垫底：按自身比例居中，同步轻微变暗。
            painter.image(
                old.id(),
                fit_rect(stage, old.size_vec2()),
                full_uv,
                gray_tint(frame.old_brightness),
            );
            // 新图在揭示区内满幅（其解码口径即新舞台比例）。
            painter.with_clip_rect(frame.reveal).image(
                new.id(),
                fit_rect(stage, new.size_vec2()),
                full_uv,
                egui::Color32::WHITE,
            );
            paint_curl_edge(painter, transition.direction, frame.edge_x, stage, p);
        }
        ConcreteAnimation::Card => {
            // 翻面期间舞台底色垫底（极简外观下即面板黑），在场面
            // 以梯形 Mesh 呈现绕竖轴翻转。
            let frame = card_frame(transition.direction, p);
            let (texture, size) = match frame.side {
                CardSide::Old => (old, old.size_vec2()),
                CardSide::New => (new, new.size_vec2()),
            };
            let base = fit_rect(stage, size);
            let inset = (1.0 - frame.scale_x) * base.width() / 2.0;
            let tilt = frame.tilt * base.height() * 0.12;
            let corners = [
                egui::pos2(base.left() + inset + tilt, base.top()),
                egui::pos2(base.right() - inset + tilt, base.top()),
                egui::pos2(base.right() - inset - tilt, base.bottom()),
                egui::pos2(base.left() + inset - tilt, base.bottom()),
            ];
            paint_quad(painter, texture.id(), corners, gray_tint(frame.brightness));
        }
        // 无方向组（工单 03）：落笔在 ambient_paint，纯计算在
        // 库 transition_ambient；三种都不读 direction。
        ConcreteAnimation::Blinds => paint_blinds(painter, p, old, new, stage),
        ConcreteAnimation::Ink => paint_ink(painter, p, old, new, stage),
        ConcreteAnimation::Pixel => {
            paint_pixel(painter, p, elapsed_ms, transition.seed, old, new, stage)
        }
    }
}

/// 卷页的卷边亮影：前沿一条高光线 + 揭示侧数条渐淡的影带
/// （借 frame_paint 的分段渐变手法，宽度合计约 14px）。
fn paint_curl_edge(
    painter: &egui::Painter,
    direction: Direction,
    edge_x: f32,
    stage: egui::Rect,
    p: f32,
) {
    if p <= 0.0 || p >= 1.0 {
        return;
    }
    // 高光线：紧贴前沿、朝未揭示侧。
    let line = match direction {
        Direction::Next => egui::Rect::from_min_max(
            egui::pos2(edge_x, stage.top()),
            egui::pos2((edge_x + 2.0).min(stage.right()), stage.bottom()),
        ),
        Direction::Prev => egui::Rect::from_min_max(
            egui::pos2((edge_x - 2.0).max(stage.left()), stage.top()),
            egui::pos2(edge_x, stage.bottom()),
        ),
    };
    painter.rect_filled(line, 0.0, egui::Color32::from_white_alpha(150));
    // 影带：落在已揭示侧，离前沿越远越淡。
    const BANDS: usize = 4;
    for i in 0..BANDS {
        let w = 3.0;
        let alpha = (80.0 * (1.0 - i as f32 / BANDS as f32)) as u8;
        let band = match direction {
            Direction::Next => {
                let right = edge_x - i as f32 * w;
                egui::Rect::from_min_max(
                    egui::pos2((right - w).max(stage.left()), stage.top()),
                    egui::pos2(right, stage.bottom()),
                )
            }
            Direction::Prev => {
                let left = edge_x + i as f32 * w;
                egui::Rect::from_min_max(
                    egui::pos2(left, stage.top()),
                    egui::pos2((left + w).min(stage.right()), stage.bottom()),
                )
            }
        };
        if band.width() > 0.0 {
            painter.rect_filled(band, 0.0, egui::Color32::from_black_alpha(alpha));
        }
    }
}
