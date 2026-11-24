//! 吸附组的方向关系与领队拖动计划（多实例吸附专题工单 06，
//! spec 第 15 节 / ADR-0005）：吸附关系带方向——从属窗以 Win32
//! 窗口属性记其基座 HWND（属性名与读写薄层在宿主 snap_enum，
//! 名为 `MoeFrame.SnapBaseHwnd`，与工单 04 的退出请求标记
//! `MoeFrame.ExitAllRequested` 错开）；本模块只做纯判定：属性
//! 值解析、卫生校验、从属子树（传递闭包）、拖动开始计划、
//! 存量接触临时成树、锚边邻居与菜单出现条件。关系不落盘。
//!
//! 领队语义（PotPlayer）：协调权只在被拖进程手里。拖动开始
//! 时算搬运集合并定死本次拖动——拖基座带整棵子树；拖从属先
//! 清自己的基座属性脱离、只带自己的子树；Shift 独走清属性
//! 且不带任何子树。拖动中吸上新基座只改关系（被拖窗设属性
//! 成为其从属、原有子树随之并入新树），本次拖动的搬运集合
//! 不变、新基座不被搬运——领队仍是被拖窗，下一次拖基座时
//! 才按新树联动。
//!
//! 关系记录时机（工单 06 定死，审查修复 F1/F2 补排除口径）：
//! WM_MOVING 每帧求解后由 [`base_update_decision`] 裁决是否
//! 写属性——命中非空、选出的基座与会话已记录者不同、且不
//! 在排除集内才写；排除集 = 本次搬运集合 ∪ 拖动开始被清掉
//! 的基座，Shift 独走会话整体不记录。一窗至多一个基座，多
//! 命中时取命中列表第一个（求解器按横轴、纵轴顺序组装命
//! 中，第一个即本帧的首个贴合命中）；命中为空不清属性（拖
//! 离后由卫生规则清理——拖动开始与拖动中每帧记录前各校验
//! 一次，审查修复 F4）。拖动结束时关系以最后一次写入为准，
//! 留在窗上不清。

use crate::snap::{pair_in_contact, PartnerFrame, SnapHit, WindowRect, CONTACT_TOLERANCE};
use crate::snap_follow::AnchorEdge;

/// 窗口身份标识（宿主以相框窗口句柄值供给）。
pub type WindowId = u64;

/// 一条方向关系：`subordinate` 的基座是 `base`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapRelation {
    pub subordinate: WindowId,
    pub base: WindowId,
}

/// 一位带身份与基座的伙伴相框：宿主枚举时顺带读其基座属性
/// 组装，供关系表、临时成树与锚边邻居判定消费。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapPartner {
    pub id: WindowId,
    pub frame: PartnerFrame,
    /// 该伙伴的基座（其窗上属性所记）；无属性为 None。
    pub base: Option<WindowId>,
}

/// 窗口属性值解析：属性缺失时宿主薄层给 0 → 无基座；非零即
/// 基座的窗口句柄值。
pub fn parse_base_prop(raw_value: usize) -> Option<WindowId> {
    if raw_value == 0 {
        None
    } else {
        Some(raw_value as WindowId)
    }
}

/// 本帧命中里要记录的基座：一窗至多一个基座，多命中取第一
/// 个；无命中为 None（不改关系）。
pub fn choose_base(hits: &[SnapHit]) -> Option<WindowId> {
    hits.first().map(|hit| hit.partner_hwnd)
}

/// 基座记录裁决（纯函数，工单 06 审查修复 F1/F2）：本帧命中选
/// 出的基座是否要写成被拖窗的新基座属性——返回 Some(新基座)
/// 表示宿主应写属性，None 表示不变（不写）。不变的情形：
/// - 无贴合命中，或选出的基座与已记录者相同；
/// - 选出的基座在 `excluded` 内：宿主组装的排除集 = 本次搬运
///   集合（carry，我的从属 / 存量接触临时从属）∪ 会话排除集
///   （拖动开始被清掉的那个基座——拖从属先脱离 / 卫生清理，
///   整个会话内不得被首帧命中写回）。把 carry 内伙伴记成基
///   座会让属性树成环（我带着走的从属反过来成了我的基座），
///   故一律不记录；
/// - `block_all` 为真（Shift 独走会话）：整个会话不记录任何
///   基座。排除与独走都只活在本次拖动会话内，下一拖动由宿主
///   以空排除集重新裁决、恢复正常记录。
pub fn base_update_decision(
    recorded_base: Option<WindowId>,
    hits: &[SnapHit],
    excluded: &[WindowId],
    block_all: bool,
) -> Option<WindowId> {
    if block_all {
        return None;
    }
    let candidate = choose_base(hits)?;
    if recorded_base == Some(candidate) || excluded.contains(&candidate) {
        return None;
    }
    Some(candidate)
}

/// 从属子树：关系表里以 `root` 为（传递）基座的全部窗口。
/// 广度优先、按关系表出现顺序展开，结果稳定；关系异常成环
/// 时已访问者不再展开（断环处理），根自身不计入结果。
pub fn descendant_subtree(root: WindowId, relations: &[SnapRelation]) -> Vec<WindowId> {
    let mut result: Vec<WindowId> = Vec::new();
    let mut queue: Vec<WindowId> = vec![root];
    let mut visited: Vec<WindowId> = vec![root];
    while let Some(current) = queue.first().copied() {
        queue.remove(0);
        for relation in relations {
            if relation.base == current && !visited.contains(&relation.subordinate) {
                visited.push(relation.subordinate);
                result.push(relation.subordinate);
                queue.push(relation.subordinate);
            }
        }
    }
    result
}

/// 两矩形是否仍算接触（卫生口径，工单 06 审查修复 F4 收紧）：
/// 一轴的相对边间隙 ≤ 阈值、且另一轴投影真重叠。对角摆放时
/// 两轴间隙可以同时 ≤ 阈值却毫无投影重叠——那不是接触（旧
/// 口径取两轴间隙大者会误判有效），必须判无效。
fn still_in_contact(a: &WindowRect, b: &WindowRect, threshold: f32) -> bool {
    let gap_x = (a.left - b.right).max(b.left - a.right);
    let gap_y = (a.top - b.bottom).max(b.top - a.bottom);
    (gap_x <= threshold && a.overlap_y(b) > 0.0) || (gap_y <= threshold && a.overlap_x(b) > 0.0)
}

/// 卫生校验（纯判定）：基座须存活、与本窗同屏、且两者仍接
/// 触（见 [`still_in_contact`]，入参按调用路径的坐标口径：
/// 拖动路径为物理像素阈值）。任一不满足即陈旧关系、应清
/// 属性。执行时机有二：拖动开始（宿主 ENTERSIZEMOVE）与拖
/// 动中每帧记录基座前（宿主 WM_MOVING）。
pub fn base_relation_valid(
    base_alive: bool,
    own: WindowRect,
    base: WindowRect,
    same_monitor: bool,
    threshold: f32,
) -> bool {
    base_alive && same_monitor && still_in_contact(&own, &base, threshold)
}

/// 拖动开始计划：是否清本窗基座属性 + 本次拖动的搬运集合
/// （从属子树 ∪ 存量接触临时从属，均不含被拖窗自身）。
#[derive(Clone, Debug, PartialEq)]
pub struct DragStartPlan {
    pub clear_own_base: bool,
    pub carry: Vec<WindowId>,
}

/// 拖动开始裁决（纯函数，口径见文件头）：`own_has_base` 是
/// 卫生校验后的本窗基座有无；`temporary` 是存量接触临时成
/// 树的伙伴名单（调用方已剔除有关系者与本窗基座）。
pub fn plan_drag_start(
    own: WindowId,
    relations: &[SnapRelation],
    own_has_base: bool,
    solo: bool,
    temporary: &[WindowId],
) -> DragStartPlan {
    if solo {
        // Shift 独走：脱离基座、本次不带任何子树。
        return DragStartPlan {
            clear_own_base: true,
            carry: Vec::new(),
        };
    }
    let mut carry = descendant_subtree(own, relations);
    for id in temporary {
        if *id != own && !carry.contains(id) {
            carry.push(*id);
        }
    }
    DragStartPlan {
        // 拖从属先脱离：清自己的基座属性，只带自己的子树走。
        clear_own_base: own_has_base,
        carry,
    }
}

/// 与本窗直接接触（贴合）的伙伴身份：同屏 + 接触派生（工
/// 单 02 的 pair_in_contact，相对边间隙 ≈ 0 且投影重叠）。
/// 仅对齐（同名边齐平但相对边不贴）不算接触。
pub fn direct_contact_ids(own: PartnerFrame, partners: &[SnapPartner]) -> Vec<WindowId> {
    partners
        .iter()
        .filter(|partner| {
            partner.frame.monitor == own.monitor && pair_in_contact(&own.rect, &partner.frame.rect)
        })
        .map(|partner| partner.id)
        .collect()
}

/// 存量接触临时成树：与本窗直接接触、且与本窗之间无任何
/// 方向关系（既非本窗基座、亦非本窗从属）的伙伴，临时视
/// 为被拖窗的从属参与本次搬运（不写属性）。跨屏者由直接
/// 接触判定天然排除（同屏才判接触）。
pub fn temporary_contact_ids(
    own_id: WindowId,
    own: PartnerFrame,
    partners: &[SnapPartner],
    relations: &[SnapRelation],
) -> Vec<WindowId> {
    direct_contact_ids(own, partners)
        .into_iter()
        .filter(|id| {
            !relations.iter().any(|r| {
                (r.subordinate == own_id && r.base == *id)
                    || (r.subordinate == *id && r.base == own_id)
            })
        })
        .collect()
}

/// 锚边邻居名单：本窗的基座 + 本窗的直接从属 + 直接接触者
/// （去重、按此顺序）。锚边变尺寸的邻居来源（工单 06 口
/// 径，取代工单 03 的无方向连通分量）。
pub fn anchor_neighbour_ids(
    own: WindowId,
    relations: &[SnapRelation],
    direct_contacts: &[WindowId],
) -> Vec<WindowId> {
    let mut ids: Vec<WindowId> = Vec::new();
    for relation in relations {
        if relation.subordinate == own && !ids.contains(&relation.base) {
            ids.push(relation.base);
        }
    }
    for relation in relations {
        if relation.base == own && !ids.contains(&relation.subordinate) {
            ids.push(relation.subordinate);
        }
    }
    for id in direct_contacts {
        if *id != own && !ids.contains(id) {
            ids.push(*id);
        }
    }
    ids
}

/// 本窗与指定邻居之间的贴合边（锚边变尺寸用）：只有名单内
/// 邻居的接触才产生锚边；按 Left/Right/Top/Bottom 稳定输出。
/// 接触与否统一由 snap::pair_in_contact 判定（审查修复 S5，
/// 不再自带第二份接触判定）；命中后按伙伴的相对方位命名边
/// （间隙 ≈ 0 的那条相对边即贴合边）。
pub fn anchor_edges(
    own: PartnerFrame,
    partners: &[SnapPartner],
    neighbours: &[WindowId],
) -> Vec<AnchorEdge> {
    let mut edges = Vec::new();
    for partner in partners.iter().filter(|p| neighbours.contains(&p.id)) {
        if partner.frame.monitor != own.monitor {
            continue;
        }
        let p = &partner.frame.rect;
        if !pair_in_contact(&own.rect, p) {
            continue;
        }
        if (own.rect.left - p.right).abs() <= CONTACT_TOLERANCE
            && !edges.contains(&AnchorEdge::Left)
        {
            edges.push(AnchorEdge::Left);
        }
        if (own.rect.right - p.left).abs() <= CONTACT_TOLERANCE
            && !edges.contains(&AnchorEdge::Right)
        {
            edges.push(AnchorEdge::Right);
        }
        if (own.rect.top - p.bottom).abs() <= CONTACT_TOLERANCE && !edges.contains(&AnchorEdge::Top)
        {
            edges.push(AnchorEdge::Top);
        }
        if (own.rect.bottom - p.top).abs() <= CONTACT_TOLERANCE
            && !edges.contains(&AnchorEdge::Bottom)
        {
            edges.push(AnchorEdge::Bottom);
        }
    }
    edges
}

/// 菜单「脱离吸附组」的出现条件（工单 06 审查修复 F5 改定）：
/// 本窗有基座属性，或有直接从属指向本窗——即存在与本窗相
/// 关的属性关系可清。无任何属性关系的存量接触不出现该项
/// （其分离走 Shift 独走）。菜单动作双向清属性：清本窗基
/// 座属性 + 清直接从属窗上指向本窗的基座属性（宿主执行）。
pub fn snap_detach_available(has_base: bool, has_direct_subordinate: bool) -> bool {
    has_base || has_direct_subordinate
}

/// 一帧的搬运增量（物理像素）：本帧最终落位左上（吸附求解
/// 改写后的拟拖矩形左上）减本窗当前实际左上（帧首以自家
/// HWND 的 GetWindowRect 取得）。吸附跳变也包含在这个差
/// 里，故子树随跳变一起走；增量为零时宿主不调 HDWP。
pub fn frame_delta(final_top_left: [f32; 2], own_actual_top_left: [f32; 2]) -> [f32; 2] {
    [
        final_top_left[0] - own_actual_top_left[0],
        final_top_left[1] - own_actual_top_left[1],
    ]
}
