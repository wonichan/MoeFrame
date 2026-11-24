//! 多实例吸附专题工单 02：窗口吸附的纯逻辑——伙伴判定、接触派生
//! 与松手吸附求解全部在库 snap 的 interface 上验证（宿主 Win32 枚举
//! 层只把窗口事实喂进来，不含判定）。

use moe_frame::snap::{
    derive_contacts, is_partner_frame, partner_frames, physical_snap_threshold, snap_groups,
    solve_drag_snap, CandidateWindow, DragSnapPartner, PartnerFrame, SnapEdge, WindowRect,
    SNAP_THRESHOLD,
};

const OWN_EXE: &str = r"C:\Apps\MoeFrame\MoeFrame.exe";
/// 主屏：原点 (0,0)，工作区扣掉底部任务栏。
const MONITOR_MAIN: WindowRect = WindowRect {
    left: 0.0,
    top: 0.0,
    right: 1920.0,
    bottom: 1080.0,
};
const WORK_MAIN: WindowRect = WindowRect {
    left: 0.0,
    top: 0.0,
    right: 1920.0,
    bottom: 1040.0,
};
/// 副屏：原点在主屏右侧。
const MONITOR_SECOND: WindowRect = WindowRect {
    left: 1920.0,
    top: 0.0,
    right: 3840.0,
    bottom: 1080.0,
};

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> WindowRect {
    WindowRect {
        left,
        top,
        right,
        bottom,
    }
}

fn candidate(title: &str, exe: Option<&str>, rect: WindowRect) -> CandidateWindow {
    CandidateWindow {
        title: title.to_owned(),
        exe_path: exe.map(str::to_owned),
        owner_alive: true,
        is_self_window: false,
        rect,
        monitor: MONITOR_MAIN,
        work_area: WORK_MAIN,
    }
}

fn partner(rect: WindowRect) -> PartnerFrame {
    PartnerFrame {
        rect,
        monitor: MONITOR_MAIN,
        work_area: WORK_MAIN,
    }
}

// ---------- 伙伴判定 ----------

#[test]
fn partner_requires_frame_title_same_exe_alive_and_not_self() {
    let good = candidate("MoeFrame", Some(OWN_EXE), rect(0.0, 0.0, 100.0, 100.0));
    assert!(is_partner_frame(&good, OWN_EXE));
}

#[test]
fn partner_rejects_same_title_from_another_program() {
    // 同标题异进程：标题对得上、exe 对不上，不入列。
    let alien = candidate(
        "MoeFrame",
        Some(r"C:\Other\NotMoeFrame.exe"),
        rect(0.0, 0.0, 100.0, 100.0),
    );
    assert!(!is_partner_frame(&alien, OWN_EXE));
}

#[test]
fn partner_rejects_own_settings_window() {
    // 设置窗混入：同进程同 exe，但标题不是相框标题，不入列。
    let settings = candidate(
        "萌框设置 · 相框 1",
        Some(OWN_EXE),
        rect(0.0, 0.0, 420.0, 910.0),
    );
    assert!(!is_partner_frame(&settings, OWN_EXE));
}

#[test]
fn partner_rejects_self_dead_owner_and_unknown_exe() {
    let mut own = candidate("MoeFrame", Some(OWN_EXE), rect(0.0, 0.0, 100.0, 100.0));
    own.is_self_window = true;
    assert!(!is_partner_frame(&own, OWN_EXE), "自己的窗口不入列");

    let mut dead = candidate("MoeFrame", Some(OWN_EXE), rect(0.0, 0.0, 100.0, 100.0));
    dead.owner_alive = false;
    assert!(
        !is_partner_frame(&dead, OWN_EXE),
        "已退出实例的残留窗口不入列"
    );

    let unknown = candidate("MoeFrame", None, rect(0.0, 0.0, 100.0, 100.0));
    assert!(!is_partner_frame(&unknown, OWN_EXE), "取不到 exe 时不入列");
}

#[test]
fn partner_exe_compare_tolerates_case_and_separator_spelling() {
    let spelled = candidate(
        "MoeFrame",
        Some("c:/apps/moeframe/moeframe.exe"),
        rect(0.0, 0.0, 100.0, 100.0),
    );
    assert!(is_partner_frame(&spelled, OWN_EXE));
}

#[test]
fn partner_frames_filters_and_keeps_geometry() {
    let candidates = vec![
        candidate("MoeFrame", Some(OWN_EXE), rect(0.0, 0.0, 100.0, 100.0)),
        candidate(
            "MoeFrame",
            Some(r"C:\Other\NotMoeFrame.exe"),
            rect(200.0, 0.0, 300.0, 100.0),
        ),
        candidate(
            "萌框设置 · 相框 2",
            Some(OWN_EXE),
            rect(400.0, 0.0, 500.0, 100.0),
        ),
    ];
    let frames = partner_frames(&candidates, OWN_EXE);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].rect, rect(0.0, 0.0, 100.0, 100.0));
    assert_eq!(frames[0].monitor, MONITOR_MAIN);
}

// ---------- 接触派生 ----------

#[test]
fn contact_single_shared_edge() {
    let frames = vec![
        partner(rect(0.0, 0.0, 100.0, 100.0)),
        partner(rect(100.0, 0.0, 200.0, 100.0)),
    ];
    let contacts = derive_contacts(&frames);
    assert_eq!(contacts.len(), 1);
    assert_eq!((contacts[0].a, contacts[0].b), (0, 1));
    assert_eq!(snap_groups(&frames), vec![vec![0, 1]]);
}

#[test]
fn contact_chain_of_three_forms_one_group() {
    let frames = vec![
        partner(rect(0.0, 0.0, 100.0, 100.0)),
        partner(rect(100.0, 0.0, 200.0, 100.0)),
        partner(rect(200.0, 0.0, 300.0, 100.0)),
    ];
    assert_eq!(derive_contacts(&frames).len(), 2);
    assert_eq!(snap_groups(&frames), vec![vec![0, 1, 2]]);
}

#[test]
fn contact_l_shape_two_edges() {
    // 窗 2 同时贴住窗 0 的右边与窗 1 的下边：两条接触边、一个组。
    let frames = vec![
        partner(rect(0.0, 0.0, 100.0, 100.0)),
        partner(rect(100.0, 100.0, 200.0, 200.0)),
        partner(rect(100.0, 0.0, 200.0, 100.0)),
    ];
    assert_eq!(derive_contacts(&frames).len(), 2);
    assert_eq!(snap_groups(&frames), vec![vec![0, 1, 2]]);
}

#[test]
fn contact_none_when_apart() {
    let frames = vec![
        partner(rect(0.0, 0.0, 100.0, 100.0)),
        partner(rect(300.0, 300.0, 400.0, 400.0)),
    ];
    assert!(derive_contacts(&frames).is_empty());
    assert_eq!(snap_groups(&frames), vec![vec![0], vec![1]]);
}

#[test]
fn contact_gap_beyond_tolerance_does_not_count() {
    let frames = vec![
        partner(rect(0.0, 0.0, 100.0, 100.0)),
        partner(rect(105.0, 0.0, 205.0, 100.0)),
    ];
    assert!(derive_contacts(&frames).is_empty());
}

#[test]
fn contact_across_monitors_does_not_count() {
    // 几何上恰好相邻，但分属两屏：不判贴合。
    let mut second = partner(rect(1920.0, 0.0, 2020.0, 100.0));
    second.monitor = MONITOR_SECOND;
    let frames = vec![partner(rect(1820.0, 0.0, 1920.0, 100.0)), second];
    assert!(derive_contacts(&frames).is_empty());
    assert_eq!(snap_groups(&frames), vec![vec![0], vec![1]]);
}

#[test]
fn contact_requires_projection_overlap() {
    // 边线对齐但纵向投影完全错开：不是贴合。
    let frames = vec![
        partner(rect(0.0, 0.0, 100.0, 100.0)),
        partner(rect(100.0, 200.0, 200.0, 300.0)),
    ];
    assert!(derive_contacts(&frames).is_empty());
}

// ---------- 拖动中吸附求解（工单 05：物理像素 + 二维评分） ----------

fn drag_partner(hwnd: u64, rect: WindowRect) -> DragSnapPartner {
    DragSnapPartner { hwnd, rect }
}

#[test]
fn threshold_scales_with_dpi() {
    assert_eq!(physical_snap_threshold(96), 12.0, "100%");
    assert_eq!(physical_snap_threshold(120), 15.0, "125%");
    assert_eq!(physical_snap_threshold(144), 18.0, "150%");
    assert_eq!(physical_snap_threshold(0), 12.0, "取不到 DPI 时退回逻辑值");
}

#[test]
fn drag_attach_zeroes_gap_and_records_hit() {
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![drag_partner(7, rect(108.0, 0.0, 208.0, 100.0))];
    let outcome =
        solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD).expect("阈值内贴合应命中");
    assert_eq!(outcome.target, rect(8.0, 0.0, 108.0, 100.0));
    assert_eq!(outcome.hits.len(), 1);
    assert_eq!(outcome.hits[0].partner_hwnd, 7);
    assert_eq!(outcome.hits[0].edge, SnapEdge::Right);
    assert_eq!(outcome.hits[0].target, outcome.target);
}

#[test]
fn drag_beyond_threshold_does_nothing() {
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![drag_partner(7, rect(113.0, 0.0, 213.0, 100.0))];
    assert_eq!(
        solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD),
        None
    );
}

#[test]
fn drag_counterexample_axes_attach_to_different_partners() {
    // 反例 1：X 轴吸向伙伴 A、Y 轴吸向伙伴 B，两轴独立裁决组合落位。
    let moving = rect(200.0, 200.0, 300.0, 300.0);
    let partners = vec![
        drag_partner(1, rect(306.0, 200.0, 406.0, 300.0)),
        drag_partner(2, rect(200.0, 306.0, 300.0, 406.0)),
    ];
    let outcome = solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, rect(206.0, 206.0, 306.0, 306.0));
    assert_eq!(outcome.hits.len(), 2);
    assert_eq!(outcome.hits[0].partner_hwnd, 1);
    assert_eq!(outcome.hits[0].edge, SnapEdge::Right);
    assert_eq!(outcome.hits[1].partner_hwnd, 2);
    assert_eq!(outcome.hits[1].edge, SnapEdge::Bottom);
}

#[test]
fn drag_counterexample_grazing_projection_loses_to_head_on_partner() {
    // 反例 2：仅擦边投影（重叠 5%）的近伙伴不得压过正对（重叠
    // 100%）的稍远伙伴——投影重叠比例参与评分。
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![
        drag_partner(1, rect(102.0, 95.0, 202.0, 195.0)),
        drag_partner(2, rect(106.0, 0.0, 206.0, 100.0)),
    ];
    let outcome = solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, rect(6.0, 0.0, 106.0, 100.0));
    assert_eq!(outcome.hits[0].partner_hwnd, 2);
}

#[test]
fn drag_counterexample_far_alignment_does_not_steal_near_attach() {
    // 反例 3：远处伙伴的对齐（另一轴相距过远直接不触发；另一轴
    // 邻近但综合评分更差的对齐）不得抢走近处贴合。
    let moving = rect(200.0, 200.0, 300.0, 300.0);
    let partners = vec![
        drag_partner(1, rect(308.0, 200.0, 408.0, 300.0)),
        drag_partner(2, rect(201.0, 600.0, 301.0, 700.0)),
        drag_partner(3, rect(201.0, 310.0, 301.0, 410.0)),
    ];
    let outcome = solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD).expect("应命中");
    // 横轴取伙伴 1 的贴合（不被对齐抢走）；伙伴 3 在纵轴另构成真
    // 贴合（下边贴其上边，间隙 10），纵轴独立命中。
    assert_eq!(outcome.target, rect(208.0, 210.0, 308.0, 310.0));
    assert_eq!(outcome.hits[0].partner_hwnd, 1);
    assert_eq!(outcome.hits[0].edge, SnapEdge::Right);
    assert_eq!(outcome.hits[1].partner_hwnd, 3);
    assert_eq!(outcome.hits[1].edge, SnapEdge::Bottom);
}

#[test]
fn drag_counterexample_work_area_does_not_beat_nearer_partner_by_category() {
    // 反例 4a（同轴比较）：纵轴上工作区上边缘贴合间隙 10，不得凭
    // 类别压过明显更近的伙伴顶边对齐（间隙 1，另一轴投影重叠）。
    let moving = rect(300.0, 10.0, 400.0, 110.0);
    let partners = vec![drag_partner(5, rect(320.0, 11.0, 420.0, 111.0))];
    let outcome =
        solve_drag_snap(moving, &partners, Some(WORK_MAIN), SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, rect(300.0, 11.0, 400.0, 111.0));
    assert!(outcome.hits.is_empty(), "对齐命中不产出贴合接缝");
    // 反例 4b：同为贴合时更近的伙伴（间隙 2）压过工作区（间隙 10）。
    let partners = vec![drag_partner(5, rect(300.0, 112.0, 400.0, 212.0))];
    let outcome =
        solve_drag_snap(moving, &partners, Some(WORK_MAIN), SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, rect(300.0, 12.0, 400.0, 112.0));
    assert_eq!(outcome.hits[0].partner_hwnd, 5);
    assert_eq!(outcome.hits[0].edge, SnapEdge::Bottom);
}

#[test]
fn drag_attach_beats_alignment_at_equal_proximity() {
    // 同等邻近程度下贴合优先于对齐：横轴上右边贴合间隙 4（目标
    // x=204）对左边缘对齐间隙 4（目标 x=196），取贴合。
    let moving = rect(200.0, 200.0, 300.0, 300.0);
    let partners = vec![
        drag_partner(1, rect(304.0, 200.0, 404.0, 300.0)),
        drag_partner(2, rect(196.0, 312.0, 296.0, 412.0)),
    ];
    let outcome = solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, rect(204.0, 200.0, 304.0, 300.0));
    assert_eq!(outcome.hits.len(), 1);
    assert_eq!(outcome.hits[0].partner_hwnd, 1);
    assert_eq!(outcome.hits[0].edge, SnapEdge::Right);
}

#[test]
fn drag_work_area_edge_still_attaches_without_partners() {
    let moving = rect(8.0, 300.0, 108.0, 400.0);
    let outcome = solve_drag_snap(moving, &[], Some(WORK_MAIN), SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, rect(0.0, 300.0, 100.0, 400.0));
    assert!(outcome.hits.is_empty());
}

#[test]
fn drag_solve_is_idempotent_at_target() {
    // 已在贴合目标位：求解仍报告命中（接缝要记已贴合事实），且
    // 目标与现位一致——重复求解稳定、不漂移。
    let moving = rect(8.0, 0.0, 108.0, 100.0);
    let partners = vec![drag_partner(7, rect(108.0, 0.0, 208.0, 100.0))];
    let outcome = solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, moving);
    assert_eq!(outcome.hits.len(), 1);
}

#[test]
fn drag_attach_left_and_top_edges_record_their_edges() {
    let moving = rect(200.0, 200.0, 300.0, 300.0);
    let partners = vec![drag_partner(3, rect(94.0, 200.0, 194.0, 300.0))];
    let outcome = solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, rect(194.0, 200.0, 294.0, 300.0));
    assert_eq!(outcome.hits[0].edge, SnapEdge::Left);
    let partners = vec![drag_partner(4, rect(200.0, 94.0, 300.0, 194.0))];
    let outcome = solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, rect(200.0, 194.0, 300.0, 294.0));
    assert_eq!(outcome.hits[0].edge, SnapEdge::Top);
}

#[test]
fn drag_alignment_fires_when_other_axis_is_near() {
    // 顶边相差 5、横向投影重叠（另一轴邻近）：对齐成线，不产出
    // 贴合命中（伙伴与被拖窗横向重叠，相对边间隙远超阈值）。
    let moving = rect(100.0, 100.0, 200.0, 200.0);
    let partners = vec![drag_partner(9, rect(150.0, 105.0, 250.0, 205.0))];
    let outcome = solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target.top, 105.0);
    assert!(outcome.hits.is_empty());
}

#[test]
fn drag_physical_threshold_governs_the_gap() {
    // 间隙 15：逻辑阈值 12 不吸，150% DPI 的物理阈值 18 吸。
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![drag_partner(7, rect(115.0, 0.0, 215.0, 100.0))];
    assert_eq!(
        solve_drag_snap(moving, &partners, None, SNAP_THRESHOLD),
        None
    );
    let outcome = solve_drag_snap(moving, &partners, None, physical_snap_threshold(144))
        .expect("物理阈值内应命中");
    assert_eq!(outcome.target, rect(15.0, 0.0, 115.0, 100.0));
}

#[test]
fn drag_work_area_right_and_bottom_edges_attach() {
    let moving = rect(1810.0, 930.0, 1910.0, 1030.0);
    let outcome = solve_drag_snap(moving, &[], Some(WORK_MAIN), SNAP_THRESHOLD).expect("应命中");
    assert_eq!(outcome.target, rect(1820.0, 940.0, 1920.0, 1040.0));
    assert!(outcome.hits.is_empty());
}
