//! 自研拖动模态环的矩形纯判定（窗口投影工单 02）：剥掉边框
//! 样式后，DefWindowProc 的移动/缩放模态环随之失效，宿主在
//! 吸附子类化里拦截 WM_NCLBUTTONDOWN 的 HTCAPTION 与八个
//! HT 缩放码自行跑环。本模块把「命中码 → 拖动种类」与
//! 「起始矩形 + 起始光标 + 当前光标 → 拟议矩形」做成纯函数：
//! 移动是整窗跟随，缩放是被拖边跟随、对边锚定，最小尺寸按仓
//! 库既有常量在环里钳制（DefWindowProc 曾以最小跟踪尺寸代
//! 劳）。环本体（SetCapture、泵消息、SetWindowPos）在宿主
//! snap_subclass；拟议矩形与既有口径一致——只是提议，应用层
//! 的高宽派生/回正在后续帧照常收敛。
//!
//! 坐标口径：物理像素（与 WM_MOVING 吸附求解同源），矩形容
//! 器复用 [`crate::snap::WindowRect`]。

use crate::snap::WindowRect;

/// WM_NCLBUTTONDOWN 的命中码分类：标题栏拖动（整窗移动）与
/// 八向边缘缩放。码值是 Win32 文档的 HT* 常量，宿主从
/// wparam 解出后喂进来。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragHit {
    /// HTCAPTION：整窗跟随光标。
    Move,
    /// HTLEFT：左边跟随、右边锚定。
    Left,
    /// HTRIGHT：右边跟随、左边锚定。
    Right,
    /// HTTOP：上边跟随、下边锚定。
    Top,
    /// HTBOTTOM：下边跟随、上边锚定。
    Bottom,
    /// HTTOPLEFT：左、上两边跟随。
    TopLeft,
    /// HTTOPRIGHT：右、上两边跟随。
    TopRight,
    /// HTBOTTOMLEFT：左、下两边跟随。
    BottomLeft,
    /// HTBOTTOMRIGHT：右、下两边跟随。
    BottomRight,
}

impl DragHit {
    /// 命中码 → 拖动种类：只认 HTCAPTION（2）与八个缩放码
    /// （10–17），其余（HTCLIENT 等）返回 None、宿主不拦截。
    pub fn from_hit_code(code: i32) -> Option<Self> {
        // 码值与 Win32 文档的 HT* 常量对齐（宿主以 wparam 原值
        // 喂入，windows-sys 的同名常量同值）。
        Some(match code {
            2 => Self::Move,
            10 => Self::Left,
            11 => Self::Right,
            12 => Self::Top,
            13 => Self::TopLeft,
            14 => Self::TopRight,
            15 => Self::Bottom,
            16 => Self::BottomLeft,
            17 => Self::BottomRight,
            _ => return None,
        })
    }

    /// 本种类是否拖动左边。
    fn follows_left(self) -> bool {
        matches!(self, Self::Left | Self::TopLeft | Self::BottomLeft)
    }

    /// 本种类是否拖动右边。
    fn follows_right(self) -> bool {
        matches!(self, Self::Right | Self::TopRight | Self::BottomRight)
    }

    /// 本种类是否拖动上边。
    fn follows_top(self) -> bool {
        matches!(self, Self::Top | Self::TopLeft | Self::TopRight)
    }

    /// 本种类是否拖动下边。
    fn follows_bottom(self) -> bool {
        matches!(self, Self::Bottom | Self::BottomLeft | Self::BottomRight)
    }
}

/// 由起始矩形与光标位移算拟议矩形：移动整窗平移（尺寸不变、
/// 不钳制）；缩放按被拖边跟随、对边锚定算，再按 `min_size`
/// （物理像素的 [宽, 高] 下限）钳制——钳制只推被拖边，锚定边
/// 不动。
pub fn drag_rect(
    hit: DragHit,
    start: WindowRect,
    start_cursor: [f32; 2],
    cursor: [f32; 2],
    min_size: [f32; 2],
) -> WindowRect {
    let dx = cursor[0] - start_cursor[0];
    let dy = cursor[1] - start_cursor[1];
    if hit == DragHit::Move {
        return WindowRect {
            left: start.left + dx,
            top: start.top + dy,
            right: start.right + dx,
            bottom: start.bottom + dy,
        };
    }
    let mut rect = start;
    if hit.follows_left() {
        rect.left = start.left + dx;
        if rect.width() < min_size[0] {
            rect.left = rect.right - min_size[0];
        }
    }
    if hit.follows_right() {
        rect.right = start.right + dx;
        if rect.width() < min_size[0] {
            rect.right = rect.left + min_size[0];
        }
    }
    if hit.follows_top() {
        rect.top = start.top + dy;
        if rect.height() < min_size[1] {
            rect.top = rect.bottom - min_size[1];
        }
    }
    if hit.follows_bottom() {
        rect.bottom = start.bottom + dy;
        if rect.height() < min_size[1] {
            rect.bottom = rect.top + min_size[1];
        }
    }
    rect
}

/// Esc 取消的按下沿判定（工单 02 评审补记）：模态环逐轮把
/// GetAsyncKeyState 的当前键态喂进来，只认「上一轮松开、这
/// 一轮按住」的新按下。进入环时已按住的 Esc 以入环键态为基
/// 线（宿主把入环采样作首轮的 `was_down`），持续按住不触发
/// ——拖动开始前的残留按键不该取消本次拖动；松开再按才算
/// 一次新的取消意图。与 DefWindowProc 原生环逐轮轮询键态
/// 的语义对齐，且不依赖本窗是否持有焦点。
pub fn esc_press_edge(was_down: bool, is_down: bool) -> bool {
    is_down && !was_down
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: WindowRect = WindowRect {
        left: 100.0,
        top: 100.0,
        right: 300.0,
        bottom: 260.0,
    };
    const MIN: [f32; 2] = [120.0, 120.0];

    #[test]
    fn hit_codes_map_to_drag_kinds() {
        assert_eq!(DragHit::from_hit_code(2), Some(DragHit::Move));
        assert_eq!(DragHit::from_hit_code(10), Some(DragHit::Left));
        assert_eq!(DragHit::from_hit_code(11), Some(DragHit::Right));
        assert_eq!(DragHit::from_hit_code(12), Some(DragHit::Top));
        assert_eq!(DragHit::from_hit_code(13), Some(DragHit::TopLeft));
        assert_eq!(DragHit::from_hit_code(14), Some(DragHit::TopRight));
        assert_eq!(DragHit::from_hit_code(15), Some(DragHit::Bottom));
        assert_eq!(DragHit::from_hit_code(16), Some(DragHit::BottomLeft));
        assert_eq!(DragHit::from_hit_code(17), Some(DragHit::BottomRight));
    }

    #[test]
    fn other_hit_codes_are_not_intercepted() {
        // HTNOWHERE=0、HTCLIENT=1、HTSYSMENU=3、HTMENU=5、
        // HTHSCROLL=6 都不是拖动/缩放入口。
        for code in [0, 1, 3, 5, 6, 18, -1] {
            assert_eq!(DragHit::from_hit_code(code), None, "code {code}");
        }
    }

    #[test]
    fn move_follows_cursor_delta_and_keeps_size() {
        let rect = drag_rect(DragHit::Move, START, [150.0, 120.0], [200.0, 180.0], MIN);
        assert_eq!(
            rect,
            WindowRect {
                left: 150.0,
                top: 160.0,
                right: 350.0,
                bottom: 320.0
            }
        );
    }

    #[test]
    fn right_edge_follows_and_left_anchors() {
        let rect = drag_rect(DragHit::Right, START, [300.0, 150.0], [340.0, 170.0], MIN);
        assert_eq!(
            rect,
            WindowRect {
                left: 100.0,
                top: 100.0,
                right: 340.0,
                bottom: 260.0
            }
        );
    }

    #[test]
    fn left_edge_follows_and_right_anchors() {
        let rect = drag_rect(DragHit::Left, START, [100.0, 150.0], [70.0, 130.0], MIN);
        assert_eq!(
            rect,
            WindowRect {
                left: 70.0,
                top: 100.0,
                right: 300.0,
                bottom: 260.0
            }
        );
    }

    #[test]
    fn top_left_corner_follows_both_axes() {
        let rect = drag_rect(DragHit::TopLeft, START, [100.0, 100.0], [70.0, 120.0], MIN);
        assert_eq!(
            rect,
            WindowRect {
                left: 70.0,
                top: 120.0,
                right: 300.0,
                bottom: 260.0
            }
        );
    }

    #[test]
    fn bottom_right_corner_follows_both_axes() {
        let rect = drag_rect(
            DragHit::BottomRight,
            START,
            [300.0, 260.0],
            [360.0, 300.0],
            MIN,
        );
        assert_eq!(
            rect,
            WindowRect {
                left: 100.0,
                top: 100.0,
                right: 360.0,
                bottom: 300.0
            }
        );
    }

    #[test]
    fn right_edge_clamps_at_min_width() {
        // 右边左拖 150：未钳制时宽 50 < 120，右边停在左锚 + 120。
        let rect = drag_rect(DragHit::Right, START, [300.0, 150.0], [150.0, 150.0], MIN);
        assert_eq!(rect.left, 100.0);
        assert_eq!(rect.right, 220.0);
    }

    #[test]
    fn left_edge_clamps_at_min_width() {
        // 左边右拖 150：未钳制时宽 50 < 120，左边停在右锚 − 120。
        let rect = drag_rect(DragHit::Left, START, [100.0, 150.0], [250.0, 150.0], MIN);
        assert_eq!(rect.left, 180.0);
        assert_eq!(rect.right, 300.0);
    }

    #[test]
    fn esc_edge_only_on_new_press() {
        // 真值表：只有松开 → 按下这一跳算取消触发。
        assert!(!esc_press_edge(false, false));
        assert!(esc_press_edge(false, true));
        assert!(!esc_press_edge(true, true));
        assert!(!esc_press_edge(true, false));
    }

    #[test]
    fn esc_held_at_entry_does_not_trigger_until_repressed() {
        // 入环时已按住：以入环键态为基线，持续按住不触发；
        // 松开后再按，才是一次新的取消。
        let mut was_down = true;
        let script = [
            (true, false),
            (true, false),
            (false, false),
            (false, false),
            (true, true),
        ];
        for (is_down, expected) in script {
            assert_eq!(
                esc_press_edge(was_down, is_down),
                expected,
                "is_down={is_down}"
            );
            was_down = is_down;
        }
    }

    #[test]
    fn top_edge_clamps_at_min_height() {
        // 上边下拖 100：未钳制时高 60 < 120，上边停在下锚 − 120。
        let rect = drag_rect(DragHit::Top, START, [150.0, 100.0], [150.0, 200.0], MIN);
        assert_eq!(rect.top, 140.0);
        assert_eq!(rect.bottom, 260.0);
    }
}
