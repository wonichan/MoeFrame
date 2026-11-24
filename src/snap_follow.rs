//! 锚边变尺寸的纯逻辑（多实例吸附专题工单 03 落地、工单 06
//! 保留）：相框切图变尺寸时保持贴合边不动。邻居与贴合边的
//! 派生在 [`crate::snap_group`]（工单 06 起按「属性关系 +
//! 直接接触」判定，取代旧的无方向连通分量）。
//!
//! 工单 06 拆除注记：路线 (b) 的轮询跟随引擎（FollowEngine）、
//! 轮询调度（PollSchedule）与菜单脱离的抑制标记已整段拆除
//! （spec 第 15 节 / ADR-0005）——整组联动改由被拖进程在
//! WM_MOVING 中以 HDWP 领队搬运（见 snap_group 与宿主
//! snap_subclass），菜单脱离改清本窗基座属性。本模块只剩
//! 锚边这一条与路线无关的纯计算。

use crate::snap::WindowRect;

/// 本窗的一条贴合边（与某位伙伴共享的那条边）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorEdge {
    Left,
    Right,
    Top,
    Bottom,
}

/// 锚边变尺寸（spec 第 7 节）：新尺寸下保持贴合边位置不动、向空
/// 闲方向长/缩，返回新左上位置。多边贴合的优先级定死：横轴
/// Right > Left，纵轴 Bottom > Top（两轴独立裁决，角贴合同时锚
/// 两轴）；无锚边的轴保持原位置（既有「切图不移动窗口」口径）。
/// 变后是否仍贴合由接触派生按新几何回答，本函数不判。
pub fn anchored_resize_position(
    old: WindowRect,
    new_size: [f32; 2],
    edges: &[AnchorEdge],
) -> [f32; 2] {
    let x = if edges.contains(&AnchorEdge::Right) {
        old.right - new_size[0]
    } else {
        // Left 锚边与无锚边同果：左边不动、向右长/缩。
        old.left
    };
    let y = if edges.contains(&AnchorEdge::Bottom) {
        old.bottom - new_size[1]
    } else {
        old.top
    };
    [x, y]
}
