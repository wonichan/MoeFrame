//! 多实例吸附专题工单 06：方向关系（属性树）、领队拖动计划、
//! 存量接触临时成树与卫生规则的纯逻辑测试。关系表以
//! 「从属 → 基座」边列表供给（宿主枚举伙伴时顺带读各窗的基座
//! 属性组装），本文件只测纯判定。
use moe_frame::snap::{PartnerFrame, WindowRect};
use moe_frame::snap_follow::AnchorEdge;
use moe_frame::snap_group::{
    anchor_edges, anchor_neighbour_ids, base_relation_valid, base_update_decision, choose_base,
    descendant_subtree, direct_contact_ids, frame_delta, parse_base_prop, plan_drag_start,
    snap_detach_available, temporary_contact_ids, SnapPartner, SnapRelation,
};

const MONITOR: WindowRect = WindowRect {
    left: 0.0,
    top: 0.0,
    right: 1920.0,
    bottom: 1080.0,
};
const MONITOR2: WindowRect = WindowRect {
    left: 1920.0,
    top: 0.0,
    right: 3840.0,
    bottom: 1080.0,
};

fn rel(subordinate: u64, base: u64) -> SnapRelation {
    SnapRelation { subordinate, base }
}

fn frame(x: f32, y: f32, w: f32, h: f32) -> PartnerFrame {
    PartnerFrame {
        rect: WindowRect::from_pos_size([x, y], [w, h]),
        monitor: MONITOR,
        work_area: MONITOR,
    }
}

fn partner(id: u64, x: f32, y: f32, base: Option<u64>) -> SnapPartner {
    SnapPartner {
        id,
        frame: frame(x, y, 200.0, 100.0),
        base,
    }
}

// ---------- 子树计算（传递闭包 + 环防御） ----------

#[test]
fn subtree_single_chain() {
    // 1 的基座是 2，2 的基座是 3：拖 3 带 2、1。
    let relations = [rel(1, 2), rel(2, 3)];
    assert_eq!(descendant_subtree(3, &relations), vec![2, 1]);
    assert_eq!(descendant_subtree(2, &relations), vec![1]);
}

#[test]
fn subtree_three_level_tree() {
    // 4 为根：2、3 是其从属，1 是 2 的从属。
    let relations = [rel(1, 2), rel(2, 4), rel(3, 4)];
    assert_eq!(descendant_subtree(4, &relations), vec![2, 3, 1]);
}

#[test]
fn subtree_one_base_many_subordinates() {
    let relations = [rel(1, 9), rel(2, 9), rel(3, 9)];
    assert_eq!(descendant_subtree(9, &relations), vec![1, 2, 3]);
}

#[test]
fn subtree_without_relations_is_empty() {
    assert!(descendant_subtree(7, &[]).is_empty());
    assert!(descendant_subtree(7, &[rel(1, 2)]).is_empty());
}

#[test]
fn subtree_cycle_is_broken_not_infinite() {
    // 异常成环 1→2→3→1：不死循环，环上其余成员各计一次，根自身不计。
    let relations = [rel(1, 2), rel(2, 3), rel(3, 1)];
    assert_eq!(descendant_subtree(1, &relations), vec![3, 2]);
}

// ---------- 拖动开始计划（PotPlayer 分离语义） ----------

#[test]
fn drag_base_carries_whole_subtree() {
    let relations = [rel(1, 2), rel(2, 3)];
    let plan = plan_drag_start(3, &relations, false, false, &[]);
    assert!(!plan.clear_own_base);
    assert_eq!(plan.carry, vec![2, 1]);
}

#[test]
fn drag_subordinate_detaches_first_and_carries_only_own_subtree() {
    // 拖链中间的 2：先清自己的基座属性（脱离 3），只带自己的从属 1；
    // 原基座 3 与其余成员不动。
    let relations = [rel(1, 2), rel(2, 3)];
    let plan = plan_drag_start(2, &relations, true, false, &[]);
    assert!(plan.clear_own_base);
    assert_eq!(plan.carry, vec![1]);
}

#[test]
fn solo_drag_clears_base_and_carries_nothing() {
    let relations = [rel(1, 2), rel(2, 3)];
    let plan = plan_drag_start(2, &relations, true, true, &[1]);
    assert!(plan.clear_own_base);
    assert!(plan.carry.is_empty());
}

#[test]
fn mid_drag_new_base_does_not_switch_leader() {
    // 拖动中吸上新基座：choose_base 取命中列表第一个（每帧记录口径），
    // 被拖窗成为其从属；但本次拖动的搬运集合是拖动开始时定死的，
    // 不因新基座改变——新基座不被搬运、原有子树仍由被拖窗带走，
    // 下一次拖基座时才按新树联动。
    use moe_frame::snap::{SnapEdge, SnapHit};
    let hits = [SnapHit {
        partner_hwnd: 9,
        edge: SnapEdge::Right,
        target: WindowRect::from_pos_size([0.0, 0.0], [10.0, 10.0]),
    }];
    assert_eq!(choose_base(&hits), Some(9));
    assert_eq!(choose_base(&[]), None);
    let plan = plan_drag_start(2, &[rel(1, 2)], false, false, &[]);
    assert_eq!(plan.carry, vec![1]);
    assert!(!plan.carry.contains(&9));
}

// ---------- 基座记录裁决（审查修复 F1/F2） ----------

fn hit(partner_hwnd: u64) -> moe_frame::snap::SnapHit {
    moe_frame::snap::SnapHit {
        partner_hwnd,
        edge: moe_frame::snap::SnapEdge::Right,
        target: WindowRect::from_pos_size([0.0, 0.0], [10.0, 10.0]),
    }
}

#[test]
fn base_update_skips_partner_inside_carry() {
    // F1：拖基座时命中已在本次搬运集合（carry）内的从属——记
    // 成基座会让属性树成环，不记录。
    assert_eq!(base_update_decision(None, &[hit(5)], &[5, 6], false), None);
}

#[test]
fn base_update_records_partner_outside_exclusions() {
    // F1：命中 carry 外的伙伴，正常记录为新基座。
    assert_eq!(
        base_update_decision(None, &[hit(9)], &[5, 6], false),
        Some(9)
    );
}

#[test]
fn base_update_never_rewrites_detached_base_within_same_drag() {
    // F2：拖从属先脱离，旧基座 3 进会话排除集；同一拖动首帧再
    // 命中 3 也不写回（否则脱离被当场撤销）。
    assert_eq!(base_update_decision(None, &[hit(3)], &[3], false), None);
    // 命中别的伙伴则正常记录。
    assert_eq!(base_update_decision(None, &[hit(9)], &[3], false), Some(9));
}

#[test]
fn base_update_solo_session_records_nothing_and_next_drag_recovers() {
    // F2：solo 会话整体禁记——命中任何新伙伴都不记录；下一拖
    // 动（新会话、空排除集、不禁记）恢复正常记录。
    assert_eq!(base_update_decision(None, &[hit(9)], &[], true), None);
    assert_eq!(base_update_decision(None, &[hit(9)], &[], false), Some(9));
}

#[test]
fn base_update_keeps_recorded_base_when_hit_repeats_or_empty() {
    // 已记录者与命中相同不重写；无命中不改关系。
    assert_eq!(base_update_decision(Some(9), &[hit(9)], &[], false), None);
    assert_eq!(base_update_decision(Some(9), &[], &[], false), None);
    assert_eq!(base_update_decision(None, &[], &[], false), None);
}

// ---------- 存量接触临时成树 ----------

#[test]
fn temporary_tree_includes_attached_contact() {
    // 本窗 (100,100) 200x100；伙伴 2 贴在本窗右边，无属性关系。
    let own = frame(100.0, 100.0, 200.0, 100.0);
    let partners = [partner(2, 300.0, 100.0, None)];
    assert_eq!(temporary_contact_ids(1, own, &partners, &[]), vec![2]);
}

#[test]
fn temporary_tree_excludes_aligned_but_not_attached() {
    // 伙伴 2 与本窗左对齐（同 x）但纵向相距 200：只是对齐，不入树。
    let own = frame(100.0, 100.0, 200.0, 100.0);
    let partners = [partner(2, 100.0, 400.0, None)];
    assert!(temporary_contact_ids(1, own, &partners, &[]).is_empty());
}

#[test]
fn temporary_tree_excludes_cross_monitor_and_related() {
    let own = frame(100.0, 100.0, 200.0, 100.0);
    let mut cross = partner(2, 300.0, 100.0, None);
    cross.frame.monitor = MONITOR2;
    // 伙伴 3 贴合但已有属性关系（是本窗的从属）：走关系树不走临时。
    let related = partner(3, 300.0, 100.0, Some(1));
    let partners = [cross, related];
    assert!(temporary_contact_ids(1, own, &partners, &[rel(3, 1)]).is_empty());
}

// ---------- 卫生规则 ----------

fn rect(x: f32, y: f32) -> WindowRect {
    WindowRect::from_pos_size([x, y], [200.0, 100.0])
}

#[test]
fn hygiene_rejects_vanished_base() {
    assert!(!base_relation_valid(
        false,
        rect(0.0, 0.0),
        rect(200.0, 0.0),
        true,
        12.0
    ));
}

#[test]
fn hygiene_rejects_gap_beyond_threshold() {
    // 基座还在、同屏，但相对边间隙 50 > 阈值 12：陈旧关系失效。
    assert!(!base_relation_valid(
        true,
        rect(0.0, 0.0),
        rect(250.0, 0.0),
        true,
        12.0
    ));
    // 间隙恰在阈值内仍有效。
    assert!(base_relation_valid(
        true,
        rect(0.0, 0.0),
        rect(210.0, 0.0),
        true,
        12.0
    ));
}

#[test]
fn hygiene_rejects_cross_monitor_base() {
    assert!(!base_relation_valid(
        true,
        rect(0.0, 0.0),
        rect(200.0, 0.0),
        false,
        12.0
    ));
}

#[test]
fn hygiene_rejects_diagonal_without_projection_overlap() {
    // 对角反例（审查修复 F4）：基座在右下方，横、纵两轴间隙
    // 各 10（都 ≤ 阈值 12），但两轴投影都无重叠——不是接触，
    // 关系无效。旧口径（两轴间隙取大者 ≤ 阈值）会误判有效。
    assert!(!base_relation_valid(
        true,
        rect(0.0, 0.0),
        rect(210.0, 110.0),
        true,
        12.0
    ));
}

// ---------- 属性值解析 / 菜单出现条件 / 帧增量 ----------

#[test]
fn base_prop_value_parsing() {
    assert_eq!(parse_base_prop(0), None);
    assert_eq!(parse_base_prop(12345), Some(12345));
}

#[test]
fn snap_detach_available_with_base_or_direct_subordinate() {
    // 审查修复 F5：出现条件是与本窗相关的属性关系——本窗有基
    // 座，或有直接从属指向本窗；存量接触（无属性关系）不触发。
    assert!(snap_detach_available(true, false));
    assert!(snap_detach_available(false, true));
    assert!(!snap_detach_available(false, false));
}

#[test]
fn frame_delta_is_target_minus_actual_top_left() {
    assert_eq!(frame_delta([140.0, 90.0], [100.0, 100.0]), [40.0, -10.0]);
    assert_eq!(frame_delta([100.0, 100.0], [100.0, 100.0]), [0.0, 0.0]);
}

// ---------- 锚边邻居（关系 + 直接接触） ----------

#[test]
fn anchor_neighbours_are_base_subordinates_and_contacts() {
    // 本窗 1：基座 3、直接从属 2、直接接触（无关系）4、无关 5。
    let relations = [rel(1, 3), rel(2, 1)];
    let neighbours = anchor_neighbour_ids(1, &relations, &[2, 4]);
    assert_eq!(neighbours, vec![3, 2, 4]);
}

#[test]
fn anchor_edges_come_from_neighbour_contacts_only() {
    // 本窗 (100,100) 200x100；邻居 2 贴右、非邻居 3 贴左：只报右边。
    let own = frame(100.0, 100.0, 200.0, 100.0);
    let partners = [
        partner(2, 300.0, 100.0, None),
        partner(3, -100.0, 100.0, None),
    ];
    let edges = anchor_edges(own, &partners, &[2]);
    assert_eq!(edges, vec![AnchorEdge::Right]);
    // 直接接触名单同样只认贴合：3 也贴合本窗，故接触名单含两者。
    assert_eq!(direct_contact_ids(own, &partners), vec![2, 3]);
}
