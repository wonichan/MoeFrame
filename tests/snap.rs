//! 多实例吸附专题工单 02：窗口吸附的纯逻辑——伙伴判定、接触派生
//! 与松手吸附求解全部在库 snap 的 interface 上验证（宿主 Win32 枚举
//! 层只把窗口事实喂进来，不含判定）。

use moe_frame::snap::{
    derive_contacts, is_partner_frame, partner_frames, snap_groups, solve_snap, CandidateWindow,
    PartnerFrame, SnapPartner, WindowRect, SNAP_THRESHOLD,
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

fn snap_partner(rect: WindowRect) -> SnapPartner {
    SnapPartner {
        rect,
        monitor: MONITOR_MAIN,
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

// ---------- 松手吸附求解 ----------

#[test]
fn solve_attaches_right_edge_and_zeroes_the_gap() {
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![snap_partner(rect(108.0, 0.0, 208.0, 100.0))];
    let target = solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN));
    assert_eq!(target, Some([8.0, 0.0]), "右边缘贴到伙伴左边，间隙归零");
}

#[test]
fn solve_attaches_each_relative_edge() {
    let moving = rect(200.0, 200.0, 300.0, 300.0);
    // 伙伴在右：被拖窗右边贴伙伴左边。
    let partners = vec![snap_partner(rect(306.0, 200.0, 406.0, 300.0))];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        Some([206.0, 200.0])
    );
    // 伙伴在左：被拖窗左边贴伙伴右边。
    let partners = vec![snap_partner(rect(94.0, 200.0, 194.0, 300.0))];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        Some([194.0, 200.0])
    );
    // 伙伴在下：被拖窗下边贴伙伴上边。
    let partners = vec![snap_partner(rect(200.0, 306.0, 300.0, 406.0))];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        Some([200.0, 206.0])
    );
    // 伙伴在上：被拖窗上边贴伙伴下边。
    let partners = vec![snap_partner(rect(200.0, 94.0, 300.0, 194.0))];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        Some([200.0, 194.0])
    );
}

#[test]
fn solve_beyond_threshold_does_nothing() {
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![snap_partner(rect(
        100.0 + SNAP_THRESHOLD + 1.0,
        0.0,
        300.0,
        100.0,
    ))];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        None
    );
}

#[test]
fn solve_at_exact_threshold_still_attaches() {
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![snap_partner(rect(
        100.0 + SNAP_THRESHOLD,
        0.0,
        300.0,
        100.0,
    ))];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        Some([SNAP_THRESHOLD, 0.0])
    );
}

#[test]
fn solve_aligns_same_name_edges_without_contact() {
    // 顶边相差 6px、横向完全错开：对齐成一条线，不要求接触。
    let moving = rect(0.0, 100.0, 100.0, 200.0);
    let partners = vec![snap_partner(rect(500.0, 106.0, 600.0, 206.0))];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        Some([0.0, 106.0])
    );
}

#[test]
fn solve_attach_wins_over_alignment() {
    // 同一轴上贴合间隙 10px、对齐间隙 2px 同时命中：贴合优先（定死规则）。
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![
        // 右侧伙伴：右边贴合候选，间隙 10。
        snap_partner(rect(110.0, 0.0, 210.0, 100.0)),
        // 下方伙伴：与被拖窗纵向错开（无贴合候选），其左右边与被拖窗
        // 同名边只差 2px，构成间隙更小的对齐候选。
        snap_partner(rect(-2.0, 300.0, 98.0, 400.0)),
    ];
    let target = solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN));
    assert_eq!(target, Some([10.0, 0.0]), "横轴取贴合目标而非对齐目标");
}

#[test]
fn solve_attach_on_one_edge_beats_smaller_gap_alignment_on_the_other() {
    // 审查修复 S6 回归：跨边组裁决类别先行——右边贴合（间隙 10）
    // 对左边对齐（间隙 2），旧实现跨组只比间隙会误取对齐。
    // 不给工作区候选，排除其干扰。
    let moving = rect(200.0, 200.0, 300.0, 300.0);
    let partners = vec![
        // 右侧伙伴：右边贴合候选，间隙 10，目标 x=210。
        snap_partner(rect(310.0, 200.0, 410.0, 300.0)),
        // 纵向错开的伙伴：左边缘与被拖窗左边缘只差 2，构成小间隙
        // 对齐候选（目标 x=198），无贴合候选。
        snap_partner(rect(198.0, 500.0, 298.0, 600.0)),
    ];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, None),
        Some([210.0, 200.0]),
        "横轴取贴合目标而非小间隙对齐目标"
    );
}

#[test]
fn solve_nearest_partner_wins_and_tie_keeps_enumeration_order() {
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    // 近者（间隙 4）排在远者（间隙 9）之后：仍取近者。
    let partners = vec![
        snap_partner(rect(109.0, 0.0, 209.0, 100.0)),
        snap_partner(rect(104.0, 0.0, 204.0, 100.0)),
    ];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        Some([4.0, 0.0])
    );
    // 同距两伙伴（右贴目标 +8、左贴目标 −8）：取枚举顺序在先的一个。
    let partners = vec![
        snap_partner(rect(108.0, 0.0, 208.0, 100.0)),
        snap_partner(rect(-108.0, 0.0, -8.0, 100.0)),
    ];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        Some([8.0, 0.0])
    );
}

#[test]
fn solve_combines_axes_attach_on_x_align_on_y() {
    // 横轴贴合（间隙 6）、纵轴顶边对齐（差 4）：两轴各自裁决、组合落位。
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![snap_partner(rect(106.0, 4.0, 206.0, 104.0))];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        Some([6.0, 4.0])
    );
}

#[test]
fn solve_work_area_edge_zeroes_gap_without_reserve() {
    // 距工作区左边 8px：贴到边缘本身（间隙归零，不保留工单 06 的
    // 6px 钳制保留距离——两条路径口径互不引用）。
    let moving = rect(8.0, 300.0, 108.0, 400.0);
    assert_eq!(
        solve_snap(moving, &[], MONITOR_MAIN, Some(WORK_MAIN)),
        Some([0.0, 300.0])
    );
    // 距工作区右/下边同样贴合。
    let moving = rect(1810.0, 930.0, 1910.0, 1030.0);
    assert_eq!(
        solve_snap(moving, &[], MONITOR_MAIN, Some(WORK_MAIN)),
        Some([1820.0, 940.0])
    );
}

#[test]
fn solve_ignores_partners_on_other_monitors() {
    let moving = rect(1800.0, 0.0, 1900.0, 100.0);
    let mut alien = snap_partner(rect(1908.0, 0.0, 2008.0, 100.0));
    alien.monitor = MONITOR_SECOND;
    assert_eq!(
        solve_snap(moving, &[alien], MONITOR_MAIN, Some(WORK_MAIN)),
        None
    );
}

#[test]
fn solve_already_touching_is_no_command() {
    // 已贴合（间隙 0）：目标与现位一致，不产生落位。
    let moving = rect(0.0, 0.0, 100.0, 100.0);
    let partners = vec![snap_partner(rect(100.0, 0.0, 200.0, 100.0))];
    assert_eq!(
        solve_snap(moving, &partners, MONITOR_MAIN, Some(WORK_MAIN)),
        None
    );
}
