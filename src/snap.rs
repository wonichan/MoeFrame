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
//!    不参与，只在真能移动窗口的赢家里取一——类别先行（贴合压
//!    过对齐，跨组亦然），同类再按间隙小者、生成在先者。贴合候
//!    选要求两窗在另一轴上的投影重叠；对齐不要求投影重叠。
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
pub(crate) const MIN_MOVE: f32 = 0.5;

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
    pub(crate) fn overlap_x(&self, other: &WindowRect) -> f32 {
        self.right.min(other.right) - self.left.max(other.left)
    }

    /// 纵向投影重叠量：大于 0 才是真重叠，恰好相接为 0。
    pub(crate) fn overlap_y(&self, other: &WindowRect) -> f32 {
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

pub(crate) fn pair_in_contact(a: &WindowRect, b: &WindowRect) -> bool {
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

/// 单轴候选的类别：贴合优先于对齐——枚举声明序即优先级序
/// （`Attach < Align`），组内与跨组裁决共用这条序（审查修复
/// 工单 02：原 u8 类别常量改真枚举，跨组也按类别先行）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SnapCandidateKind {
    Attach,
    Align,
}

/// 一条单轴候选：类别、间隙绝对值、目标坐标与生成序号（同距
/// 决胜的稳定次序）。
#[derive(Clone, Copy, Debug)]
struct AxisCandidate {
    kind: SnapCandidateKind,
    gap: f32,
    target: f32,
    order: usize,
}

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
    fn consider(&mut self, on_min_edge: bool, kind: SnapCandidateKind, gap: f32, target: f32) {
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
    /// 过 MIN_MOVE），类别先行（贴合无条件压过对齐，跨组亦然——
    /// 审查修复 S6：原实现跨组只比间隙，对边的小间隙对齐会压过
    /// 本边的贴合），同类间隙小者优先、同距生成在先者优先；
    /// 都没有时保持现位。
    fn target(&self, current: f32) -> f32 {
        let mut winner: Option<AxisCandidate> = None;
        for candidate in [self.min_best, self.max_best].into_iter().flatten() {
            if (candidate.target - current).abs() <= MIN_MOVE {
                continue;
            }
            match winner {
                None => winner = Some(candidate),
                Some(chosen) => {
                    if candidate.kind < chosen.kind
                        || (candidate.kind == chosen.kind
                            && (candidate.gap < chosen.gap
                                || (candidate.gap == chosen.gap && candidate.order < chosen.order)))
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
                SnapCandidateKind::Attach,
                p.left - moving.right,
                p.left - moving.width(),
            );
            axis_x.consider(
                true,
                SnapCandidateKind::Attach,
                moving.left - p.right,
                p.right,
            );
        }
        axis_x.consider(true, SnapCandidateKind::Align, p.left - moving.left, p.left);
        axis_x.consider(
            false,
            SnapCandidateKind::Align,
            p.right - moving.right,
            p.right - moving.width(),
        );
        // 纵轴：贴合要求横向投影重叠；对齐不要求（true = 顶边组）。
        if moving.overlap_x(p) > 0.0 {
            axis_y.consider(
                false,
                SnapCandidateKind::Attach,
                p.top - moving.bottom,
                p.top - moving.height(),
            );
            axis_y.consider(
                true,
                SnapCandidateKind::Attach,
                moving.top - p.bottom,
                p.bottom,
            );
        }
        axis_y.consider(true, SnapCandidateKind::Align, p.top - moving.top, p.top);
        axis_y.consider(
            false,
            SnapCandidateKind::Align,
            p.bottom - moving.bottom,
            p.bottom - moving.height(),
        );
    }
    if let Some(area) = work_area {
        // 工作区边缘贴合（间隙归零）：在全部伙伴候选之后生成。
        axis_x.consider(
            true,
            SnapCandidateKind::Attach,
            moving.left - area.left,
            area.left,
        );
        axis_x.consider(
            false,
            SnapCandidateKind::Attach,
            area.right - moving.right,
            area.right - moving.width(),
        );
        axis_y.consider(
            true,
            SnapCandidateKind::Attach,
            moving.top - area.top,
            area.top,
        );
        axis_y.consider(
            false,
            SnapCandidateKind::Attach,
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
