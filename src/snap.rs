//! 窗口吸附的纯逻辑（多实例吸附专题工单 02）：伙伴判定、接触派生
//! 与松手吸附求解。宿主在拖动松手瞬间用 Win32 枚举窗口、把窗口事
//! 实（标题、所属进程 exe、矩形、所在屏）喂进来，本模块只做判定
//! 与求解，不碰任何系统调用；吸附关系不落盘、不存储，任意时刻都
//! 由窗口边缘的几何接触实时派生（spec 第 7 节）。
//!
//! 坐标口径：本模块的一切矩形都是**逻辑像素**。宿主枚举拿到的
//! Win32 矩形是物理像素，按各窗口自身的 DPI 比例（GetDpiForWindow
//! ÷ 96）换算成逻辑像素后才喂进来；阈值 12 也是逻辑 px。所在屏以
//! 带原点的显示器矩形标识——同屏判定比较显示器矩形本身，不用只
//! 有尺寸的退化口径（两块同尺寸的屏会被误判成同一块）。
//!
//! 松手求解的选择规则（定死，spec 第 3 节形态 + 工单 02 定案）：
//! 横、纵两轴各自独立裁决后组合落位。单轴内先按被拖窗的边分组
//! （左边组/右边组、顶边组/底边组——作用于不同边的候选互不压
//! 制：一边已贴合不挡另一边被吸过去），组内优先级：
//! 1. 贴合（边对边、工作区边缘）优先于同名边对齐；
//! 2. 同类候选中间隙小者优先（最近优先）；
//! 3. 间隙相同者按候选生成顺序取先者——生成顺序即伙伴的枚举顺
//!    序（工作区边缘候选在全部伙伴候选之后生成，故同距时伙伴贴
//!    合优先于工作区贴合），结果稳定可复现。
//!    两组赢家再决胜：目标与现位一致的赢家（该边已贴合/已对齐）
//!    不参与，只在真能移动窗口的赢家里按间隙小者、生成在先者
//!    取一。贴合候选要求两窗在另一轴上的投影重叠；对齐不要求
//!    投影重叠。
//!
//! 与工单 06 的 6px 钳制保留距离互不引用：那是切图溢出平移时窗
//! 口与工作区边缘保留的距离；这里的工作区贴合是主动贴到边缘本
//! 身、间隙归零。两条路径不互相改口径。

/// 相框窗口的固定标题：实例发现的第一层过滤（spec 第 9 节，相框
/// 标题不加编号后缀）。
pub const FRAME_WINDOW_TITLE: &str = "MoeFrame";

/// 吸附阈值（逻辑 px，spec 第 3 节定案）：相对边或同名边距离不超
/// 过它才产生吸附目标。固定值，不做设置项。
pub const SNAP_THRESHOLD: f32 = 12.0;

/// 接触派生的间隙容差（逻辑 px）：相对边间隙的绝对值不超过它判
/// 贴合。它只回答「现在贴没贴」，与吸附阈值是两回事——阈值回答
/// 「松手时靠得够不够近、要不要吸过去」。
pub const CONTACT_TOLERANCE: f32 = 1.0;

/// 落位生效的最小位移（逻辑 px）：求解目标与现位之差不超过它视
/// 为没有移动（已贴合/已对齐不重复发命令）。
const MIN_MOVE: f32 = 0.5;

/// 一扇窗口的矩形（逻辑像素，屏幕坐标，含原点）：左上与右下。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl WindowRect {
    /// 由左上位置与尺寸构造。
    pub fn from_pos_size(pos: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            left: pos[0],
            top: pos[1],
            right: pos[0] + size[0],
            bottom: pos[1] + size[1],
        }
    }

    /// 由 [left, top, right, bottom] 构造（与几何机的工作区口径同形）。
    pub fn from_edges(edges: [f32; 4]) -> Self {
        Self {
            left: edges[0],
            top: edges[1],
            right: edges[2],
            bottom: edges[3],
        }
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// 左上位置。
    pub fn pos(&self) -> [f32; 2] {
        [self.left, self.top]
    }

    /// 横向投影重叠量：大于 0 才是真重叠，恰好相接为 0。
    fn overlap_x(&self, other: &WindowRect) -> f32 {
        self.right.min(other.right) - self.left.max(other.left)
    }

    /// 纵向投影重叠量：大于 0 才是真重叠，恰好相接为 0。
    fn overlap_y(&self, other: &WindowRect) -> f32 {
        self.bottom.min(other.bottom) - self.top.max(other.top)
    }
}

/// 宿主枚举到的一扇候选窗口的全部事实：判定需要的字段都可注入，
/// 测试以假事实复现「同标题异进程」「设置窗混入」等场景。
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateWindow {
    /// 窗口标题原文。
    pub title: String,
    /// 窗口所属进程的 exe 完整路径；取不到时为 None（不入列）。
    pub exe_path: Option<String>,
    /// 所属进程是否存活（宿主经 instance 的存活接缝供给）：已退出
    /// 实例的残留窗口不入列。
    pub owner_alive: bool,
    /// 是否就是本进程自己的相框窗口（宿主按句柄比对）。
    pub is_self_window: bool,
    /// 窗口矩形（逻辑像素）。
    pub rect: WindowRect,
    /// 所在显示器的矩形（逻辑像素，带原点）：同屏判定的依据。
    pub monitor: WindowRect,
    /// 所在显示器的工作区矩形（逻辑像素，带原点）。
    pub work_area: WindowRect,
}

/// 伙伴判定（两层过滤，工单 02 定案）：标题恰为相框标题（设置窗
/// 标题不同，第一层挡住）、所属进程 exe 与自身一致（第二层挡住
/// 其他程序的同名窗口）、所属进程存活、且不是自己这扇窗。
pub fn is_partner_frame(candidate: &CandidateWindow, own_exe: &str) -> bool {
    candidate.title == FRAME_WINDOW_TITLE
        && !candidate.is_self_window
        && candidate.owner_alive
        && candidate
            .exe_path
            .as_deref()
            .is_some_and(|exe| same_exe_path(exe, own_exe))
}

/// exe 路径同判：Windows 路径大小写不敏感、分隔符两种写法都合法，
/// 归一化后比较，避免同一 exe 因写法不同被误挡。
fn same_exe_path(a: &str, b: &str) -> bool {
    fn normalize(path: &str) -> String {
        path.replace('/', "\\").to_lowercase()
    }
    normalize(a) == normalize(b)
}

/// 一扇已确认的伙伴相框：矩形与所在屏事实，供接触派生与工单 03
/// 的组联动跟随复用。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartnerFrame {
    pub rect: WindowRect,
    pub monitor: WindowRect,
    pub work_area: WindowRect,
}

/// 从枚举事实里筛出全部伙伴相框（保持枚举顺序：求解的同距决胜
/// 按这个顺序取先者）。
pub fn partner_frames(candidates: &[CandidateWindow], own_exe: &str) -> Vec<PartnerFrame> {
    candidates
        .iter()
        .filter(|candidate| is_partner_frame(candidate, own_exe))
        .map(|candidate| PartnerFrame {
            rect: candidate.rect,
            monitor: candidate.monitor,
            work_area: candidate.work_area,
        })
        .collect()
}

/// 一条贴合边：`frames[a]` 与 `frames[b]` 有一边贴合（下标 a < b）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contact {
    pub a: usize,
    pub b: usize,
}

/// 接触派生（spec 第 7 节）：两扇窗同屏、相对边间隙 ≈ 0（容差
/// [`CONTACT_TOLERANCE`]）、且两窗在该边的法线方向上投影重叠，
/// 即视为该边贴合。跨屏不判；恰好相接但投影错开（角对角）不判。
pub fn derive_contacts(frames: &[PartnerFrame]) -> Vec<Contact> {
    let mut contacts = Vec::new();
    for a in 0..frames.len() {
        for b in (a + 1)..frames.len() {
            if frames[a].monitor != frames[b].monitor {
                continue;
            }
            if pair_in_contact(&frames[a].rect, &frames[b].rect) {
                contacts.push(Contact { a, b });
            }
        }
    }
    contacts
}

fn pair_in_contact(a: &WindowRect, b: &WindowRect) -> bool {
    let vertical_touch = (a.right - b.left).abs() <= CONTACT_TOLERANCE
        || (a.left - b.right).abs() <= CONTACT_TOLERANCE;
    let horizontal_touch = (a.bottom - b.top).abs() <= CONTACT_TOLERANCE
        || (a.top - b.bottom).abs() <= CONTACT_TOLERANCE;
    (vertical_touch && a.overlap_y(b) > 0.0) || (horizontal_touch && a.overlap_x(b) > 0.0)
}

/// 吸附组派生：贴合边连成的连通分量（含无接触的单窗分量），组内
/// 与组间都按下标稳定排序。组关系只由此实时派生，不落盘。
pub fn snap_groups(frames: &[PartnerFrame]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..frames.len()).collect();
    fn find(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    for contact in derive_contacts(frames) {
        let (ra, rb) = (find(&mut parent, contact.a), find(&mut parent, contact.b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    // 按分量根归组：成员按下标升序进入（组内天然有序），分量按其
    // 最小成员的出现顺序排列（组间稳定有序）。
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut group_roots: Vec<usize> = Vec::new();
    for index in 0..frames.len() {
        let root = find(&mut parent, index);
        match group_roots.iter().position(|known| *known == root) {
            Some(slot) => groups[slot].push(index),
            None => {
                group_roots.push(root);
                groups.push(vec![index]);
            }
        }
    }
    groups
}

/// 松手吸附的一位伙伴：矩形与所在屏（求解只需要这两项）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapPartner {
    pub rect: WindowRect,
    pub monitor: WindowRect,
}

/// 几何机松手帧的吸附请求：宿主只在拖动松手那一帧组装并随观察
/// 递交（开关关闭时宿主不组装，几何机也无从计算）。`own_monitor`
/// 是被拖窗所在显示器的矩形（带原点），同屏判定以它为准。
#[derive(Clone, Debug, PartialEq)]
pub struct SnapRequest {
    pub partners: Vec<SnapPartner>,
    pub own_monitor: WindowRect,
}

/// 一条单轴候选：类别（贴合 0 / 对齐 1）、间隙绝对值、目标坐标
/// 与生成序号（同距决胜的稳定次序）。
#[derive(Clone, Copy, Debug)]
struct AxisCandidate {
    kind: u8,
    gap: f32,
    target: f32,
    order: usize,
}

const KIND_ATTACH: u8 = 0;
const KIND_ALIGN: u8 = 1;

/// 单轴求解器：按被拖窗的两条边分两组收录候选（见文件头的选择
/// 规则），最后在两组赢家里裁决出本轴目标。
#[derive(Default)]
struct AxisSolver {
    min_best: Option<AxisCandidate>,
    max_best: Option<AxisCandidate>,
    order: usize,
}

impl AxisSolver {
    /// 收录一条作用于 min 边（左/顶）或 max 边（右/底）的候选：
    /// 间隙在阈值内才收；组内比较类别先行、间隙次之、同距保留先
    /// 生成的（稳定决胜，不替换）。
    fn consider(&mut self, on_min_edge: bool, kind: u8, gap: f32, target: f32) {
        let order = self.order;
        self.order += 1;
        if gap.abs() > SNAP_THRESHOLD {
            return;
        }
        let candidate = AxisCandidate {
            kind,
            gap: gap.abs(),
            target,
            order,
        };
        let best = if on_min_edge {
            &mut self.min_best
        } else {
            &mut self.max_best
        };
        match best {
            None => *best = Some(candidate),
            Some(current) => {
                if candidate.kind < current.kind
                    || (candidate.kind == current.kind && candidate.gap < current.gap)
                {
                    *best = Some(candidate);
                }
            }
        }
    }

    /// 本轴目标：两组赢家里只取真能移动窗口的（目标与现位之差超
    /// 过 MIN_MOVE），间隙小者优先、同距生成在先者优先；都没有
    /// 时保持现位。
    fn target(&self, current: f32) -> f32 {
        let mut winner: Option<AxisCandidate> = None;
        for candidate in [self.min_best, self.max_best].into_iter().flatten() {
            if (candidate.target - current).abs() <= MIN_MOVE {
                continue;
            }
            match winner {
                None => winner = Some(candidate),
                Some(chosen) => {
                    if candidate.gap < chosen.gap
                        || (candidate.gap == chosen.gap && candidate.order < chosen.order)
                    {
                        winner = Some(candidate);
                    }
                }
            }
        }
        winner.map_or(current, |candidate| candidate.target)
    }
}

/// 松手吸附求解（spec 第 3、5 节）：对被拖窗当前矩形，按文件头定
/// 死的选择规则逐轴裁决，返回目标左上位置；两轴都没有生效候选
/// （或目标与现位一致）时返回 None，调用方不发落位命令。
pub fn solve_snap(
    moving: WindowRect,
    partners: &[SnapPartner],
    own_monitor: WindowRect,
    work_area: Option<WindowRect>,
) -> Option<[f32; 2]> {
    let mut axis_x = AxisSolver::default();
    let mut axis_y = AxisSolver::default();
    for partner in partners {
        if partner.monitor != own_monitor {
            continue;
        }
        let p = &partner.rect;
        // 横轴：贴合要求纵向投影重叠；对齐不要求。候选按被拖窗的
        // 边分组（true = 左边组，false = 右边组）。
        if moving.overlap_y(p) > 0.0 {
            axis_x.consider(
                false,
                KIND_ATTACH,
                p.left - moving.right,
                p.left - moving.width(),
            );
            axis_x.consider(true, KIND_ATTACH, moving.left - p.right, p.right);
        }
        axis_x.consider(true, KIND_ALIGN, p.left - moving.left, p.left);
        axis_x.consider(
            false,
            KIND_ALIGN,
            p.right - moving.right,
            p.right - moving.width(),
        );
        // 纵轴：贴合要求横向投影重叠；对齐不要求（true = 顶边组）。
        if moving.overlap_x(p) > 0.0 {
            axis_y.consider(
                false,
                KIND_ATTACH,
                p.top - moving.bottom,
                p.top - moving.height(),
            );
            axis_y.consider(true, KIND_ATTACH, moving.top - p.bottom, p.bottom);
        }
        axis_y.consider(true, KIND_ALIGN, p.top - moving.top, p.top);
        axis_y.consider(
            false,
            KIND_ALIGN,
            p.bottom - moving.bottom,
            p.bottom - moving.height(),
        );
    }
    if let Some(area) = work_area {
        // 工作区边缘贴合（间隙归零）：在全部伙伴候选之后生成。
        axis_x.consider(true, KIND_ATTACH, moving.left - area.left, area.left);
        axis_x.consider(
            false,
            KIND_ATTACH,
            area.right - moving.right,
            area.right - moving.width(),
        );
        axis_y.consider(true, KIND_ATTACH, moving.top - area.top, area.top);
        axis_y.consider(
            false,
            KIND_ATTACH,
            area.bottom - moving.bottom,
            area.bottom - moving.height(),
        );
    }
    let target_x = axis_x.target(moving.left);
    let target_y = axis_y.target(moving.top);
    if (target_x - moving.left).abs() > MIN_MOVE || (target_y - moving.top).abs() > MIN_MOVE {
        Some([target_x, target_y])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 工单 03：组联动跟随、抑制标记、轮询调度与锚边变尺寸（纯逻辑）。
//
// 跟随是「各自算各自的」：每个进程只掌握自己的窗口、枚举到的伙伴
// 矩形与接触派生，没有中心协调者（ADR-0004）。防振荡规则定死：
// 1. 只跟随「自上次轮询以来矩形确有变化」的组员；组员静止不发命令。
// 2. 本窗位置变化若与引擎上一条跟随命令的目标吻合，是命令的落地
//    回声，不算外部移动；伙伴位移与本窗这段回声位移相同者，是它
//    在跟随本窗的回声，本轮不反向牵引（回声只消费一轮）。
// 3. 本窗被外部移动（用户拖动/别家命令）时本轮不跟随，只更新
//    历史——由伙伴进程各自来跟本窗，避免双向同时跟随打架。
// 4. 同组多个组员同轮移动时，位移一致才跟随；不一致定死不动，
//    避免本窗被撕扯到任一边。
// 5. 组关系按「上次轮询时的接触图」判定移动资格：组员刚被拖走
//    的那一轮，它在旧图里仍是组员，本窗跟随到自身工作区边缘停
//    住（跨屏脱组口径）；下一轮起按新图自然脱组。
// ---------------------------------------------------------------------------

/// 伙伴的窗口身份标识（宿主以相框窗口句柄的值供给）：抑制标记
/// 按它键控，同一扇伙伴窗在多次枚举间保持同一标识。
pub type PartnerId = u64;

/// 一位带身份的伙伴相框：工单 02 的伙伴事实 + 窗口身份标识。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FollowPartner {
    pub id: PartnerId,
    pub frame: PartnerFrame,
}

/// 两扇窗的分离量：横、纵两轴各自的间隙（重叠为负），取大者——
/// 任一轴分开超过阈值，两扇窗在几何上就已真正分开。
fn separation(a: &WindowRect, b: &WindowRect) -> f32 {
    let dx = (a.left - b.right).max(b.left - a.right);
    let dy = (a.top - b.bottom).max(b.top - a.bottom);
    dx.max(dy)
}

fn pos_changed(a: &WindowRect, b: &WindowRect) -> bool {
    (a.left - b.left).abs() > MIN_MOVE || (a.top - b.top).abs() > MIN_MOVE
}

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

/// 跟随目标的屏内钳制（spec 第 8 节）：目标位置钳进本窗所在屏的
/// 工作区，窗口比工作区还大时钉到工作区左上。只钳位置不改尺寸。
fn clamp_follow_pos(pos: [f32; 2], size: [f32; 2], area: WindowRect) -> [f32; 2] {
    let max_x = (area.right - size[0]).max(area.left);
    let max_y = (area.bottom - size[1]).max(area.top);
    [
        pos[0].clamp(area.left, max_x),
        pos[1].clamp(area.top, max_y),
    ]
}

/// 引擎上一条跟随命令的记账：目标位置（回声识别用）。
#[derive(Clone, Copy, Debug)]
struct LastCommand {
    target: [f32; 2],
}

/// 组联动跟随引擎 + 抑制标记状态机（工单 03）：宿主按轮询调度
/// 喂入（本窗事实、伙伴列表、开关、自身拖动态），引擎返回本窗
/// 应移动到的左上位置（None = 本轮不发命令）。抑制只存本进程
/// 内存、不落盘，只影响联动跟随与成组派生。
#[derive(Default)]
pub struct FollowEngine {
    /// 上次轮询的本窗矩形（移动判定与组图的历史基准）。
    prev_own: Option<WindowRect>,
    /// 上次轮询的伙伴矩形（按身份键控）：不在表中的伙伴是新出现
    /// 的，无历史可比、不算移动；消失的伙伴随表重建自然清除。
    prev_partners: Vec<(PartnerId, WindowRect)>,
    last_command: Option<LastCommand>,
    /// 菜单脱离的抑制集：伙伴身份 → 立标记时的本窗矩形无关，只记
    /// 身份；失效判定每轮按当前几何算分离量。
    suppressed: Vec<PartnerId>,
}

impl FollowEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// 整体重置（开关关闭时宿主调用）：历史、回声记账与抑制全清。
    pub fn reset(&mut self) {
        self.prev_own = None;
        self.prev_partners.clear();
        self.last_command = None;
        self.suppressed.clear();
    }

    pub fn is_suppressed(&self, id: PartnerId) -> bool {
        self.suppressed.contains(&id)
    }

    /// 与本窗直接接触且未被抑制的伙伴身份（菜单「脱离吸附组」
    /// 的出现条件与锚边计算共用这份派生）。
    pub fn direct_contact_ids(
        &self,
        own: PartnerFrame,
        partners: &[FollowPartner],
    ) -> Vec<PartnerId> {
        partners
            .iter()
            .filter(|partner| {
                partner.frame.monitor == own.monitor
                    && !self.is_suppressed(partner.id)
                    && pair_in_contact(&own.rect, &partner.frame.rect)
            })
            .map(|partner| partner.id)
            .collect()
    }

    /// 本窗当前全部贴合边（未被抑制的直接接触伙伴所共享的边，
    /// 按 Left/Right/Top/Bottom 的枚举顺序稳定输出）。
    pub fn anchor_edges(&self, own: PartnerFrame, partners: &[FollowPartner]) -> Vec<AnchorEdge> {
        let contacts = self.direct_contact_ids(own, partners);
        let mut edges = Vec::new();
        for partner in partners.iter().filter(|p| contacts.contains(&p.id)) {
            let p = &partner.frame.rect;
            if (own.rect.left - p.right).abs() <= CONTACT_TOLERANCE
                && own.rect.overlap_y(p) > 0.0
                && !edges.contains(&AnchorEdge::Left)
            {
                edges.push(AnchorEdge::Left);
            }
            if (own.rect.right - p.left).abs() <= CONTACT_TOLERANCE
                && own.rect.overlap_y(p) > 0.0
                && !edges.contains(&AnchorEdge::Right)
            {
                edges.push(AnchorEdge::Right);
            }
            if (own.rect.top - p.bottom).abs() <= CONTACT_TOLERANCE
                && own.rect.overlap_x(p) > 0.0
                && !edges.contains(&AnchorEdge::Top)
            {
                edges.push(AnchorEdge::Top);
            }
            if (own.rect.bottom - p.top).abs() <= CONTACT_TOLERANCE
                && own.rect.overlap_x(p) > 0.0
                && !edges.contains(&AnchorEdge::Bottom)
            {
                edges.push(AnchorEdge::Bottom);
            }
        }
        edges
    }

    /// 菜单脱离：对当时所有直接接触的伙伴各立一条抑制，返回立标
    /// 记的伙伴身份。位置不变；抑制持续到两者真正分开超过吸附
    /// 阈值（接触断开）后自动失效，之后重新贴合可正常成组。
    pub fn detach(&mut self, own: PartnerFrame, partners: &[FollowPartner]) -> Vec<PartnerId> {
        let contacts = self.direct_contact_ids(own, partners);
        for id in &contacts {
            if !self.suppressed.contains(id) {
                self.suppressed.push(*id);
            }
        }
        contacts
    }

    /// 按身份直接立抑制（宿主以最近一轮派生的接触名单脱离时用，
    /// 与 detach 同语义）。
    pub fn detach_ids(&mut self, ids: &[PartnerId]) {
        for id in ids {
            if !self.suppressed.contains(id) {
                self.suppressed.push(*id);
            }
        }
    }

    /// 一轮轮询：返回本窗应移动到的左上位置，None 表示不发命令。
    /// `enabled` 为吸附开关（关闭时整体停摆并清历史，重新打开后
    /// 第一轮只建历史）；`own_dragging` 为本窗正被拖动（原生手势
    /// 进行中，只记历史不跟随）。
    pub fn poll(
        &mut self,
        own: PartnerFrame,
        partners: &[FollowPartner],
        enabled: bool,
        own_dragging: bool,
    ) -> Option<[f32; 2]> {
        let result = self.poll_inner(own, partners, enabled, own_dragging);
        // 抑制失效在本轮裁决之后才生效：被抑制的伙伴已不在场，
        // 或与本窗真正分开超过吸附阈值，标记自动清除——造成分
        // 开的那次移动本身仍被抑制挡住，下一轮起重新贴合可正常
        // 成组。
        self.suppressed.retain(|id| {
            partners.iter().find(|p| p.id == *id).is_some_and(|p| {
                p.frame.monitor == own.monitor
                    && separation(&own.rect, &p.frame.rect) <= SNAP_THRESHOLD
            })
        });
        result
    }

    fn poll_inner(
        &mut self,
        own: PartnerFrame,
        partners: &[FollowPartner],
        enabled: bool,
        own_dragging: bool,
    ) -> Option<[f32; 2]> {
        if !enabled {
            self.prev_own = None;
            self.prev_partners.clear();
            self.last_command = None;
            return None;
        }
        let prev_own = self.prev_own;
        let prev_partners = std::mem::take(&mut self.prev_partners);
        // 历史先行推进：无论本轮是否发命令，下轮的比较基准都是
        // 本轮所见几何（本窗被命令移动时，基准仍记命令前的矩形，
        // 下轮才能量出命令的实际落地量做回声识别）。
        self.prev_own = Some(own.rect);
        self.prev_partners = partners.iter().map(|p| (p.id, p.frame.rect)).collect();
        let prev_own = prev_own?;
        if own_dragging {
            return None;
        }
        // 本窗自身的移动：与上一条跟随命令目标吻合的是回声落地，
        // 否则是外部移动——本轮不跟随（防振荡规则 3）。
        let own_delta = [own.rect.left - prev_own.left, own.rect.top - prev_own.top];
        let own_moved = own_delta[0].abs() > MIN_MOVE || own_delta[1].abs() > MIN_MOVE;
        let echo_target = self.last_command.take().map(|command| command.target);
        let own_is_echo = own_moved
            && echo_target.is_some_and(|target| {
                (own.rect.left - target[0]).abs() <= CONTACT_TOLERANCE
                    && (own.rect.top - target[1]).abs() <= CONTACT_TOLERANCE
            });
        if own_moved && !own_is_echo {
            return None;
        }
        // 组图按上次轮询的几何派生（规则 5）：节点 0 是本窗，其余
        // 是上轮在场的伙伴；抑制边视为不存在。
        let mut frames: Vec<PartnerFrame> = vec![PartnerFrame {
            rect: prev_own,
            ..own
        }];
        let mut ids: Vec<PartnerId> = vec![0];
        for (id, rect) in &prev_partners {
            if self.is_suppressed(*id) {
                continue;
            }
            if let Some(partner) = partners.iter().find(|p| p.id == *id) {
                frames.push(PartnerFrame {
                    rect: *rect,
                    ..partner.frame
                });
                ids.push(*id);
            }
        }
        let groups = snap_groups(&frames);
        let own_group = groups.iter().find(|group| group.contains(&0))?;
        // 组员移动判定：与上轮矩形比，位置确有变化者才是移动者；
        // 与本窗回声位移相同者是跟随本窗的回声，剔除（规则 2）。
        let mut deltas: Vec<[f32; 2]> = Vec::new();
        for index in own_group.iter().filter(|index| **index != 0) {
            let id = ids[*index];
            let current = partners.iter().find(|p| p.id == id)?.frame.rect;
            let previous = frames[*index].rect;
            if !pos_changed(&previous, &current) {
                continue;
            }
            let delta = [current.left - previous.left, current.top - previous.top];
            if own_is_echo
                && (delta[0] - own_delta[0]).abs() <= CONTACT_TOLERANCE
                && (delta[1] - own_delta[1]).abs() <= CONTACT_TOLERANCE
            {
                continue;
            }
            deltas.push(delta);
        }
        let first = *deltas.first()?;
        // 多移动者位移不一致：定死不动（规则 4）。
        if deltas
            .iter()
            .any(|d| (d[0] - first[0]).abs() > MIN_MOVE || (d[1] - first[1]).abs() > MIN_MOVE)
        {
            return None;
        }
        let target = clamp_follow_pos(
            [own.rect.left + first[0], own.rect.top + first[1]],
            [own.rect.width(), own.rect.height()],
            own.work_area,
        );
        if (target[0] - own.rect.left).abs() <= MIN_MOVE
            && (target[1] - own.rect.top).abs() <= MIN_MOVE
        {
            return None;
        }
        self.last_command = Some(LastCommand { target });
        Some(target)
    }
}

/// 轮询的一轮结果（宿主枚举后回告调度机）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollOutcome {
    /// 枚举不到任何伙伴。
    NoPartners,
    /// 有伙伴，但没有任何移动。
    Quiet,
    /// 检测到移动（组员移动或本轮发出了跟随命令）。
    Movement,
}

/// 轮询所处相位：休眠（零唤醒）/ 探活（重新武装后的一次枚举）/
/// 心跳（低频检查）/ 跟随（约一帧量级）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollPhase {
    Dormant,
    Probing,
    Heartbeat,
    Active,
}

/// 心跳间隔（毫秒）：有伙伴但全静止时的低频检查节奏。
pub const HEARTBEAT_MS: u64 = 1_000;
/// 跟随节奏（毫秒）：检测到移动后的轮询间隔，约一帧量级。
pub const ACTIVE_MS: u64 = 16;
/// 跟随相位连续静默多少轮后回落到心跳。
const ACTIVE_QUIET_POLLS: u32 = 30;

/// 轮询调度状态机（工单 03 轮询成本口径）：无伙伴即休眠——宿主
/// 帧尾唤醒裁决在休眠时得不到任何延时，不产生周期性唤醒；重新
/// 武装靠本窗自身事件（启动、拖动开始/结束、开关打开）调 wake。
/// 有伙伴全静止时只有心跳级延时；检测到移动才进入跟随节奏。
#[derive(Clone, Debug)]
pub struct PollSchedule {
    phase: PollPhase,
    next_due_ms: u64,
    quiet_polls: u32,
}

impl Default for PollSchedule {
    fn default() -> Self {
        Self::new()
    }
}

impl PollSchedule {
    /// 启动即探活：新进程先枚举一次确认有没有伙伴。
    pub fn new() -> Self {
        Self {
            phase: PollPhase::Probing,
            next_due_ms: 0,
            quiet_polls: 0,
        }
    }

    pub fn phase(&self) -> PollPhase {
        self.phase
    }

    /// 本相位下宿主帧尾应请求的唤醒延时：休眠为 None（零唤醒）。
    pub fn delay_ms(&self) -> Option<u64> {
        match self.phase {
            PollPhase::Dormant => None,
            PollPhase::Probing => Some(0),
            PollPhase::Heartbeat => Some(HEARTBEAT_MS),
            PollPhase::Active => Some(ACTIVE_MS),
        }
    }

    /// 重新武装：休眠时回到探活、立即到期；其余相位不受影响。
    pub fn wake(&mut self) {
        if self.phase == PollPhase::Dormant {
            self.phase = PollPhase::Probing;
            self.next_due_ms = 0;
        }
    }

    /// 进入休眠（开关关闭时宿主调用）。
    pub fn sleep(&mut self) {
        self.phase = PollPhase::Dormant;
        self.quiet_polls = 0;
    }

    /// 当前时刻是否到期该轮询（休眠永不到期）。
    pub fn poll_due(&self, now_ms: u64) -> bool {
        self.phase != PollPhase::Dormant && now_ms >= self.next_due_ms
    }

    /// 回告一轮结果并推进相位与下次到期时刻。
    pub fn report(&mut self, outcome: PollOutcome, now_ms: u64) {
        match outcome {
            PollOutcome::NoPartners => {
                self.phase = PollPhase::Dormant;
                self.quiet_polls = 0;
            }
            PollOutcome::Movement => {
                self.phase = PollPhase::Active;
                self.quiet_polls = 0;
            }
            PollOutcome::Quiet => match self.phase {
                PollPhase::Probing | PollPhase::Heartbeat => {
                    self.phase = PollPhase::Heartbeat;
                    self.quiet_polls = 0;
                }
                PollPhase::Active => {
                    self.quiet_polls += 1;
                    if self.quiet_polls >= ACTIVE_QUIET_POLLS {
                        self.phase = PollPhase::Heartbeat;
                        self.quiet_polls = 0;
                    }
                }
                PollPhase::Dormant => {}
            },
        }
        self.next_due_ms = now_ms + self.delay_ms().unwrap_or(0);
    }
}
