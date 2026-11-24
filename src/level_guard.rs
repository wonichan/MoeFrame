//! 窗口层级守擂的纯判定（置顶守擂工单 01）：层级命令全仓库只有
//! 菜单点击一处发送点、之后永不重申，任何 z-order 扰动（置顶带内
//! 被后激活窗口排前、置顶标记被摘）都不可恢复。本状态机判定何时
//! 重申：非 Normal 层级在相框窗口失去焦点（带内排序变化的时刻）
//! 后重申一次，带节流下限防命令风暴；重申本身走宿主既有 viewport
//! 命令通路，本模块只出判定、不碰窗口。
//!
//! 成本口径（与 global_sync 一致，定死）：不产生任何唤醒——宿主
//! 只在既有调度已经唤醒的帧上采样一次焦点并轮询本状态机，不新
//! 增定时器、不把静止零重绘顶成常开。

use eframe::egui;

/// 两次重申之间的最小间隔（毫秒）：焦点在窗口间反复横跳时不产
/// 生命令风暴。被节流压住的重申不丢弃——挂起，间隔一到就在后续
/// 帧补发（失焦状态持续期间）。
pub const REASSERT_MIN_INTERVAL_MS: u64 = 1_000;

/// 重申前自检读到的本窗置顶标记（WS_EX_TOPMOST）状态：宿主在
/// Windows 上读窗口扩展样式得到；读不到或非 Windows 观测时为
/// Unknown，只影响日志分类，不影响重申本身。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TopmostMark {
    /// 标记还在：本次重申是置顶带内被后激活窗口排前后的抢回。
    Present,
    /// 标记不在：被外部摘掉，本次重申是修复。
    Missing,
    /// 未观测（非 Windows 或读取失败）。
    Unknown,
}

/// 一次重申的日志分类：与上次记录的分类相同则不再记，避免刷屏。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReassertCause {
    /// 置顶标记被摘，重申修复标记。
    MarkMissing,
    /// 标记还在，置顶带内重排，重申抢回带内次序。
    BandReorder,
}

/// 守擂状态机：宿主持有一份，每帧以（层级，焦点，时刻）轮询。
#[derive(Debug, Default)]
pub struct LevelGuard {
    /// 上一帧的焦点采样；None 表示还没采样过（首帧不构成边沿）。
    was_focused: Option<bool>,
    /// 已观察到失焦边沿、等待节流间隔到期的挂起重申。
    pending: bool,
    /// 上次实际发出重申的时刻；None 表示还没重申过。
    last_reassert_ms: Option<u64>,
    /// 上次记入日志的重申分类（去重用；层级变化时由 poll 重置）。
    last_cause: Option<ReassertCause>,
    /// 上一帧轮询到的层级；与本帧不同即层级发生了变化。
    last_level: Option<egui::WindowLevel>,
}

impl LevelGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// 每帧轮询：返回 true 表示本帧应重申当前层级命令。宿主在
    /// 返回 true 时先自检置顶标记，再调 note_reassert 记账分类。
    pub fn poll(&mut self, level: egui::WindowLevel, focused: bool, now_ms: u64) -> bool {
        let focus_lost = self.was_focused == Some(true) && !focused;
        self.was_focused = Some(focused);
        if self.last_level != Some(level) {
            // 层级切换开启新的守擂纪元：旧层级下记过的分类不再
            // 去重，否则切回置顶后同类真事件会被陈旧记录吞掉。
            self.last_level = Some(level);
            self.last_cause = None;
        }
        if level == egui::WindowLevel::Normal {
            // Normal 无守擂：挂起一并清掉，避免切回非 Normal 时
            // 补发陈旧重申。
            self.pending = false;
            return false;
        }
        if focus_lost {
            self.pending = true;
        }
        if focused {
            // 已回到前台：带内次序自然在本窗一侧，挂起作废。
            self.pending = false;
        }
        if !self.pending {
            return false;
        }
        let due = self
            .last_reassert_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= REASSERT_MIN_INTERVAL_MS);
        if !due {
            return false;
        }
        self.pending = false;
        self.last_reassert_ms = Some(now_ms);
        true
    }

    /// 重申已发出后记账：按自检标记分类，分类与上次记录不同才
    /// 返回 Some（宿主据此记一行日志）。分类只对置顶有意义：
    /// 非置顶层级（置底）本就没有置顶标记，读到「不在」不是被
    /// 摘——一律按 Unknown 处理，不分类、不记、也不动去重状态；
    /// Unknown 同样不记。
    pub fn note_reassert(
        &mut self,
        level: egui::WindowLevel,
        mark: TopmostMark,
    ) -> Option<ReassertCause> {
        if level != egui::WindowLevel::AlwaysOnTop {
            return None;
        }
        let cause = match mark {
            TopmostMark::Missing => ReassertCause::MarkMissing,
            TopmostMark::Present => ReassertCause::BandReorder,
            TopmostMark::Unknown => return None,
        };
        if self.last_cause == Some(cause) {
            None
        } else {
            self.last_cause = Some(cause);
            Some(cause)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_loss_reasserts_when_always_on_top() {
        let mut guard = LevelGuard::new();
        // 先聚焦（建立基线），再失焦：边沿触发重申。
        assert!(!guard.poll(egui::WindowLevel::AlwaysOnTop, true, 0));
        assert!(guard.poll(egui::WindowLevel::AlwaysOnTop, false, 10));
    }

    #[test]
    fn focus_loss_reasserts_when_always_on_bottom() {
        let mut guard = LevelGuard::new();
        assert!(!guard.poll(egui::WindowLevel::AlwaysOnBottom, true, 0));
        assert!(guard.poll(egui::WindowLevel::AlwaysOnBottom, false, 10));
    }

    #[test]
    fn normal_level_never_reasserts() {
        let mut guard = LevelGuard::new();
        assert!(!guard.poll(egui::WindowLevel::Normal, true, 0));
        assert!(!guard.poll(egui::WindowLevel::Normal, false, 10));
        // 间隔再久也不补发：Normal 根本不挂起。
        assert!(!guard.poll(egui::WindowLevel::Normal, false, 60_000));
    }

    #[test]
    fn no_focus_edge_no_reassert() {
        let mut guard = LevelGuard::new();
        // 首帧就是失焦：没有基线，不构成边沿。
        assert!(!guard.poll(egui::WindowLevel::AlwaysOnTop, false, 0));
        // 持续失焦不重复触发（边沿只有一次）。
        assert!(!guard.poll(egui::WindowLevel::AlwaysOnTop, false, 5_000));
        // 获得焦点不触发。
        assert!(!guard.poll(egui::WindowLevel::AlwaysOnTop, true, 6_000));
    }

    #[test]
    fn reassert_is_throttled_and_deferred_not_dropped() {
        let mut guard = LevelGuard::new();
        let top = egui::WindowLevel::AlwaysOnTop;
        assert!(!guard.poll(top, true, 0));
        // 第一次失焦：立即重申，记下时刻 100。
        assert!(guard.poll(top, false, 100));
        // 100ms 后再次失焦：距上次重申不足最小间隔，本帧不发。
        assert!(!guard.poll(top, true, 150));
        assert!(!guard.poll(top, false, 200));
        // 间隔未到、仍失焦：继续挂起，不发。
        assert!(!guard.poll(top, false, 1_099));
        // 间隔一到（100 + 1000）：挂起的重申在后续帧补发一次。
        assert!(guard.poll(top, false, 1_100));
        // 补发后不再重复：持续失焦保持安静。
        assert!(!guard.poll(top, false, 9_999));
    }

    #[test]
    fn regaining_focus_cancels_a_deferred_reassert() {
        let mut guard = LevelGuard::new();
        let top = egui::WindowLevel::AlwaysOnTop;
        assert!(!guard.poll(top, true, 0));
        assert!(guard.poll(top, false, 100));
        assert!(!guard.poll(top, true, 150));
        // 间隔内失焦：挂起。
        assert!(!guard.poll(top, false, 200));
        // 间隔到期前又回到前台：挂起作废，之后即使再过很久也不补发。
        assert!(!guard.poll(top, true, 300));
        assert!(!guard.poll(top, false, 301));
        // 上面 301 的失焦是新边沿：它自己的重申受节流（距 100 不
        // 足间隔）挂起，到期补发——与被取消的旧挂起无关。
        assert!(guard.poll(top, false, 1_100));
    }

    #[test]
    fn switching_to_normal_cancels_a_deferred_reassert() {
        let mut guard = LevelGuard::new();
        let top = egui::WindowLevel::AlwaysOnTop;
        assert!(!guard.poll(top, true, 0));
        assert!(guard.poll(top, false, 100));
        assert!(!guard.poll(top, true, 150));
        assert!(!guard.poll(top, false, 200));
        // 挂起期间切回 Normal：挂起清除，切回顶层也不会补发陈旧重申。
        assert!(!guard.poll(egui::WindowLevel::Normal, false, 300));
        assert!(!guard.poll(top, false, 60_000));
    }

    #[test]
    fn reassert_cause_is_classified_by_the_mark_and_deduplicated() {
        let mut guard = LevelGuard::new();
        let top = egui::WindowLevel::AlwaysOnTop;
        // 标记被摘：记一次；连续同类不重复记。
        assert_eq!(
            guard.note_reassert(top, TopmostMark::Missing),
            Some(ReassertCause::MarkMissing)
        );
        assert_eq!(guard.note_reassert(top, TopmostMark::Missing), None);
        // 标记还在、带内重排：分类切换，记一次。
        assert_eq!(
            guard.note_reassert(top, TopmostMark::Present),
            Some(ReassertCause::BandReorder)
        );
        assert_eq!(guard.note_reassert(top, TopmostMark::Present), None);
        // Unknown 不记、也不动上次分类。
        assert_eq!(guard.note_reassert(top, TopmostMark::Unknown), None);
        assert_eq!(guard.note_reassert(top, TopmostMark::Present), None);
        assert_eq!(
            guard.note_reassert(top, TopmostMark::Missing),
            Some(ReassertCause::MarkMissing)
        );
    }

    #[test]
    fn bottom_level_never_produces_a_cause() {
        let mut guard = LevelGuard::new();
        let bottom = egui::WindowLevel::AlwaysOnBottom;
        // 置底窗本无置顶标记：读到「不在」恒不分类，绝不产出
        // MarkMissing；「在」同样不产出 BandReorder。
        assert_eq!(guard.note_reassert(bottom, TopmostMark::Missing), None);
        assert_eq!(guard.note_reassert(bottom, TopmostMark::Missing), None);
        assert_eq!(guard.note_reassert(bottom, TopmostMark::Present), None);
        // 置底的记账不污染去重状态：切回置顶后真遇摘标记仍可记录。
        assert_eq!(
            guard.note_reassert(egui::WindowLevel::AlwaysOnTop, TopmostMark::Missing),
            Some(ReassertCause::MarkMissing)
        );
    }

    #[test]
    fn level_change_resets_cause_dedup() {
        let mut guard = LevelGuard::new();
        let top = egui::WindowLevel::AlwaysOnTop;
        // 建立层级基线并记一次「标记被摘」；同类第二次被去重吞掉。
        assert!(!guard.poll(top, true, 0));
        assert_eq!(
            guard.note_reassert(top, TopmostMark::Missing),
            Some(ReassertCause::MarkMissing)
        );
        assert_eq!(guard.note_reassert(top, TopmostMark::Missing), None);
        // 切到置底再切回置顶：层级变化重置去重，同类事件重新记录。
        assert!(!guard.poll(egui::WindowLevel::AlwaysOnBottom, true, 10));
        assert!(!guard.poll(top, true, 20));
        assert_eq!(
            guard.note_reassert(top, TopmostMark::Missing),
            Some(ReassertCause::MarkMissing)
        );
    }
}
