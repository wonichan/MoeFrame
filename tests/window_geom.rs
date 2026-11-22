//! 工单 20 落地、深挖工单 05 收拢：窗口几何生命周期状态机测试——
//! 全部改打状态机 interface（观察进、命令出），纯算术不再是公开面。
use moe_frame::{
    config_sync::GeometryRecord,
    window_geom::{GeomCommand, GeomObservation, GeometryMachine, Insets},
};

const MONITOR: [f32; 2] = [1920.0, 1080.0];
const SAVED: GeometryRecord = (Some(100), Some(100), 480, 320);

fn machine(saved: GeometryRecord, aspect: Option<f32>, has_position: bool) -> GeometryMachine {
    GeometryMachine::new(saved, aspect, has_position)
}

/// 一帧观察：位置与尺寸自洽（外尺寸 = 内尺寸，本相框无边框装饰）。
fn obs(pos: [f32; 2], inner: [f32; 2], now: f64) -> GeomObservation {
    obs_with_insets(pos, inner, now, Insets::ZERO)
}

/// 带四边内距的一帧观察（V2 相框外观专题工单 01）：inner 是外框
/// 尺寸（窗口内尺寸），画面区域 = 外框 − 四边内距。
fn obs_with_insets(pos: [f32; 2], inner: [f32; 2], now: f64, insets: Insets) -> GeomObservation {
    GeomObservation {
        outer_pos: Some(pos),
        outer_size: Some(inner),
        inner: Some(inner),
        monitor: Some(MONITOR),
        pointer_down: false,
        frame_visible: true,
        now_secs: now,
        insets,
    }
}

fn sizes(commands: &[GeomCommand]) -> Vec<[f32; 2]> {
    commands
        .iter()
        .filter_map(|c| match c {
            GeomCommand::SetInnerSize(size) => Some(*size),
            _ => None,
        })
        .collect()
}

fn assert_size(actual: [f32; 2], expected: [f32; 2]) {
    assert!(
        (actual[0] - expected[0]).abs() < 0.51 && (actual[1] - expected[1]).abs() < 0.51,
        "期望 {expected:?}，实际 {actual:?}"
    );
}

// ---------- 首次放置 ----------

#[test]
fn the_first_frame_places_a_positionless_frame_bottom_right() {
    let mut m = machine((None, None, 480, 320), None, false);
    let commands = m.observe(&obs([0.0, 0.0], [480.0, 320.0], 0.0));
    assert!(commands.contains(&GeomCommand::SetOuterPosition([1416.0, 736.0])));
    // 只放一次。
    let commands = m.observe(&obs([1416.0, 736.0], [480.0, 320.0], 0.1));
    assert!(!commands
        .iter()
        .any(|c| matches!(c, GeomCommand::SetOuterPosition(_))));
}

#[test]
fn a_saved_position_never_triggers_first_placement() {
    let mut m = machine(SAVED, None, true);
    let commands = m.observe(&obs([100.0, 100.0], [480.0, 320.0], 0.0));
    assert!(!commands
        .iter()
        .any(|c| matches!(c, GeomCommand::SetOuterPosition(_))));
}

// ---------- 启动离屏钳制 ----------

#[test]
fn a_fully_offscreen_start_position_is_clamped_once() {
    // 副屏拔掉后 x=5000 落在主屏之外：回主屏右下内缩 24，只钳一次。
    let saved: GeometryRecord = (Some(5000), Some(100), 480, 320);
    let mut m = machine(saved, None, true);
    let commands = m.observe(&obs([5000.0, 100.0], [480.0, 320.0], 0.0));
    assert_eq!(
        commands,
        vec![GeomCommand::SetOuterPosition([1416.0, 736.0])]
    );
    // 钳制后即使观察仍报旧位置也不再重钳（只查一次）。
    let commands = m.observe(&obs([5000.0, 100.0], [480.0, 320.0], 0.1));
    assert!(!commands
        .iter()
        .any(|c| matches!(c, GeomCommand::SetOuterPosition(_))));
    // 完全在主屏上方同样算离屏。
    let mut m = machine((Some(100), Some(-900), 480, 320), None, true);
    let commands = m.observe(&obs([100.0, -900.0], [480.0, 320.0], 0.0));
    assert_eq!(
        commands,
        vec![GeomCommand::SetOuterPosition([1416.0, 736.0])]
    );
}

#[test]
fn a_partially_visible_start_position_is_left_alone() {
    let mut m = machine((Some(-100), Some(50), 480, 320), None, true);
    assert!(m
        .observe(&obs([-100.0, 50.0], [480.0, 320.0], 0.0))
        .is_empty());
    let mut m = machine(SAVED, None, true);
    assert!(m
        .observe(&obs([100.0, 100.0], [480.0, 320.0], 0.0))
        .is_empty());
}
// ---------- 当前图像比例修正（经观察驱动） ----------
/// 锁定机：先喂一帧基准尺寸，返回机与已消费基准的观察。
fn locked_machine(aspect: f32, base: [f32; 2]) -> GeometryMachine {
    let saved: GeometryRecord = (Some(100), Some(100), base[0] as u32, base[1] as u32);
    let mut m = machine(saved, Some(aspect), true);
    m.observe(&obs([100.0, 100.0], base, 0.0));
    m
}

#[test]
fn a_width_drag_keeps_height_as_the_scale_reference() {
    let mut m = locked_machine(1.5, [480.0, 320.0]);
    let commands = m.observe(&obs([100.0, 100.0], [600.0, 320.0], 0.1));
    assert_size(sizes(&commands)[0], [480.0, 320.0]);
}

#[test]
fn a_height_driven_resize_derives_the_width() {
    let mut m = locked_machine(1.5, [480.0, 320.0]);
    let commands = m.observe(&obs([100.0, 100.0], [480.0, 400.0], 0.1));
    assert_size(sizes(&commands)[0], [600.0, 400.0]);
}

#[test]
fn a_corner_resize_keeps_height_as_the_scale_reference() {
    let mut m = locked_machine(1.5, [480.0, 320.0]);
    let commands = m.observe(&obs([100.0, 100.0], [600.0, 500.0], 0.1));
    assert_size(sizes(&commands)[0], [750.0, 500.0]);
}

#[test]
fn minimum_window_sides_preserve_the_image_ratio() {
    let mut m = locked_machine(1.5, [480.0, 320.0]);
    let commands = m.observe(&obs([100.0, 100.0], [600.0, 80.0], 0.1));
    assert_size(sizes(&commands)[0], [180.0, 120.0]);

    let mut m = locked_machine(0.5, [160.0, 320.0]);
    let commands = m.observe(&obs([100.0, 100.0], [100.0, 80.0], 0.1));
    assert_size(sizes(&commands)[0], [120.0, 240.0]);
}

#[test]
fn an_invalid_aspect_never_corrects() {
    let mut m = locked_machine(0.0, [480.0, 320.0]);
    let commands = m.observe(&obs([100.0, 100.0], [600.0, 500.0], 0.1));
    assert!(sizes(&commands).is_empty());
}

#[test]
fn initial_image_ratio_corrects_the_window_using_height_as_reference() {
    let mut m = machine(SAVED, Some(2.0), true);
    let commands = m.observe(&obs([100.0, 100.0], [480.0, 320.0], 0.0));
    assert_size(sizes(&commands)[0], [640.0, 320.0]);
}

#[test]
fn resize_corrects_the_aspect_when_pointer_is_released() {
    let mut m = locked_machine(1.5, [480.0, 320.0]);
    let mut dragging = obs([100.0, 100.0], [600.0, 320.0], 0.1);
    dragging.pointer_down = true;
    assert!(sizes(&m.observe(&dragging)).is_empty());
    let commands = m.observe(&obs([100.0, 100.0], [600.0, 320.0], 0.2));
    assert_size(sizes(&commands)[0], [480.0, 320.0]);
}

// ---------- 落盘防抖 ----------

#[test]
fn a_settled_geometry_produces_one_save_command() {
    let mut m = machine(SAVED, None, true);
    // 移到新位置：首帧只记候选，停稳 0.6 秒后产出一次保存。
    assert!(m
        .observe(&obs([200.0, 150.0], [480.0, 320.0], 0.0))
        .is_empty());
    assert!(m
        .observe(&obs([200.0, 150.0], [480.0, 320.0], 0.3))
        .is_empty());
    let commands = m.observe(&obs([200.0, 150.0], [480.0, 320.0], 0.61));
    assert_eq!(
        commands,
        vec![GeomCommand::SaveGeometry((Some(200), Some(150), 480, 320))]
    );
    // 基准已推进：同值不再产出，退出结算交回的正是这份记录。
    assert!(m
        .observe(&obs([200.0, 150.0], [480.0, 320.0], 1.3))
        .is_empty());
    assert_eq!(
        m.exit_settle(),
        (
            Some((Some(200), Some(150), 480, 320)),
            (Some(200), Some(150), 480, 320)
        )
    );
}

#[test]
fn a_moving_geometry_restarts_the_settle_timer() {
    let mut m = machine(SAVED, None, true);
    m.observe(&obs([200.0, 150.0], [480.0, 320.0], 0.0));
    m.observe(&obs([260.0, 150.0], [480.0, 320.0], 0.5));
    assert!(m
        .observe(&obs([260.0, 150.0], [480.0, 320.0], 1.0))
        .is_empty());
    let commands = m.observe(&obs([260.0, 150.0], [480.0, 320.0], 1.11));
    assert_eq!(commands.len(), 1);
}

#[test]
fn a_hidden_frame_observes_no_geometry_to_save() {
    let mut m = machine(SAVED, None, true);
    let mut hidden = obs([200.0, 150.0], [480.0, 320.0], 0.0);
    hidden.frame_visible = false;
    m.observe(&hidden);
    let mut hidden = obs([200.0, 150.0], [480.0, 320.0], 1.0);
    hidden.frame_visible = false;
    assert!(m.observe(&hidden).is_empty());
    // 退出结算仍交回存档基准，没有隐藏期的观察值可冲刷。
    assert_eq!(m.exit_settle(), (None, SAVED));
}

// ---------- 当前图变化 ----------

#[test]
fn changing_images_updates_the_window_aspect() {
    let mut m = machine(SAVED, None, true);
    m.observe(&obs([100.0, 100.0], [480.0, 320.0], 0.0));
    m.set_image_aspect(Some(2.0));
    let commands = m.observe(&obs([100.0, 100.0], [480.0, 320.0], 0.1));
    assert_size(sizes(&commands)[0], [640.0, 320.0]);

    m.set_image_aspect(Some(0.5));
    let commands = m.observe(&obs([100.0, 100.0], [640.0, 320.0], 0.2));
    assert_size(sizes(&commands)[0], [160.0, 320.0]);
}

// ---------- 恢复默认 ----------

#[test]
fn restore_default_sizes_places_and_saves() {
    let mut m = machine((Some(300), Some(200), 800, 600), None, true);
    let commands = m.restore_default(Some(MONITOR), true, Insets::ZERO);
    assert_eq!(
        commands,
        vec![
            GeomCommand::SetInnerSize([480.0, 320.0]),
            GeomCommand::SetOuterPosition([1416.0, 736.0]),
            GeomCommand::SaveGeometry((Some(1416), Some(736), 480, 320)),
        ]
    );
    assert_eq!(m.exit_settle().1, (Some(1416), Some(736), 480, 320));
}

#[test]
fn restore_default_uses_the_current_image_ratio_and_height() {
    let mut m = machine((Some(300), Some(200), 800, 600), Some(2.0), true);
    let commands = m.restore_default(Some(MONITOR), true, Insets::ZERO);
    assert_eq!(
        commands,
        vec![
            GeomCommand::SetInnerSize([640.0, 320.0]),
            GeomCommand::SetOuterPosition([1256.0, 736.0]),
            GeomCommand::SaveGeometry((Some(1256), Some(736), 640, 320)),
        ]
    );
}

#[test]
fn restore_default_shows_a_hidden_frame() {
    let mut m = machine(SAVED, None, true);
    let commands = m.restore_default(Some(MONITOR), false, Insets::ZERO);
    assert!(commands.contains(&GeomCommand::ShowFrame));
}

#[test]
fn restore_default_without_a_monitor_keeps_the_saved_position() {
    let mut m = machine((Some(300), Some(200), 800, 600), None, true);
    let commands = m.restore_default(None, true, Insets::ZERO);
    assert!(!commands
        .iter()
        .any(|c| matches!(c, GeomCommand::SetOuterPosition(_))));
    assert!(commands.contains(&GeomCommand::SaveGeometry((Some(300), Some(200), 480, 320))));
}

// ---------- 四边内距（V2 相框外观专题工单 01） ----------

/// 四边不等的内距：左 10 / 上 20 / 右 30 / 下 40（横合计 40、纵合计 60）。
const UNEQUAL_INSETS: Insets = Insets {
    left: 10.0,
    top: 20.0,
    right: 30.0,
    bottom: 40.0,
};

#[test]
fn unequal_insets_derive_the_outer_size_from_the_outer_height() {
    // 比例 2.0：外框高 380 → 画面高 380−60=320 → 画面宽 640 →
    // 外框宽 640+40=680（基准帧自洽，无命令）。
    let saved: GeometryRecord = (Some(100), Some(100), 680, 380);
    let mut m = machine(saved, Some(2.0), true);
    assert!(sizes(&m.observe(&obs_with_insets(
        [100.0, 100.0],
        [680.0, 380.0],
        0.0,
        UNEQUAL_INSETS
    )))
    .is_empty());
    // 外框高拉到 440：画面高 380 → 画面宽 760 → 外框 [800, 440]。
    let commands = m.observe(&obs_with_insets(
        [100.0, 100.0],
        [680.0, 440.0],
        0.1,
        UNEQUAL_INSETS,
    ));
    assert_size(sizes(&commands)[0], [800.0, 440.0]);
}

#[test]
fn zero_insets_match_the_pre_insets_derivation() {
    // 内距全 0 时与 V1 推导逐项一致：比例 1.5、高驱动缩放。
    let saved: GeometryRecord = (Some(100), Some(100), 480, 320);
    let mut m = machine(saved, Some(1.5), true);
    m.observe(&obs_with_insets(
        [100.0, 100.0],
        [480.0, 320.0],
        0.0,
        Insets::ZERO,
    ));
    let commands = m.observe(&obs_with_insets(
        [100.0, 100.0],
        [480.0, 400.0],
        0.1,
        Insets::ZERO,
    ));
    assert_size(sizes(&commands)[0], [600.0, 400.0]);
}

#[test]
fn changing_insets_rederives_the_width_at_the_same_outer_height() {
    // 外框高度不变、内距从全 0 变为四边不等：画面高 320−60=260 →
    // 画面宽 520 → 外框宽 520+40=560，高度仍是外框总高 320。
    let saved: GeometryRecord = (Some(100), Some(100), 640, 320);
    let mut m = machine(saved, Some(2.0), true);
    m.observe(&obs_with_insets(
        [100.0, 100.0],
        [640.0, 320.0],
        0.0,
        Insets::ZERO,
    ));
    let commands = m.observe(&obs_with_insets(
        [100.0, 100.0],
        [640.0, 320.0],
        0.1,
        UNEQUAL_INSETS,
    ));
    assert_size(sizes(&commands)[0], [560.0, 320.0]);
}

#[test]
fn restore_default_sizes_by_outer_height_with_insets() {
    // 恢复默认按外框总高 320：画面高 320−60=260 → 画面宽 520 →
    // 外框 [560, 320]，落盘与放置都按外框尺寸。
    let mut m = machine((Some(300), Some(200), 800, 600), Some(2.0), true);
    let commands = m.restore_default(Some(MONITOR), true, UNEQUAL_INSETS);
    assert_eq!(
        commands,
        vec![
            GeomCommand::SetInnerSize([560.0, 320.0]),
            GeomCommand::SetOuterPosition([1336.0, 736.0]),
            GeomCommand::SaveGeometry((Some(1336), Some(736), 560, 320)),
        ]
    );
}
