//! 组联动跟随的纯逻辑（多实例吸附专题工单 03）：跟随引擎、抑制
//! 标记（菜单脱离）、轮询调度与锚边变尺寸。伙伴判定、接触派生与
//! 松手求解在 [`crate::snap`]；本模块只消费它派生的接触关系。
//!
//! 跟随是「各自算各自的」：每个进程只掌握自己的窗口、枚举到的
//! 伙伴矩形与接触派生，没有中心协调者（ADR-0004）。防振荡规则
//! 定死（见 FollowEngine 轮询实现）：
//! 1. 只跟随「自上次轮询以来矩形确有变化」的组员；组员静止不发命令。
//! 2. 本窗位置变化若与引擎上一条跟随命令的目标吻合，是命令的落地
//!    回声，不算外部移动；伙伴位移与本窗这段回声位移相同者，是它
//!    在跟随本窗的回声，本轮不反向牵引（回声只消费一轮）。
//! 3. 本窗被外部移动（用户拖动/别家命令）时本轮不跟随，只更新
//!    历史——由伙伴进程各自来跟本窗，避免双向同时跟随打架。
//! 4. 同组多个组员同轮移动时，位移一致才跟随；不一致定死不动，
//!    避免本窗被撕扯到任一边。
//! 5. 组关系按「上次轮询时的接触图」判定移动资格：组员刚被拖走
//!    的那一轮，它在旧图里仍是组员，本窗跟随到自身工作区边缘停
//!    住（跨屏脱组口径）；下一轮起按新图自然脱组。

use crate::snap::{
    pair_in_contact, snap_groups, PartnerFrame, WindowRect, CONTACT_TOLERANCE, MIN_MOVE,
    SNAP_THRESHOLD,
};

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
    /// 的出现条件与立抑制的名单都用这份派生）。
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

    /// 与本窗直接接触的全部伙伴身份（不看抑制）：锚边派生专用
    /// （审查修复 S3 解耦）——菜单脱离的抑制只管拖动联动（跟随
    /// 与成组），不管切图变尺寸的锚边：spec 第 4 节对脱离的定义
    /// 只说后续拖动不再联动，未要求解除第 7 节的锚边关系，故
    /// 锚边按原始接触派生，被脱离的伙伴仍可作锚。
    fn raw_contact_ids(&self, own: PartnerFrame, partners: &[FollowPartner]) -> Vec<PartnerId> {
        partners
            .iter()
            .filter(|partner| {
                partner.frame.monitor == own.monitor
                    && pair_in_contact(&own.rect, &partner.frame.rect)
            })
            .map(|partner| partner.id)
            .collect()
    }

    /// 本窗当前全部贴合边（直接接触伙伴所共享的边，不受抑制影
    /// 响，见 raw_contact_ids 的解耦说明；按 Left/Right/Top/
    /// Bottom 的枚举顺序稳定输出）。
    pub fn anchor_edges(&self, own: PartnerFrame, partners: &[FollowPartner]) -> Vec<AnchorEdge> {
        let contacts = self.raw_contact_ids(own, partners);
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
/// 帧尾唤醒裁决在休眠时得不到任何间隔，不产生周期性唤醒；重新
/// 武装靠本窗自身事件（启动、拖动开始/结束、开关打开）调 wake。
/// 有伙伴全静止时只有心跳级间隔；检测到移动才进入跟随节奏。
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

    /// 本相位下宿主帧尾应请求的唤醒间隔：休眠为 None（零唤醒）。
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
