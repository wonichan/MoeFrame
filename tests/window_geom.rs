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
        work_area: None,
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

/// 模拟一次用户拖拽：按下帧不抢尺寸，松手帧按名义高度校正。
fn drag_then_release(
    m: &mut GeometryMachine,
    pos: [f32; 2],
    inner: [f32; 2],
    now: f64,
    insets: Insets,
) -> Vec<GeomCommand> {
    let mut dragging = obs_with_insets(pos, inner, now, insets);
    dragging.pointer_down = true;
    assert!(sizes(&m.observe(&dragging)).is_empty(), "拖动中不抢尺寸");
    m.observe(&obs_with_insets(pos, inner, now + 0.01, insets))
}

fn assert_size(actual: [f32; 2], expected: [f32; 2]) {
    assert!(
        (actual[0] - expected[0]).abs() < 0.51 && (actual[1] - expected[1]).abs() < 0.51,
        "期望 {expected:?}，实际 {actual:?}"
    );
}

/// 指定工作区的一帧观察（含屏幕原点与任务栏扣除）。
fn obs_in_work_area(
    pos: [f32; 2],
    inner: [f32; 2],
    now: f64,
    work_area: [f32; 4],
) -> GeomObservation {
    let mut observation = obs(pos, inner, now);
    observation.work_area = Some(work_area);
    observation
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
    let commands = drag_then_release(&mut m, [100.0, 100.0], [600.0, 320.0], 0.1, Insets::ZERO);
    assert_size(sizes(&commands)[0], [480.0, 320.0]);
}

#[test]
fn a_height_driven_resize_derives_the_width() {
    let mut m = locked_machine(1.5, [480.0, 320.0]);
    let commands = drag_then_release(&mut m, [100.0, 100.0], [480.0, 400.0], 0.1, Insets::ZERO);
    assert_size(sizes(&commands)[0], [600.0, 400.0]);
}

#[test]
fn a_corner_resize_keeps_height_as_the_scale_reference() {
    let mut m = locked_machine(1.5, [480.0, 320.0]);
    let commands = drag_then_release(&mut m, [100.0, 100.0], [600.0, 500.0], 0.1, Insets::ZERO);
    assert_size(sizes(&commands)[0], [750.0, 500.0]);
}

#[test]
fn minimum_window_sides_preserve_the_image_ratio() {
    let mut m = locked_machine(1.5, [480.0, 320.0]);
    let commands = drag_then_release(&mut m, [100.0, 100.0], [600.0, 80.0], 0.1, Insets::ZERO);
    assert_size(sizes(&commands)[0], [180.0, 120.0]);

    let mut m = locked_machine(0.5, [160.0, 320.0]);
    let commands = drag_then_release(&mut m, [100.0, 100.0], [100.0, 80.0], 0.1, Insets::ZERO);
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
    let commands = drag_then_release(&mut m, [100.0, 100.0], [680.0, 440.0], 0.1, UNEQUAL_INSETS);
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
    let commands = drag_then_release(&mut m, [100.0, 100.0], [480.0, 400.0], 0.1, Insets::ZERO);
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

// ---------- 落盘时机：移动后不必等下一次轮播 ----------

fn saves(commands: &[GeomCommand]) -> Vec<GeometryRecord> {
    commands
        .iter()
        .filter_map(|c| match c {
            GeomCommand::SaveGeometry(record) => Some(*record),
            _ => None,
        })
        .collect()
}

#[test]
fn pending_geometry_saves_when_the_settle_window_ends() {
    let mut m = machine(SAVED, None, true);
    let commands = m.observe(&obs([300.0, 180.0], [480.0, 320.0], 1.0));
    assert!(saves(&commands).is_empty(), "刚移动不该立刻落盘");
    // 宿主据此安排一帧，不必睡到下一次轮播。
    assert_eq!(m.settle_deadline_secs(), Some(1.6));
    let commands = m.observe(&obs([300.0, 180.0], [480.0, 320.0], 1.61));
    assert_eq!(saves(&commands), vec![(Some(300), Some(180), 480, 320)]);
    assert_eq!(m.settle_deadline_secs(), None);
}

#[test]
fn hiding_flushes_the_pending_geometry() {
    let mut m = machine(SAVED, None, true);
    assert!(saves(&m.observe(&obs([300.0, 180.0], [480.0, 320.0], 1.0))).is_empty());
    // 隐藏帧不再更新观测量：等不到"停稳 0.6 秒"也必须落盘，
    // 否则"移动完随手关到托盘"就丢位置。
    let mut hidden = obs([300.0, 180.0], [480.0, 320.0], 1.05);
    hidden.frame_visible = false;
    assert_eq!(
        saves(&m.observe(&hidden)),
        vec![(Some(300), Some(180), 480, 320)]
    );
    assert_eq!(m.settle_deadline_secs(), None);
    // 再出隐藏帧不会重复写盘。
    assert!(saves(&m.observe(&hidden)).is_empty());
}

// ---------- 切图几何动画：中心锚定、工作区钳制、名义高度 ----------

#[test]
fn transition_grows_around_the_old_center_without_moving_the_center() {
    let mut m = machine(SAVED, Some(1.5), true);
    let area = [0.0, 0.0, 1000.0, 800.0];
    m.observe(&obs_in_work_area([100.0, 100.0], [480.0, 320.0], 0.0, area));
    m.begin_image_transition(Some(2.0), 0.1, 0.24);
    let commands = m.observe(&obs_in_work_area(
        [100.0, 100.0],
        [480.0, 320.0],
        0.34,
        area,
    ));
    assert_size(sizes(&commands)[0], [640.0, 320.0]);
    assert_eq!(
        commands.iter().find_map(|c| match c {
            GeomCommand::SetOuterPosition(p) => Some(*p),
            _ => None,
        }),
        Some([20.0, 100.0])
    );
}

#[test]
fn transition_clamps_the_target_to_the_six_pixel_work_area_margin() {
    let mut m = machine((Some(500), Some(100), 480, 320), Some(1.5), true);
    let area = [0.0, 0.0, 1000.0, 800.0];
    m.observe(&obs_in_work_area([500.0, 100.0], [480.0, 320.0], 0.0, area));
    m.begin_image_transition(Some(2.5), 0.1, 0.24);
    let commands = m.observe(&obs_in_work_area(
        [500.0, 100.0],
        [480.0, 320.0],
        0.34,
        area,
    ));
    assert_size(sizes(&commands)[0], [800.0, 320.0]);
    // 右边最多贴到 1000−6，左边相应变成 194。
    assert_eq!(
        commands.iter().find_map(|c| match c {
            GeomCommand::SetOuterPosition(p) => Some(*p),
            _ => None,
        }),
        Some([194.0, 100.0])
    );
}

#[test]
fn an_oversized_image_shrinks_for_display_but_keeps_the_nominal_height_on_disk() {
    let mut m = machine(SAVED, Some(1.5), true);
    let area = [0.0, 0.0, 1000.0, 800.0];
    m.observe(&obs_in_work_area([100.0, 100.0], [480.0, 320.0], 0.0, area));
    m.begin_image_transition(Some(4.0), 0.1, 0.24);
    let commands = m.observe(&obs_in_work_area(
        [100.0, 100.0],
        [480.0, 320.0],
        0.34,
        area,
    ));
    let shown = sizes(&commands)[0];
    assert!(
        (shown[0] - 988.0).abs() < 0.51,
        "宽度应缩到工作区可用宽：{shown:?}"
    );
    assert!(shown[1] < 320.0, "高度应等比缩小：{shown:?}");
    // 落盘仍按名义高度 320：切回竖图时回到原来高度。
    let record = m.exit_settle().0.expect("动画结束应有落盘基准");
    assert_eq!(
        (record.2, record.3),
        (1280, 320),
        "名义尺寸不随临时缩高变化"
    );
}

#[test]
fn transition_interpolates_and_ends_at_the_target_without_saving_intermediate_sizes() {
    let mut m = machine(SAVED, Some(1.5), true);
    let area = [0.0, 0.0, 1000.0, 800.0];
    m.observe(&obs_in_work_area([100.0, 100.0], [480.0, 320.0], 0.0, area));
    m.begin_image_transition(Some(2.0), 0.1, 0.24);
    assert!(m
        .transition_deadline_secs()
        .is_some_and(|deadline| (deadline - 0.34).abs() < 1e-6));
    let mid = m.observe(&obs_in_work_area(
        [100.0, 100.0],
        [480.0, 320.0],
        0.22,
        area,
    ));
    let mid_size = sizes(&mid)[0];
    assert!(
        mid_size[0] > 480.0 && mid_size[0] < 640.0,
        "中间帧应插值：{mid_size:?}"
    );
    assert!(saves(&mid).is_empty(), "动画中不落盘");
    let end = m.observe(&obs_in_work_area([20.0, 100.0], [640.0, 320.0], 0.34, area));
    assert!(
        end.is_empty(),
        "到点后实际几何已等于目标，不再发命令：{end:?}"
    );
    assert_eq!(m.transition_deadline_secs(), None);
}

#[test]
fn a_temporary_clamp_does_not_move_the_nominal_center() {
    // 用户把相框放在右侧，中心 x=740。
    let saved: GeometryRecord = (Some(500), Some(100), 480, 320);
    let mut m = machine(saved, Some(1.5), true);
    let area = [0.0, 0.0, 1000.0, 800.0];
    m.observe(&obs_in_work_area([500.0, 100.0], [480.0, 320.0], 0.0, area));
    // 宽图 2.5 → 目标 800 宽，右边越界，显示位置被钳到 x=194。
    m.begin_image_transition(Some(2.5), 0.1, 0.24);
    let wide = m.observe(&obs_in_work_area(
        [500.0, 100.0],
        [480.0, 320.0],
        0.34,
        area,
    ));
    assert_size(sizes(&wide)[0], [800.0, 320.0]);
    assert_eq!(
        wide.iter().find_map(|c| match c {
            GeomCommand::SetOuterPosition(p) => Some(*p),
            _ => None,
        }),
        Some([194.0, 100.0])
    );
    // 已按钳制后的实际位置落地。
    m.observe(&obs_in_work_area(
        [194.0, 100.0],
        [800.0, 320.0],
        0.35,
        area,
    ));
    // 落盘位置也按名义中心算，不把临时钳制后的 x=194 当成新家。
    let persisted = m.exit_settle().0.expect("应有落盘基准");
    assert_eq!((persisted.0, persisted.1), (Some(340), Some(100)));
    // 切回窄图：应回到原来的中心 x=740，而不是停在右边。
    m.begin_image_transition(Some(1.5), 0.4, 0.24);
    let narrow = m.observe(&obs_in_work_area(
        [194.0, 100.0],
        [800.0, 320.0],
        0.64,
        area,
    ));
    assert_size(sizes(&narrow)[0], [480.0, 320.0]);
    assert_eq!(
        narrow.iter().find_map(|c| match c {
            GeomCommand::SetOuterPosition(p) => Some(*p),
            _ => None,
        }),
        Some([500.0, 100.0])
    );
}

#[test]
fn a_native_move_after_pointer_events_are_lost_keeps_the_new_position() {
    let mut m = locked_machine(1.5, [480.0, 320.0]);
    // Windows 原生拖动期间可能一帧都没有；松手后的第一帧只带
    // “刚结束原生手势”的标志，也要按最终位置更新名义中心。
    let mut after_native_move = obs([300.0, 200.0], [480.0, 320.0], 0.2);
    after_native_move.pointer_down = true;
    assert!(m.observe(&after_native_move).is_empty());
    // 下一帧按新的名义中心校正，不能把窗口拉回按下时的位置。
    let commands = m.observe(&obs([300.0, 200.0], [480.0, 320.0], 0.3));
    assert!(!commands
        .iter()
        .any(|c| matches!(c, GeomCommand::SetOuterPosition(_))));
}

#[test]
fn user_drag_interrupts_the_geometry_animation_without_a_snap_back() {
    let mut m = machine(SAVED, Some(1.5), true);
    let area = [0.0, 0.0, 1920.0, 1080.0];
    m.observe(&obs_in_work_area([100.0, 100.0], [480.0, 320.0], 0.0, area));
    m.begin_image_transition(Some(1.0), 0.1, 0.24);
    // 动画第一帧：用户只按下、还没移动，也应立刻打断。
    let mut dragging = obs_in_work_area([100.0, 100.0], [480.0, 320.0], 0.2, area);
    dragging.pointer_down = true;
    let commands = m.observe(&dragging);
    assert_eq!(commands, vec![GeomCommand::TransitionInterrupted]);
    assert_eq!(m.transition_deadline_secs(), None);
    // 拖动中不再抢方向盘；松手后才按新中心做一次比例校正。
    let mut still_dragging = obs_in_work_area([140.0, 120.0], [560.0, 320.0], 0.3, area);
    still_dragging.pointer_down = true;
    assert!(m.observe(&still_dragging).is_empty());
    let released = m.observe(&obs_in_work_area([140.0, 120.0], [560.0, 420.0], 0.4, area));
    assert_size(sizes(&released)[0], [420.0, 420.0]);
    assert_eq!(
        released.iter().find_map(|c| match c {
            GeomCommand::SetOuterPosition(p) => Some(*p),
            _ => None,
        }),
        Some([210.0, 70.0])
    );
}
