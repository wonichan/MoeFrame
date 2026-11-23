//! 多实例吸附专题工单 03：组联动跟随、抑制标记（菜单脱离）、轮询
//! 调度与锚边变尺寸的纯逻辑测试。跟随引擎只掌握「自己的窗口 +
//! 枚举到的伙伴矩形 + 接触派生」，没有中心协调者（ADR-0004）。
use moe_frame::snap::{
    anchored_resize_position, AnchorEdge, FollowEngine, FollowPartner, PartnerFrame, PollOutcome,
    PollPhase, PollSchedule, WindowRect, SNAP_THRESHOLD,
};

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

fn partner(id: u64, x: f32, y: f32, w: f32, h: f32) -> FollowPartner {
    FollowPartner {
        id,
        frame: frame(x, y, w, h),
    }
}

/// 本窗 200x100 在 (100,100)；伙伴 2 同尺寸贴在本窗右侧（x=300）。
fn own() -> PartnerFrame {
    frame(100.0, 100.0, 200.0, 100.0)
}

fn right_partner(x: f32) -> FollowPartner {
    partner(2, x, 100.0, 200.0, 100.0)
}

// ---------- 跟随基本行为 ----------

#[test]
fn first_poll_only_establishes_history() {
    let mut engine = FollowEngine::new();
    let partners = [right_partner(300.0)];
    assert_eq!(engine.poll(own(), &partners, true, false), None);
}

#[test]
fn follows_a_single_moved_contact_partner() {
    let mut engine = FollowEngine::new();
    assert_eq!(
        engine.poll(own(), &[right_partner(300.0)], true, false),
        None
    );
    // 伙伴右移 40：本窗按贴合偏移跟随到 (140,100)。
    let target = engine.poll(own(), &[right_partner(340.0)], true, false);
    assert_eq!(target, Some([140.0, 100.0]));
}

#[test]
fn quiet_partners_produce_no_command() {
    let mut engine = FollowEngine::new();
    let partners = [right_partner(300.0)];
    engine.poll(own(), &partners, true, false);
    assert_eq!(engine.poll(own(), &partners, true, false), None);
    assert_eq!(engine.poll(own(), &partners, true, false), None);
}

#[test]
fn a_partner_outside_the_group_is_not_followed() {
    let mut engine = FollowEngine::new();
    // 伙伴 3 远离本窗、与谁都不接触：动了也不跟。
    let far = partner(3, 900.0, 600.0, 200.0, 100.0);
    engine.poll(own(), &[far], true, false);
    let far_moved = partner(3, 960.0, 600.0, 200.0, 100.0);
    assert_eq!(engine.poll(own(), &[far_moved], true, false), None);
}

#[test]
fn chain_group_middle_member_follows_either_side() {
    // 本窗在中间：左邻 1 贴左、右邻 2 贴右；左邻左移 30 → 本窗跟。
    let mut engine = FollowEngine::new();
    let left = partner(1, -100.0, 100.0, 200.0, 100.0);
    let partners = [left, right_partner(300.0)];
    engine.poll(own(), &partners, true, false);
    let left_moved = partner(1, -130.0, 100.0, 200.0, 100.0);
    let target = engine.poll(own(), &[left_moved, right_partner(300.0)], true, false);
    assert_eq!(target, Some([70.0, 100.0]));
}

#[test]
fn chain_group_end_member_follows_through_the_middle() {
    // 链 A(1)-B(2)-本窗：A 动、B 还没动，本窗与 A 同组（经 B 连通），
    // 按 A 的位移跟随（现实中 B 也会跟 A，本窗等 B 动再跟会慢一拍）。
    let mut engine = FollowEngine::new();
    let a = partner(1, -300.0, 100.0, 200.0, 100.0);
    let b = partner(2, -100.0, 100.0, 200.0, 100.0);
    engine.poll(own(), &[a, b], true, false);
    let a_moved = partner(1, -260.0, 100.0, 200.0, 100.0);
    let target = engine.poll(own(), &[a_moved, b], true, false);
    assert_eq!(target, Some([140.0, 100.0]));
}

#[test]
fn two_neighbours_moving_together_are_followed() {
    let mut engine = FollowEngine::new();
    let left = partner(1, -100.0, 100.0, 200.0, 100.0);
    engine.poll(own(), &[left, right_partner(300.0)], true, false);
    let left_moved = partner(1, -80.0, 120.0, 200.0, 100.0);
    let right_moved = partner(2, 320.0, 120.0, 200.0, 100.0);
    let target = engine.poll(own(), &[left_moved, right_moved], true, false);
    assert_eq!(target, Some([120.0, 120.0]));
}

#[test]
fn two_neighbours_moving_apart_are_not_followed() {
    // 双邻居位移不一致：定死不动，避免本窗被撕扯到任一边。
    let mut engine = FollowEngine::new();
    let left = partner(1, -100.0, 100.0, 200.0, 100.0);
    engine.poll(own(), &[left, right_partner(300.0)], true, false);
    let left_moved = partner(1, -140.0, 100.0, 200.0, 100.0);
    let right_moved = partner(2, 340.0, 100.0, 200.0, 100.0);
    assert_eq!(
        engine.poll(own(), &[left_moved, right_moved], true, false),
        None
    );
}

#[test]
fn a_vanished_partner_produces_no_command_and_no_phantom_history() {
    let mut engine = FollowEngine::new();
    engine.poll(own(), &[right_partner(300.0)], true, false);
    // 伙伴进程退出：不发命令。
    assert_eq!(engine.poll(own(), &[], true, false), None);
    // 同一伙伴在新位置重新出现：无历史可比，不算「移动」，不跟随。
    assert_eq!(
        engine.poll(own(), &[right_partner(500.0)], true, false),
        None
    );
}

// ---------- 防振荡 ----------

#[test]
fn own_external_move_is_never_followed_back() {
    // 本窗被用户拖动（位置变化不是引擎命令造成的）：本轮不跟随，
    // 只更新历史——由伙伴进程各自来跟本窗。
    let mut engine = FollowEngine::new();
    engine.poll(own(), &[right_partner(300.0)], true, false);
    let dragged_own = frame(180.0, 100.0, 200.0, 100.0);
    let dragged_partner = right_partner(380.0);
    assert_eq!(
        engine.poll(dragged_own, &[dragged_partner], true, false),
        None
    );
}

#[test]
fn follow_echo_does_not_bounce_back() {
    // 引擎命令本窗跟随伙伴 +40 → 本窗落位 (140,100)；下一轮伙伴因
    // 它自己的跟随逻辑同移 +40（回声位移与本窗实际位移相同）：该
    // 伙伴的移动不得反向牵引本窗再动一次。
    let mut engine = FollowEngine::new();
    engine.poll(own(), &[right_partner(300.0)], true, false);
    let target = engine.poll(own(), &[right_partner(340.0)], true, false);
    assert_eq!(target, Some([140.0, 100.0]));
    let landed = frame(140.0, 100.0, 200.0, 100.0);
    assert_eq!(
        engine.poll(landed, &[right_partner(380.0)], true, false),
        None
    );
    // 回声消化后伙伴回到贴合位（这一轮按旧图不跟随、只更新
    // 历史），再独立移动时仍可正常跟随。
    assert_eq!(
        engine.poll(landed, &[right_partner(340.0)], true, false),
        None
    );
    let target = engine.poll(landed, &[right_partner(380.0)], true, false);
    assert_eq!(target, Some([180.0, 100.0]));
}

#[test]
fn no_follow_while_own_window_is_being_dragged() {
    let mut engine = FollowEngine::new();
    engine.poll(own(), &[right_partner(300.0)], true, false);
    assert_eq!(
        engine.poll(own(), &[right_partner(340.0)], true, true),
        None
    );
}

#[test]
fn disabled_switch_stops_the_engine_and_clears_history() {
    let mut engine = FollowEngine::new();
    engine.poll(own(), &[right_partner(300.0)], true, false);
    // 开关关闭：不跟随；重新打开后第一轮只建历史，不跳变跟随。
    assert_eq!(
        engine.poll(own(), &[right_partner(300.0)], false, false),
        None
    );
    assert_eq!(
        engine.poll(own(), &[right_partner(300.0)], true, false),
        None
    );
    let target = engine.poll(own(), &[right_partner(340.0)], true, false);
    assert_eq!(target, Some([140.0, 100.0]));
}

// ---------- 跨屏 / 工作区钳制 ----------

#[test]
fn follow_target_is_clamped_into_own_work_area() {
    // 本窗所在屏工作区只到 x=400：伙伴右移 500，本窗跟到屏边停住，
    // 不跨屏发命令（目标仍在本窗工作区内）。
    let small_area = WindowRect {
        left: 0.0,
        top: 0.0,
        right: 400.0,
        bottom: 300.0,
    };
    let own_small = PartnerFrame {
        rect: WindowRect::from_pos_size([100.0, 100.0], [200.0, 100.0]),
        monitor: small_area,
        work_area: small_area,
    };
    let mate = FollowPartner {
        id: 2,
        frame: PartnerFrame {
            rect: WindowRect::from_pos_size([300.0, 100.0], [200.0, 100.0]),
            monitor: small_area,
            work_area: small_area,
        },
    };
    let mate_moved = FollowPartner {
        frame: PartnerFrame {
            rect: WindowRect::from_pos_size([800.0, 100.0], [200.0, 100.0]),
            ..mate.frame
        },
        ..mate
    };
    let mut engine = FollowEngine::new();
    engine.poll(own_small, &[mate], true, false);
    // 注意：伙伴移到 800 后与本窗不在同屏接触——组关系按移动前
    // 的接触判定，本轮仍跟随到工作区右界（200 = 400-200）。
    let target = engine.poll(own_small, &[mate_moved], true, false);
    assert_eq!(target, Some([200.0, 100.0]));
}

#[test]
fn cross_monitor_partner_is_not_in_the_group() {
    let other_monitor = WindowRect {
        left: 1920.0,
        top: 0.0,
        right: 3840.0,
        bottom: 1080.0,
    };
    let far = FollowPartner {
        id: 9,
        frame: PartnerFrame {
            rect: WindowRect::from_pos_size([2220.0, 100.0], [200.0, 100.0]),
            monitor: other_monitor,
            work_area: other_monitor,
        },
    };
    let mut engine = FollowEngine::new();
    engine.poll(own(), &[far], true, false);
    let far_moved = FollowPartner {
        frame: PartnerFrame {
            rect: WindowRect::from_pos_size([2260.0, 100.0], [200.0, 100.0]),
            ..far.frame
        },
        ..far
    };
    assert_eq!(engine.poll(own(), &[far_moved], true, false), None);
}

// ---------- 抑制标记（菜单脱离） ----------

#[test]
fn detach_suppresses_follow_until_real_separation() {
    let mut engine = FollowEngine::new();
    let partners = [right_partner(300.0)];
    engine.poll(own(), &partners, true, false);
    // 菜单脱离：对当时直接接触的伙伴立抑制，位置不变。
    let marked = engine.detach(own(), &partners);
    assert_eq!(marked, vec![2]);
    assert!(engine.is_suppressed(2));
    // 抑制期内伙伴沿贴合边滑动也不跟随（该边在派生中视为不
    // 存在）；滑动不构成「真正分开」，抑制不失效。
    let slid = partner(2, 300.0, 140.0, 200.0, 100.0);
    assert_eq!(engine.poll(own(), &[slid], true, false), None);
    assert!(engine.is_suppressed(2));
    // 伙伴被拖开超过吸附阈值：造成分开的这次移动仍被抑制挡
    // 住，抑制随即自动失效。
    let away_x = 300.0 + 200.0 + SNAP_THRESHOLD + 20.0;
    assert_eq!(
        engine.poll(own(), &[right_partner(away_x)], true, false),
        None
    );
    assert!(!engine.is_suppressed(2));
    // 之后重新贴合可正常成组并跟随。
    assert_eq!(
        engine.poll(own(), &[right_partner(300.0)], true, false),
        None
    );
    let target = engine.poll(own(), &[right_partner(340.0)], true, false);
    assert_eq!(target, Some([140.0, 100.0]));
}

#[test]
fn suppression_survives_small_gaps_within_the_threshold() {
    let mut engine = FollowEngine::new();
    let partners = [right_partner(300.0)];
    engine.poll(own(), &partners, true, false);
    engine.detach(own(), &partners);
    // 只拉开 6px（未超阈值）：抑制仍在，贴回去也不恢复联动。
    assert_eq!(
        engine.poll(own(), &[right_partner(306.0)], true, false),
        None
    );
    assert!(engine.is_suppressed(2));
    assert_eq!(
        engine.poll(own(), &[right_partner(300.0)], true, false),
        None
    );
    assert!(engine.is_suppressed(2));
}

#[test]
fn detach_without_contact_marks_nothing() {
    let mut engine = FollowEngine::new();
    let far = [partner(3, 900.0, 600.0, 200.0, 100.0)];
    assert!(engine.detach(own(), &far).is_empty());
    assert!(!engine.is_suppressed(3));
}

#[test]
fn direct_contacts_exclude_suppressed_partners() {
    let mut engine = FollowEngine::new();
    let partners = [right_partner(300.0)];
    assert_eq!(engine.direct_contact_ids(own(), &partners), vec![2]);
    engine.detach(own(), &partners);
    assert!(engine.direct_contact_ids(own(), &partners).is_empty());
}

// ---------- 锚边 ----------

#[test]
fn anchor_edges_report_the_shared_sides() {
    let engine = FollowEngine::new();
    let partners = [right_partner(300.0)];
    assert_eq!(
        engine.anchor_edges(own(), &partners),
        vec![AnchorEdge::Right]
    );
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
    // 本窗下边与伙伴上边贴合（伙伴在下方），本窗变矮时以 Bottom
    // 为锚会把顶边下移——改用场景：右边贴合且纵向只擦边重叠，
    // 变矮后投影重叠消失，派生中该边自然脱开。
    let own_frame = frame(100.0, 100.0, 200.0, 100.0);
    let mate = partner(2, 300.0, 180.0, 200.0, 100.0);
    let engine = FollowEngine::new();
    assert_eq!(
        engine.anchor_edges(own_frame, &[mate]),
        vec![AnchorEdge::Right]
    );
    // 无纵向锚边：变矮保持顶边，高度 100→10 后与伙伴（y 180..280）
    // 投影不再重叠，接触派生消失。
    let pos = anchored_resize_position(own_frame.rect, [200.0, 10.0], &[AnchorEdge::Right]);
    assert_eq!(pos, [100.0, 100.0]);
    let shrunk = PartnerFrame {
        rect: WindowRect::from_pos_size(pos, [200.0, 10.0]),
        ..own_frame
    };
    assert!(engine.anchor_edges(shrunk, &[mate]).is_empty());
    assert!(engine.direct_contact_ids(shrunk, &[mate]).is_empty());
}

// ---------- 轮询调度 ----------

#[test]
fn schedule_starts_probing_then_sleeps_without_partners() {
    let mut schedule = PollSchedule::new();
    assert_eq!(schedule.phase(), PollPhase::Probing);
    assert!(schedule.poll_due(0));
    assert_eq!(schedule.delay_ms(), Some(0));
    schedule.report(PollOutcome::NoPartners, 0);
    assert_eq!(schedule.phase(), PollPhase::Dormant);
    assert_eq!(schedule.delay_ms(), None);
    assert!(!schedule.poll_due(60_000));
}

#[test]
fn schedule_wake_rearms_a_dormant_schedule() {
    let mut schedule = PollSchedule::new();
    schedule.report(PollOutcome::NoPartners, 0);
    schedule.wake();
    assert_eq!(schedule.phase(), PollPhase::Probing);
    assert!(schedule.poll_due(10));
}

#[test]
fn schedule_heartbeat_is_low_frequency_and_active_is_frame_paced() {
    let mut schedule = PollSchedule::new();
    schedule.report(PollOutcome::Quiet, 0);
    assert_eq!(schedule.phase(), PollPhase::Heartbeat);
    assert_eq!(schedule.delay_ms(), Some(1_000));
    assert!(!schedule.poll_due(500));
    assert!(schedule.poll_due(1_000));
    schedule.report(PollOutcome::Movement, 1_000);
    assert_eq!(schedule.phase(), PollPhase::Active);
    assert_eq!(schedule.delay_ms(), Some(16));
    assert!(schedule.poll_due(1_016));
}

#[test]
fn schedule_active_settles_back_to_heartbeat_after_quiet_polls() {
    let mut schedule = PollSchedule::new();
    schedule.report(PollOutcome::Movement, 0);
    assert_eq!(schedule.phase(), PollPhase::Active);
    let mut now = 0;
    for _ in 0..29 {
        now += 16;
        schedule.report(PollOutcome::Quiet, now);
        assert_eq!(schedule.phase(), PollPhase::Active);
    }
    schedule.report(PollOutcome::Quiet, now + 16);
    assert_eq!(schedule.phase(), PollPhase::Heartbeat);
}
