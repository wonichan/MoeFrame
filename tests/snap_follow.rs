//! 多实例吸附专题工单 03 落地、工单 06 保留的锚边变尺寸纯逻辑
//! 测试。工单 06 拆除注记：跟随引擎、抑制标记与轮询调度的用例
//! 已随实现整段删除（逐条去向见工单 06 Comments）；锚边派生
//! 改归 snap_group（tests/snap_group.rs），本文件只剩
//! anchored_resize_position 的位置计算用例。
use moe_frame::snap::{PartnerFrame, WindowRect};
use moe_frame::snap_follow::{anchored_resize_position, AnchorEdge};
use moe_frame::snap_group::{anchor_edges, direct_contact_ids, SnapPartner};

const MONITOR: WindowRect = WindowRect {
    left: 0.0,
    top: 0.0,
    right: 1920.0,
    bottom: 1080.0,
};

fn frame(x: f32, y: f32, w: f32, h: f32) -> PartnerFrame {
    PartnerFrame {
        rect: WindowRect::from_pos_size([x, y], [w, h]),
        monitor: MONITOR,
        work_area: MONITOR,
    }
}

#[test]
fn right_anchor_grows_to_the_left() {
    let old = WindowRect::from_pos_size([100.0, 100.0], [200.0, 100.0]);
    // 右边贴合：右边不动，变宽向左长。
    assert_eq!(
        anchored_resize_position(old, [300.0, 100.0], &[AnchorEdge::Right]),
        [0.0, 100.0]
    );
}

#[test]
fn left_anchor_grows_to_the_right() {
    let old = WindowRect::from_pos_size([100.0, 100.0], [200.0, 100.0]);
    assert_eq!(
        anchored_resize_position(old, [300.0, 100.0], &[AnchorEdge::Left]),
        [100.0, 100.0]
    );
}

#[test]
fn bottom_anchor_wins_over_top_and_right_over_left() {
    let old = WindowRect::from_pos_size([100.0, 100.0], [200.0, 100.0]);
    // 多边贴合优先级定死：横轴 Right > Left，纵轴 Bottom > Top。
    assert_eq!(
        anchored_resize_position(
            old,
            [300.0, 160.0],
            &[
                AnchorEdge::Left,
                AnchorEdge::Right,
                AnchorEdge::Top,
                AnchorEdge::Bottom
            ]
        ),
        [0.0, 40.0]
    );
}

#[test]
fn shrinking_can_detach_the_contact_naturally() {
    // 右边贴合且纵向只擦边重叠，变矮后投影重叠消失，接触自然脱开。
    // 工单 06 改写：锚边/接触派生改走 snap_group（关系+接触口径）。
    let own_frame = frame(100.0, 100.0, 200.0, 100.0);
    let mate = SnapPartner {
        id: 2,
        frame: frame(300.0, 180.0, 200.0, 100.0),
        base: None,
    };
    assert_eq!(
        anchor_edges(own_frame, &[mate], &[2]),
        vec![AnchorEdge::Right]
    );
    let pos = anchored_resize_position(own_frame.rect, [200.0, 10.0], &[AnchorEdge::Right]);
    assert_eq!(pos, [100.0, 100.0]);
    let shrunk = PartnerFrame {
        rect: WindowRect::from_pos_size(pos, [200.0, 10.0]),
        ..own_frame
    };
    assert!(anchor_edges(shrunk, &[mate], &[2]).is_empty());
    assert!(direct_contact_ids(shrunk, &[mate]).is_empty());
}
