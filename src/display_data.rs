//! 显示数据管线（architecture-deepening 工单 01）：「当前纹理与所显示图片
//! 的显示口径一致」这一不变量只在这里维护——显示数据的记账、转正、过期
//! 判定与刷新决策都在本模块，宿主只上传纹理与绘制。
//!
//! 形状与手势机、行为核心一致：egui-free，宿主把加载回送与每帧观察喂
//! 进来，本模块返回裁决与刷新命令。纹理句柄归宿主，按本模块的抽象槽位
//! （当前 / 传入）对应持有；刷新的决策形态是「替换当前槽内容」。
//!
//! 刷新失败策略（2026-11-21 grilling 定案，明文于此）：刷新失败时保持
//! 当前纹理不动、不重试、不提示；同一期望口径不再重复触发，直到口径
//! 变化（模式、目标或当前图）或导航转正刷新了当前槽记录。

use crate::decode_validation::DisplayMode;
use std::path::{Path, PathBuf};

/// 目标尺寸变化的停稳时长（秒）：停稳才触发刷新，避免拖动缩放中反复解码。
const TARGET_SETTLE_SECS: f64 = 0.25;

/// 「同一份显示数据」的身份：路径 + 物理目标 + 显示方式。worker 预载
/// 缓存的键口径由这里定义，缓存存储仍在 worker。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayKey {
    pub path: PathBuf,
    pub target: [u32; 2],
    pub mode: DisplayMode,
}

/// 导航加载回送的裁决。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDecision {
    /// 回送有效：图已记为传入槽，等宿主上传纹理并由核心决定提交或转场。
    Accept,
    /// 回送有效但加载全失败：当前槽记录不动，在飞已清。
    Failed,
    /// 过期回送（换文件夹/重扫之后）：不碰任何记录。
    Stale,
}

/// 刷新回送的裁决。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshDecision {
    /// 用回送图替换当前槽内容，当前槽记录记为刷新的期望键。
    Swap,
    /// 丢弃回送图：画面已切走、待决已被作废，或刷新失败（保旧图）。
    Discard,
}

/// 宿主每帧采样到的、刷新决策需要的观察值。
#[derive(Clone, Debug)]
pub struct Observation {
    /// 核心当前正在显示的图；None 为无图状态。
    pub showing: Option<PathBuf>,
    /// 当前生效的显示方式（宿主状态）。
    pub fit_mode: DisplayMode,
    /// 当前窗口的物理目标尺寸。
    pub target: [u32; 2],
    /// 滑动转场进行中（转场期间不刷新）。
    pub slide_active: bool,
    /// 宿主 UI 时钟的秒值（防抖计时）。
    pub now_secs: f64,
}

/// 一次在飞的核心导航加载：序号与发出时的显示口径。
struct NavLoad {
    seq: u64,
    mode: DisplayMode,
    target: [u32; 2],
}

#[derive(Default)]
pub struct DisplayPipeline {
    next_seq: u64,
    nav_in_flight: Option<NavLoad>,
    /// 当前槽里纹理的显示数据身份。
    current: Option<DisplayKey>,
    /// 传入槽里纹理的显示数据身份（转场目标图或待提交的首图）。
    incoming: Option<DisplayKey>,
    /// 已发出、等回送的刷新期望键。
    refresh_pending: Option<DisplayKey>,
    /// 已失败过的刷新期望键：同键不再重试（见模块头策略）。
    refresh_failed_for: Option<DisplayKey>,
    /// 目标尺寸防抖候选与起始时刻。
    target_candidate: Option<([u32; 2], f64)>,
}

impl DisplayPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_key(&self) -> Option<&DisplayKey> {
        self.current.as_ref()
    }

    pub fn incoming_key(&self) -> Option<&DisplayKey> {
        self.incoming.as_ref()
    }

    /// 是否有核心导航加载在飞（宿主据此决定回送轮询节奏）。
    pub fn load_in_flight(&self) -> bool {
        self.nav_in_flight.is_some()
    }

    /// 发起一次核心导航加载：分配序号并记下在飞口径。新加载顶替旧的
    /// 在飞记录——旧回送此后按过期处理。
    pub fn begin_load(&mut self, mode: DisplayMode, target: [u32; 2]) -> u64 {
        self.next_seq += 1;
        self.nav_in_flight = Some(NavLoad {
            seq: self.next_seq,
            mode,
            target,
        });
        self.next_seq
    }

    /// 新一轮扫描开始：在飞加载的回送从此过期，待决刷新作废，
    /// 刷新失败记忆一并清（上下文已换）。
    pub fn invalidate(&mut self) {
        self.nav_in_flight = None;
        self.refresh_pending = None;
        self.refresh_failed_for = None;
        self.target_candidate = None;
    }

    /// 导航加载回送裁决。Accept 时回送图记为传入槽，口径取在飞记录
    /// （不是回送到达时的宿主当前口径）。
    pub fn nav_reply(&mut self, seq: u64, image_path: Option<&Path>) -> NavDecision {
        let Some(nav) = &self.nav_in_flight else {
            return NavDecision::Stale;
        };
        if nav.seq != seq {
            return NavDecision::Stale;
        }
        let nav = self.nav_in_flight.take().expect("在飞记录刚核对过");
        match image_path {
            Some(path) => {
                self.incoming = Some(DisplayKey {
                    path: path.to_path_buf(),
                    target: nav.target,
                    mode: nav.mode,
                });
                NavDecision::Accept
            }
            None => NavDecision::Failed,
        }
    }

    /// 传入槽转正为当前槽：直接提交与转场完成共用同一条记账路径。
    /// 转正意味着当前槽换了数据，刷新失败记忆随之失效。
    pub fn promote_incoming(&mut self) {
        if let Some(key) = self.incoming.take() {
            self.current = Some(key);
            self.refresh_failed_for = None;
        }
    }

    /// 丢弃传入槽记录（扫描、清空画面时宿主已丢弃对应纹理）。
    pub fn drop_incoming(&mut self) {
        self.incoming = None;
    }

    /// 当前槽记录清空（核心回到无图状态、宿主已清纹理）。
    pub fn clear_current(&mut self) {
        self.current = None;
    }

    /// 每帧刷新决策：当前槽口径过期且满足触发条件时，返回要刷新的
    /// 期望键并记为待决刷新；其余情况返回 None。
    pub fn tick(&mut self, obs: &Observation) -> Option<DisplayKey> {
        if self.refresh_pending.is_some() || obs.slide_active {
            return None;
        }
        let showing = obs.showing.as_ref()?;
        // 在飞导航加载若已按期望口径发出，其转正会刷新当前槽记录。
        if let Some(nav) = &self.nav_in_flight {
            if nav.mode == obs.fit_mode && nav.target == obs.target {
                return None;
            }
        }
        let current = self.current.as_ref()?;
        let mode_stale = current.mode != obs.fit_mode;
        let target_stale = current.target != obs.target;
        if !mode_stale && !target_stale {
            self.target_candidate = None;
            return None;
        }
        let desired = DisplayKey {
            path: showing.clone(),
            target: obs.target,
            mode: obs.fit_mode,
        };
        if self.refresh_failed_for.as_ref() == Some(&desired) {
            return None;
        }
        if !mode_stale {
            // 只有目标过期：停稳满 TARGET_SETTLE_SECS 才触发。
            match self.target_candidate {
                Some((candidate, since)) if candidate == obs.target => {
                    if obs.now_secs - since < TARGET_SETTLE_SECS {
                        return None;
                    }
                }
                _ => {
                    self.target_candidate = Some((obs.target, obs.now_secs));
                    return None;
                }
            }
        }
        self.target_candidate = None;
        self.refresh_pending = Some(desired.clone());
        Some(desired)
    }

    /// 刷新回送裁决。成功且画面未切走、转场未在进行时 Swap；刷新失败
    /// 记下期望键，同键不再重试（模块头策略）；画面已切走或转场中的
    /// 回送只是过期，不记失败（转正后会按新口径再评估）。
    pub fn refresh_reply(
        &mut self,
        image_path: Option<&Path>,
        showing: Option<&Path>,
        slide_active: bool,
    ) -> RefreshDecision {
        let Some(pending) = self.refresh_pending.take() else {
            return RefreshDecision::Discard;
        };
        if slide_active {
            return RefreshDecision::Discard;
        }
        match image_path {
            Some(path) if showing == Some(path) && *path == pending.path => {
                self.current = Some(pending);
                self.refresh_failed_for = None;
                RefreshDecision::Swap
            }
            Some(_) => RefreshDecision::Discard,
            None => {
                self.refresh_failed_for = Some(pending);
                RefreshDecision::Discard
            }
        }
    }
}
