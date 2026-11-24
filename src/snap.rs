//! 窗口吸附的纯逻辑（多实例吸附专题工单 02 / 05）：伙伴判定、
//! 接触派生与拖动中吸附求解。宿主把窗口事实（标题、所属进程
//! exe、矩形、所在屏）喂进来，本模块只做判定与求解，不碰任何系
//! 统调用。接触派生仍是几何事实（存量接触临时成树与锚边邻居
//! 判定用）；带方向的吸附关系自工单 06 起记在从属窗的窗口属
//! 性上，纯判定在 snap_group（spec 第 15 节）。
//!
//! 坐标口径：伙伴判定与接触派生用**逻辑像素**（宿主枚举拿到的
//! Win32 矩形按各窗口自身的 DPI 比例换算后才喂进来）；拖动中求
//! 解（工单 05）用**物理像素**——拟拖矩形、伙伴矩形与工作区同源
//! 于 Win32 物理坐标，阈值由逻辑 12px 按本窗 DPI 换算（见
//! [`physical_snap_threshold`]）。[`WindowRect`] 只是数值容器，
//! 单位由调用路径决定。所在屏以带原点的显示器矩形标识——同屏判
//! 定比较显示器矩形本身，不用只有尺寸的退化口径（两块同尺寸的
//! 屏会被误判成同一块）。
//!
//! 拖动中求解的评分规则（定死，spec 第 15 节 + 工单 05）：横、
//! 纵两轴各自独立裁决后组合落位（两轴可吸向不同伙伴）。单轴内
//! 全部候选（伙伴贴合、伙伴同名边对齐、工作区边缘贴合）按同一
//! 把尺子比成本，成本小者赢、同成本按生成顺序取先者（伙伴候选
//! 先于工作区候选生成）：
//! - 贴合成本 = 间隙绝对值 + (1 − 另一轴投影重叠比例) × 阈值。
//!   仅擦边投影的伙伴成本被重叠项抬高，压不过正对的伙伴；
//! - 对齐成本 = 间隙绝对值 + 另一轴相距距离 + 阈值 ÷ 2（类别罚
//!   分：同等邻近程度下贴合优先于对齐，但明显更近的对齐可以赢
//!   过更远的贴合——工作区贴合也不凭类别压过更近的伙伴）；
//! - 对齐另加另一轴邻近约束：另一轴投影不重叠且相距超过阈值
//!   时不产生对齐候选；贴合要求另一轴投影真重叠。
//!
//! 求解是当前拟拖矩形的纯函数、无状态：同一拖动内反复进出阈值
//! 结果只由当前矩形决定，已在目标位时目标与现位一致（幂等），
//! 不存在黏滞或漂移。
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

/// 一扇已确认的伙伴相框：矩形与所在屏事实，供接触判定（存量
/// 接触临时成树与锚边邻居派生）复用。
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

/// 两窗接触判定（spec 第 7 节的接触几何）：相对边间隙 ≈ 0
/// （容差 [`CONTACT_TOLERANCE`]）、且两窗在该边的法线方向上
/// 投影重叠，即视为贴合。恰好相接但投影错开（角对角）不判。
/// 同屏与否由调用方先行判定（本函数只看矩形）。存量接触临时
/// 成树、锚边邻居与卫生校验的接触口径都收敛到这一份判定。
pub(crate) fn pair_in_contact(a: &WindowRect, b: &WindowRect) -> bool {
    let vertical_touch = (a.right - b.left).abs() <= CONTACT_TOLERANCE
        || (a.left - b.right).abs() <= CONTACT_TOLERANCE;
    let horizontal_touch = (a.bottom - b.top).abs() <= CONTACT_TOLERANCE
        || (a.top - b.bottom).abs() <= CONTACT_TOLERANCE;
    (vertical_touch && a.overlap_y(b) > 0.0) || (horizontal_touch && a.overlap_x(b) > 0.0)
}

/// 拖动中吸附的一位伙伴（工单 05）：窗口句柄值与物理像素矩形。
/// 句柄只作身份标识随求解结果透传给工单 06 的接缝，本模块不解
/// 引用它。伙伴自身不带显示器矩形：同屏过滤由宿主每帧以拟拖
/// 矩形所在显示器调用 [`drag_partners_on_monitor`] 完成（伙伴
/// 的显示器事实由宿主在拖动开始时与伙伴成对保存）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragSnapPartner {
    pub hwnd: u64,
    pub rect: WindowRect,
}

/// 同屏过滤（纯判定，工单 05 审查修复 F3）：伙伴与其所在显示
/// 器矩形（物理像素）成对供给，只保留显示器矩形与 `monitor`
/// 相等者（保持原顺序）。宿主在 WM_MOVING 每帧以拟拖矩形所在
/// 显示器调用——跨屏伙伴不进求解，拖过屏界后按新屏的伙伴求解。
pub fn drag_partners_on_monitor(
    partners: &[(DragSnapPartner, WindowRect)],
    monitor: WindowRect,
) -> Vec<DragSnapPartner> {
    partners
        .iter()
        .filter(|(_, partner_monitor)| *partner_monitor == monitor)
        .map(|(partner, _)| *partner)
        .collect()
}

/// 被拖窗的哪条边贴合到伙伴（工单 05 给工单 06 的接缝词汇）：
/// `Right` = 被拖窗右边贴到伙伴左边，余类推。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapEdge {
    Left,
    Right,
    Top,
    Bottom,
}

/// 一次贴合命中（工单 05 给工单 06 的接缝）：命中伙伴、贴合边与
/// 求解给出的目标矩形（物理像素）。工单 06 据此在拖动会话里记
/// 录关系、搬运从属子树；本票只产出与保存，不设窗口属性、不搬
/// 伙伴。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapHit {
    pub partner_hwnd: u64,
    pub edge: SnapEdge,
    pub target: WindowRect,
}

/// 拖动中求解的结论：目标矩形（物理像素）+ 全部贴合命中（两轴
/// 可各命中一位伙伴）。对齐与工作区贴合不产出 [`SnapHit`]。
#[derive(Clone, Debug, PartialEq)]
pub struct DragSnapOutcome {
    pub target: WindowRect,
    pub hits: Vec<SnapHit>,
}

/// 逻辑阈值 → 物理阈值（工单 05）：12 逻辑 px 按本窗 DPI 换算，
/// `dpi` 为 GetDpiForWindow 的返回值（96 = 100%）；取不到（0）时
/// 退回逻辑值本身。
pub fn physical_snap_threshold(dpi: u32) -> f32 {
    if dpi == 0 {
        SNAP_THRESHOLD
    } else {
        SNAP_THRESHOLD * dpi as f32 / 96.0
    }
}

/// 对齐候选的类别罚分系数：成本里加阈值的这一比例，使同等邻近
/// 程度下贴合优先于对齐（见文件头评分规则）。
const ALIGN_KIND_PENALTY: f32 = 0.5;

/// 单轴上的一条候选：成本、目标坐标与贴合命中来源（伙伴贴合
/// 才有：伙伴句柄 + 贴合边）。
#[derive(Clone, Copy, Debug)]
struct DragCandidate {
    cost: f32,
    target: f32,
    hit: Option<(u64, SnapEdge)>,
}

/// 单轴评分求解器：收录全部候选，取成本最小者；替换只在成本严
/// 格更小时发生，故同成本时先生成者胜出（生成顺序稳定可复现）。
#[derive(Default)]
struct DragAxisSolver {
    best: Option<DragCandidate>,
}

impl DragAxisSolver {
    fn consider(&mut self, cost: f32, target: f32, hit: Option<(u64, SnapEdge)>) {
        let candidate = DragCandidate { cost, target, hit };
        match self.best {
            None => self.best = Some(candidate),
            Some(current) if candidate.cost < current.cost => self.best = Some(candidate),
            _ => {}
        }
    }

    /// 贴合候选：间隙在阈值内且另一轴投影真重叠才收。`overlap_ratio`
    /// 是另一轴投影重叠量 ÷ 两窗该轴长度的较小者。
    fn consider_attach(
        &mut self,
        gap: f32,
        overlap_ratio: f32,
        threshold: f32,
        target: f32,
        hit: Option<(u64, SnapEdge)>,
    ) {
        if gap.abs() > threshold || overlap_ratio <= 0.0 {
            return;
        }
        let cost = gap.abs() + (1.0 - overlap_ratio.min(1.0)) * threshold;
        self.consider(cost, target, hit);
    }

    /// 对齐候选：间隙在阈值内且另一轴邻近（投影重叠或相距不超
    /// 阈值）才收。
    fn consider_align(&mut self, gap: f32, other_axis_distance: f32, threshold: f32, target: f32) {
        if gap.abs() > threshold || other_axis_distance > threshold {
            return;
        }
        let cost = gap.abs() + other_axis_distance + ALIGN_KIND_PENALTY * threshold;
        self.consider(cost, target, None);
    }
}

/// 两矩形在某一轴上的相距距离：投影重叠（或恰好相接）为 0，
/// 否则为投影之间的间隙。`horizontal` 为真看横轴，否则看纵轴。
fn axis_distance(a: &WindowRect, b: &WindowRect, horizontal: bool) -> f32 {
    let (a_min, a_max, b_min, b_max) = if horizontal {
        (a.left, a.right, b.left, b.right)
    } else {
        (a.top, a.bottom, b.top, b.bottom)
    };
    (b_min - a_max).max(a_min - b_max).max(0.0)
}

/// 拖动中吸附求解（工单 05，spec 第 15 节）：以拟拖矩形（物理
/// 像素）为被拖窗矩形，按文件头评分规则逐轴裁决，返回目标矩形
/// 与贴合命中；两轴都没有候选时返回 None。纯函数、无状态：同
/// 一输入恒得同一输出，宿主在 WM_MOVING 每帧重复调用即可。
pub fn solve_drag_snap(
    moving: WindowRect,
    partners: &[DragSnapPartner],
    work_area: Option<WindowRect>,
    threshold: f32,
) -> Option<DragSnapOutcome> {
    let mut axis_x = DragAxisSolver::default();
    let mut axis_y = DragAxisSolver::default();
    for partner in partners {
        let p = &partner.rect;
        let ratio_y = moving.overlap_y(p) / moving.height().min(p.height());
        let ratio_x = moving.overlap_x(p) / moving.width().min(p.width());
        // 横轴：右边贴伙伴左边 / 左边贴伙伴右边（要求纵向重叠）。
        axis_x.consider_attach(
            p.left - moving.right,
            ratio_y,
            threshold,
            p.left - moving.width(),
            Some((partner.hwnd, SnapEdge::Right)),
        );
        axis_x.consider_attach(
            moving.left - p.right,
            ratio_y,
            threshold,
            p.right,
            Some((partner.hwnd, SnapEdge::Left)),
        );
        // 横轴对齐：同名边（左对左、右对右），另一轴（纵）须邻近。
        let dist_y = axis_distance(&moving, p, false);
        axis_x.consider_align(p.left - moving.left, dist_y, threshold, p.left);
        axis_x.consider_align(
            p.right - moving.right,
            dist_y,
            threshold,
            p.right - moving.width(),
        );
        // 纵轴：下边贴伙伴上边 / 上边贴伙伴下边（要求横向重叠）。
        axis_y.consider_attach(
            p.top - moving.bottom,
            ratio_x,
            threshold,
            p.top - moving.height(),
            Some((partner.hwnd, SnapEdge::Bottom)),
        );
        axis_y.consider_attach(
            moving.top - p.bottom,
            ratio_x,
            threshold,
            p.bottom,
            Some((partner.hwnd, SnapEdge::Top)),
        );
        let dist_x = axis_distance(&moving, p, true);
        axis_y.consider_align(p.top - moving.top, dist_x, threshold, p.top);
        axis_y.consider_align(
            p.bottom - moving.bottom,
            dist_x,
            threshold,
            p.bottom - moving.height(),
        );
    }
    if let Some(area) = work_area {
        // 工作区边缘贴合（间隙归零）：在全部伙伴候选之后生成，
        // 同成本时伙伴优先。重叠比例按被拖窗与工作区在另一轴的
        // 投影算（通常全重叠 = 1）。
        let ratio_y = moving.overlap_y(&area) / moving.height().min(area.height());
        let ratio_x = moving.overlap_x(&area) / moving.width().min(area.width());
        axis_x.consider_attach(moving.left - area.left, ratio_y, threshold, area.left, None);
        axis_x.consider_attach(
            area.right - moving.right,
            ratio_y,
            threshold,
            area.right - moving.width(),
            None,
        );
        axis_y.consider_attach(moving.top - area.top, ratio_x, threshold, area.top, None);
        axis_y.consider_attach(
            area.bottom - moving.bottom,
            ratio_x,
            threshold,
            area.bottom - moving.height(),
            None,
        );
    }
    if axis_x.best.is_none() && axis_y.best.is_none() {
        return None;
    }
    let mut target = moving;
    if let Some(best) = axis_x.best {
        target.left = best.target;
        target.right = best.target + moving.width();
    }
    if let Some(best) = axis_y.best {
        target.top = best.target;
        target.bottom = best.target + moving.height();
    }
    // 命中在目标矩形完整定下后组装（两轴可各命中一位伙伴）。
    let mut hits = Vec::new();
    for best in [axis_x.best, axis_y.best].into_iter().flatten() {
        if let Some((partner_hwnd, edge)) = best.hit {
            hits.push(SnapHit {
                partner_hwnd,
                edge,
                target,
            });
        }
    }
    Some(DragSnapOutcome { target, hits })
}
