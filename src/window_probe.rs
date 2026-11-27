//! 窗口实况快照的纯判定（窗口投影工单 03）：暗层归因还剩两
//! 个未决嫌疑——(甲) 样式被 winit 的 setter 整词写回、(乙) 窗
//! 口矩形与相框绘制矩形几何错位。本模块只裁决「何时记快照」
//! 与「快照一行怎么排版」，不碰窗口：焦点丢失每进程只记一
//! 次、外观模式切换每进程至多记三次（防刷屏）；样式/窗口矩
//! 形/绘制矩形的实读与落日志都在宿主。
//!
//! 绘制矩形的坐标换算口径：绘制矩形是应用侧在视口内画相框
//! 的矩形（逻辑点，原点在客户区左上）；相框窗口无边框、客
//! 户区与窗口矩形重合，客户区原点的屏幕物理坐标即
//! GetWindowRect 的左上，故物理绘制矩形 = 窗口左上 + 逻辑
//! 矩形 × pixels_per_point（逐边换算后四舍五入）。

use crate::frame_layout::EffectiveAppearance;

/// 外观切换快照的每进程上限（工单 03 定案）：焦点丢失快照
/// 每进程只记一次，不占此额。
pub const MAX_APPEARANCE_SNAPSHOTS: u8 = 3;

/// 外观模式键：只分三模式——内置画框的参数变化（预设/宽
/// 度）不算模式切换，不触发快照。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AppearanceMode {
    Minimal,
    Builtin,
    CustomArt,
}

fn appearance_mode(effective: EffectiveAppearance) -> AppearanceMode {
    match effective {
        EffectiveAppearance::Minimal => AppearanceMode::Minimal,
        EffectiveAppearance::Builtin(_) => AppearanceMode::Builtin,
        EffectiveAppearance::CustomArt => AppearanceMode::CustomArt,
    }
}

/// 快照点位状态机：宿主持有一份，焦点每帧采样、外观每帧报
/// 到，本状态机只回答「本帧要不要记一行」。
#[derive(Debug, Default)]
pub struct WindowProbe {
    /// 上一帧的焦点采样；None 表示还没采样过（首个样本只建
    /// 基线、不构成边沿，与 level_guard 同口径）。
    was_focused: Option<bool>,
    /// 焦点丢失快照是否已记过（每进程一次）。
    focus_taken: bool,
    /// 上一帧的外观模式键；None 表示还没报到过（首报只建基线）。
    last_mode: Option<AppearanceMode>,
    /// 已记的外观切换快照数（上限 MAX_APPEARANCE_SNAPSHOTS）。
    appearance_taken: u8,
}

impl WindowProbe {
    pub fn new() -> Self {
        Self::default()
    }

    /// 每帧采样焦点：返回 true 表示本帧应记焦点丢失快照——
    /// 本进程首次失焦边沿（聚焦→失焦）且此前没记过。
    pub fn poll_focus(&mut self, focused: bool) -> bool {
        let focus_lost = self.was_focused == Some(true) && !focused;
        self.was_focused = Some(focused);
        if focus_lost && !self.focus_taken {
            self.focus_taken = true;
            return true;
        }
        false
    }

    /// 生效外观每帧报到：返回 true 表示本帧应记外观切换快
    /// 照——模式键与上次不同（首报只建基线），且未达上限。
    /// 达上限后仍更新基线，后续切换不再记。
    pub fn note_appearance(&mut self, effective: EffectiveAppearance) -> bool {
        let mode = appearance_mode(effective);
        let switched = self.last_mode.is_some_and(|last| last != mode);
        self.last_mode = Some(mode);
        if switched && self.appearance_taken < MAX_APPEARANCE_SNAPSHOTS {
            self.appearance_taken += 1;
            return true;
        }
        false
    }
}

/// 绘制矩形（视口逻辑坐标 [left, top, right, bottom]）换算
/// 到屏幕物理坐标：口径见模块头。
pub fn draw_rect_physical(
    win_left_top: [i32; 2],
    draw_rect: [f32; 4],
    pixels_per_point: f32,
) -> [i32; 4] {
    let conv = |v: f32| (v * pixels_per_point).round() as i32;
    [
        win_left_top[0] + conv(draw_rect[0]),
        win_left_top[1] + conv(draw_rect[1]),
        win_left_top[0] + conv(draw_rect[2]),
        win_left_top[1] + conv(draw_rect[3]),
    ]
}

/// 快照的一行日志明细（宿主经 window_probe_record 落
/// moe_frame.log）：当前样式/扩展样式（八位十六进制）、窗
/// 口矩形与绘制矩形（物理像素，(left,top,right,bottom)）。
pub fn probe_log_details(
    style: isize,
    ex_style: isize,
    win_rect: [i32; 4],
    draw_rect: [i32; 4],
) -> String {
    format!(
        "style=0x{:08X} exstyle=0x{:08X} win_rect=({},{},{},{}) draw_rect=({},{},{},{})",
        style as u32,
        ex_style as u32,
        win_rect[0],
        win_rect[1],
        win_rect[2],
        win_rect[3],
        draw_rect[0],
        draw_rect[1],
        draw_rect[2],
        draw_rect[3],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BuiltinFrame;
    use crate::frame_layout::EffectiveAppearance;

    #[test]
    fn focus_first_loss_fires_once() {
        let mut probe = WindowProbe::new();
        assert!(!probe.poll_focus(true));
        assert!(probe.poll_focus(false));
        // 再失焦不重复记。
        assert!(!probe.poll_focus(true));
        assert!(!probe.poll_focus(false));
    }

    #[test]
    fn focus_baseline_from_unfocused_start() {
        let mut probe = WindowProbe::new();
        // 首个样本就是失焦：只建基线，不构成边沿。
        assert!(!probe.poll_focus(false));
        assert!(!probe.poll_focus(true));
        assert!(probe.poll_focus(false));
    }

    #[test]
    fn focus_never_lost_never_fires() {
        let mut probe = WindowProbe::new();
        assert!(!probe.poll_focus(true));
        assert!(!probe.poll_focus(true));
    }

    #[test]
    fn appearance_first_report_is_baseline() {
        let mut probe = WindowProbe::new();
        assert!(!probe.note_appearance(EffectiveAppearance::Minimal));
        assert!(!probe.note_appearance(EffectiveAppearance::Minimal));
    }

    #[test]
    fn appearance_mode_switch_fires_and_caps_at_three() {
        let mut probe = WindowProbe::new();
        let builtin = EffectiveAppearance::Builtin(BuiltinFrame::default());
        assert!(!probe.note_appearance(EffectiveAppearance::Minimal));
        assert!(probe.note_appearance(builtin));
        assert!(probe.note_appearance(EffectiveAppearance::CustomArt));
        assert!(probe.note_appearance(EffectiveAppearance::Minimal));
        // 第 4 次切换：已达上限，不再记。
        assert!(!probe.note_appearance(builtin));
        assert!(!probe.note_appearance(EffectiveAppearance::CustomArt));
    }

    #[test]
    fn appearance_builtin_param_change_is_not_a_mode_switch() {
        let mut probe = WindowProbe::new();
        let wide = BuiltinFrame {
            frame_width: BuiltinFrame::default().frame_width + 5.0,
            ..BuiltinFrame::default()
        };
        assert!(!probe.note_appearance(EffectiveAppearance::Builtin(BuiltinFrame::default())));
        assert!(!probe.note_appearance(EffectiveAppearance::Builtin(wide)));
        // 真切换仍然触发。
        assert!(probe.note_appearance(EffectiveAppearance::CustomArt));
    }

    #[test]
    fn draw_rect_physical_scales_and_offsets() {
        // 整窗绘制（逻辑原点在 0,0）：只有缩放。
        assert_eq!(
            draw_rect_physical([100, 50], [0.0, 0.0, 200.0, 100.0], 1.5),
            [100, 50, 400, 200]
        );
        // 带逻辑偏移：逐边换算后加窗口左上。
        assert_eq!(
            draw_rect_physical([100, 50], [10.0, 20.0, 110.0, 120.0], 2.0),
            [120, 90, 320, 290]
        );
        // ppp = 1：只剩平移。
        assert_eq!(
            draw_rect_physical([-5, 7], [3.0, 4.0, 103.0, 204.0], 1.0),
            [-2, 11, 98, 211]
        );
    }

    #[test]
    fn probe_log_details_format() {
        assert_eq!(
            probe_log_details(
                0x80C4_0000u32 as i32 as isize,
                0x0000_0100,
                [66, 15, 1103, 810],
                [66, 15, 1103, 810],
            ),
            "style=0x80C40000 exstyle=0x00000100 win_rect=(66,15,1103,810) \
             draw_rect=(66,15,1103,810)"
        );
    }
}
