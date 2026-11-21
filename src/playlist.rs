//! 播放序列（工单 11/12，architecture-deepening 工单 03 起收拢导航决策）：
//! 图库扫描给出的完整路径自然序列表、当前位置，以及随机模式的洗牌袋、
//! 本轮进度与实际历史。「下一张从哪来」只有一个地方能回答：核心按
//! 方向来问计划，这里产出候选集与落位标记；加载成功后核心把
//! 成功路径交回，这里完成落位记账。随机源由本模块持有（生产用系统
//! 随机，测试经构造注入确定性实现），核心不再持有或借出。
//!
//! 纯逻辑：不做 IO。顺序由 `library::scan_library` 保证，这里只管位置与轮转。
//! 本轮进度（本轮成功显示过的路径集合）与历史（成功显示顺序，最近 512 条）
//! 是两份独立状态（ADR-0002）：回看只动历史游标，不消耗或回退本轮进度；
//! 只有成功显示才记账，候选计算与预加载不记账。

use crate::{
    config::PlaybackMode,
    library::{natural_successor_index, rotation_from},
};
use std::{collections::HashSet, path::PathBuf};

/// 历史保留上限（父规格 §3）。
pub const HISTORY_LIMIT: usize = 512;

/// 随机性接缝：生产实现是本模块的系统随机，测试提供确定性实现。
pub trait RandomSource {
    /// 返回 [0, bound) 内的索引；bound 至少为 1。
    fn next_below(&mut self, bound: usize) -> usize;
}

/// 系统随机源（生产）：播放序列持有，核心与宿主都不再经手随机性。
pub struct SystemRandom(rand::rngs::ThreadRng);

impl SystemRandom {
    pub fn new() -> Self {
        Self(rand::rng())
    }
}

impl Default for SystemRandom {
    fn default() -> Self {
        Self::new()
    }
}

impl RandomSource for SystemRandom {
    fn next_below(&mut self, bound: usize) -> usize {
        use rand::RngExt;
        self.0.random_range(0..bound)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Next,
    Prev,
}

/// 落位方式：新鲜选择记账、历史往返只动游标、恢复只落位不记账。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandingKind {
    Fresh,
    Back,
    Forward,
    Restored,
}

/// 落位标记：由本模块产出，核心只原样携带（在飞记录、转场记录），
/// 提交显示时交回 `land`，不解释其内容。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Landing {
    kind: LandingKind,
}

impl Landing {
    pub fn fresh() -> Self {
        Self {
            kind: LandingKind::Fresh,
        }
    }

    pub fn back() -> Self {
        Self {
            kind: LandingKind::Back,
        }
    }

    pub fn forward() -> Self {
        Self {
            kind: LandingKind::Forward,
        }
    }

    pub fn restored() -> Self {
        Self {
            kind: LandingKind::Restored,
        }
    }

    pub fn kind(&self) -> LandingKind {
        self.kind
    }
}

/// 一次导航计划：按序尝试的候选集与胜出者的落位标记。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavPlan {
    pub candidates: Vec<PathBuf>,
    pub landing: Landing,
}

/// 重扫合并的结果：当前图存活、当前图缺失（附替代候选）或图库已空。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RescanPlan {
    EmptyLibrary,
    Survived,
    Missing { candidates: Vec<PathBuf> },
}

/// 续播状态的只读快照（工单 13 落盘用）：当前图、本轮进度（自然序）、
/// 历史（显示顺序）。核心不再直接读本模块的 seen/history/游标字段。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumeSnapshot {
    pub current: Option<PathBuf>,
    pub seen: Vec<PathBuf>,
    pub history: Vec<PathBuf>,
}

/// 有序图片列表与当前显示位置，外加随机轮次与历史状态。
pub struct Playlist {
    items: Vec<PathBuf>,
    current: Option<usize>,
    mode: PlaybackMode,
    /// 本轮进度：本轮成功显示过的路径。
    seen: HashSet<PathBuf>,
    /// 洗牌袋：本轮未看路径的随机候选顺序（不跨启动保存，不含当前图）。
    bag: Vec<PathBuf>,
    /// 新一轮已备好袋但尚未提交：本轮进度要等本轮首张成功显示时才清（父规格 §3：
    /// 只有成功显示才允许提交状态变化，候选计算与预加载不得提前记账）。
    rollover_pending: bool,
    /// 实际历史：成功显示顺序，旧在前、新在后。
    history: Vec<PathBuf>,
    /// 回看游标：Some(i) 表示当前显示的是 history[i] 且不在最新端；None 表示在最新端。
    history_cursor: Option<usize>,
    /// 随机源：本模块持有，候选计算与记账时的回袋插入都经它。
    rng: Box<dyn RandomSource>,
    /// 恢复时声明的当前图（restored 的入参原样）：仍在图库时是首图
    /// 与 Restored 落位的判定依据；已不在图库时是顺序模式按
    /// missing_candidates 找自然序后继的缺失点。
    declared_current: Option<PathBuf>,
    /// 恢复当前图（仍在图库的那张）：它成功显示时只落位不记账（防重复例外）。
    restore_current: Option<PathBuf>,
}

impl Playlist {
    pub fn new(items: Vec<PathBuf>, mode: PlaybackMode, rng: Box<dyn RandomSource>) -> Self {
        Self {
            items,
            current: None,
            mode,
            seen: HashSet::new(),
            bag: Vec::new(),
            rollover_pending: false,
            history: Vec::new(),
            history_cursor: None,
            rng,
            declared_current: None,
            restore_current: None,
        }
    }

    /// 按已保存的当前图、本轮进度与历史恢复（工单 13；本轮进度先按当前图库过滤，
    /// 历史原样保留、回看时跳过不在图库的路径）。随机模式且当前图已不在图库时，
    /// 旧本轮进度清空、对当前图库开始新一轮（顺序模式保留进度）。当前图给出且
    /// 仍在图库时先行落位（游标停在它在历史中的最后位置），其正式显示走
    /// plan_resume_load + initial_landing 的 Restored 落位；当前图缺失时不落位，
    /// 首图由 plan_resume_load 按模式决定——按缺失处理，不拿历史尾部冒充
    /// （工单 02 决定 10）。
    pub fn restored(
        items: Vec<PathBuf>,
        mode: PlaybackMode,
        rng: Box<dyn RandomSource>,
        current: Option<PathBuf>,
        seen: Vec<PathBuf>,
        history: Vec<PathBuf>,
    ) -> Self {
        let in_library = current.as_ref().is_some_and(|path| items.contains(path));
        // 随机模式当前图缺失：清空旧本轮进度（工单 13 准则 4/5 的区别只在此）。
        let seen: HashSet<PathBuf> =
            if mode == PlaybackMode::Random && current.is_some() && !in_library {
                HashSet::new()
            } else {
                seen.into_iter()
                    .filter(|path| items.contains(path))
                    .collect()
            };
        let mut list = Self {
            items,
            current: None,
            mode,
            seen,
            bag: Vec::new(),
            rollover_pending: false,
            history,
            history_cursor: None,
            rng,
            declared_current: current.clone(),
            restore_current: if in_library { current.clone() } else { None },
        };
        if let Some(path) = current {
            if in_library {
                list.land_restored(&path);
            }
        }
        list
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn mode(&self) -> PlaybackMode {
        self.mode
    }

    /// 切换播放模式：轮次与历史保留，洗牌袋与回看游标重置（宿主负责保存选择）。
    pub fn set_mode(&mut self, mode: PlaybackMode) {
        if self.mode != mode {
            self.mode = mode;
            self.bag.clear();
            self.rollover_pending = false;
            self.history_cursor = None;
        }
    }

    pub fn current_path(&self) -> Option<&PathBuf> {
        self.current.and_then(|index| self.items.get(index))
    }

    pub fn clear_current(&mut self) {
        self.current = None;
    }

    /// 续播快照：落盘与状态导出只经这里。
    pub fn resume_snapshot(&self) -> ResumeSnapshot {
        ResumeSnapshot {
            current: self.current_path().cloned(),
            seen: self
                .items
                .iter()
                .filter(|path| self.seen.contains(*path))
                .cloned()
                .collect(),
            history: self.history.clone(),
        }
    }

    /// 导航计划（核心的下一张/上一张，手动与自动同路）：顺序模式沿
    /// 自然序轮转；随机模式上一张沿实际历史回看、下一张在回看中先沿
    /// 历史返回最新端、无路可返才从洗牌袋作新鲜选择。无候选返回 None
    /// （单图、历史已到头、图库为空）。
    pub fn plan_navigation(&mut self, direction: Direction) -> Option<NavPlan> {
        let (candidates, landing) = match (self.mode, direction) {
            (PlaybackMode::Sequential, _) => {
                (self.rotation_candidates(direction), Landing::fresh())
            }
            (PlaybackMode::Random, Direction::Prev) => {
                (self.history_back_candidates(), Landing::back())
            }
            (PlaybackMode::Random, Direction::Next) => {
                let forward = self.history_forward_candidates();
                if self.history_cursor.is_some() && !forward.is_empty() {
                    (forward, Landing::forward())
                } else {
                    (self.bag_candidates(), Landing::fresh())
                }
            }
        };
        if candidates.is_empty() {
            None
        } else {
            Some(NavPlan {
                candidates,
                landing,
            })
        }
    }

    /// 首图计划（无恢复数据的新图库）：随机模式首图也是新选择，候选
    /// 来自洗牌袋而非自然序（工单 12）；顺序模式按自然序全量。
    pub fn plan_first_load(&mut self) -> Vec<PathBuf> {
        match self.mode {
            PlaybackMode::Random => self.bag_candidates(),
            PlaybackMode::Sequential => self.items.clone(),
        }
    }

    /// 恢复后的首图计划：当前图仍在图库时先恢复它，它加载失败时继续
    /// 按模式找下一张（随机取洗牌袋、顺序沿轮转）；当前图缺失时按
    /// 缺失规则（见 missing_candidates）。
    pub fn plan_resume_load(&mut self) -> Vec<PathBuf> {
        if let Some(current) = self.restore_current.clone() {
            let mut candidates = vec![current];
            candidates.extend(match self.mode {
                PlaybackMode::Random => self.bag_candidates(),
                PlaybackMode::Sequential => self.rotation_candidates(Direction::Next),
            });
            return candidates;
        }
        let declared = self.declared_current.clone();
        self.missing_candidates(declared.as_ref())
    }

    /// 首图加载胜出者的落位：恢复当前图本身是 Restored（只落位不
    /// 记账），其余胜出者一律 Fresh。判定后恢复标记即消费（本次启动
    /// 的恢复只发生一次）。
    pub fn initial_landing(&mut self, shown: &PathBuf) -> Landing {
        let restored = self.restore_current.take();
        self.declared_current = None;
        if restored.as_ref() == Some(shown) {
            Landing::restored()
        } else {
            Landing::fresh()
        }
    }

    /// 当前图缺失时的替代候选（工单 13 恢复与工单 16 重扫共用同一套
    /// 规则）：顺序模式从缺失路径的自然序后继起循环寻找（无缺失点
    /// 时全量），随机模式从洗牌袋抽取。
    fn missing_candidates(&mut self, missing: Option<&PathBuf>) -> Vec<PathBuf> {
        match (self.mode, missing) {
            (PlaybackMode::Sequential, Some(missing)) => {
                rotation_from(&self.items, natural_successor_index(&self.items, missing))
            }
            (PlaybackMode::Random, _) => self.bag_candidates(),
            (PlaybackMode::Sequential, None) => self.items.clone(),
        }
    }

    /// 手动重扫后的图库合并（工单 16）：以新图库为准并入增删，返回
    /// 合并结果。本轮进度只保留仍在图库的路径；新增路径作为本轮
    /// 未看并入洗牌袋（已有顺序不重洗，新增随机插入）；已备好的下一
    /// 轮袋在图库变化后作废。历史原样保留（回看时跳过缺失路径）。
    /// 当前图缺失时清空当前图，随机模式另按父规格清空本轮进度、
    /// 准备开始新一轮，替代候选按缺失规则一并给出。
    pub fn rescan(&mut self, new_items: Vec<PathBuf>) -> RescanPlan {
        // 恢复标记只属于本次启动的首图流程，重扫后一律作废。
        self.restore_current = None;
        self.declared_current = None;
        let old_current = self.current_path().cloned();
        let survived = self.merge_rescan(new_items);
        if self.items.is_empty() {
            return RescanPlan::EmptyLibrary;
        }
        if survived {
            return RescanPlan::Survived;
        }
        RescanPlan::Missing {
            candidates: self.missing_candidates(old_current.as_ref()),
        }
    }

    fn merge_rescan(&mut self, new_items: Vec<PathBuf>) -> bool {
        // 当前图以路径为准判断存废（内部存的是下标，换图库后重新定位）。
        // 成员判断走集合，避免大图库下逐项线性扫描。
        let new_set: HashSet<&PathBuf> = new_items.iter().collect();
        let current_path = self.current_path().cloned();
        let current_survives = current_path
            .as_ref()
            .is_some_and(|current| new_set.contains(current));
        self.seen.retain(|path| new_set.contains(path));
        if !current_survives && self.mode == PlaybackMode::Random {
            self.seen.clear();
        }
        if self.rollover_pending {
            self.bag.clear();
            self.rollover_pending = false;
        } else {
            self.bag.retain(|path| new_set.contains(path));
        }
        self.items = new_items;
        if current_survives {
            let current = current_path.expect("current confirmed by the survival check");
            self.set_current(&current);
        } else {
            self.current = None;
            self.history_cursor = None;
        }
        // 未看且不在袋中的路径（新增图、或袋尚未备好的剩余未看）随机插入袋中。
        let bag_set: HashSet<&PathBuf> = self.bag.iter().collect();
        let additions: Vec<PathBuf> = self
            .items
            .iter()
            .filter(|path| {
                !self.seen.contains(*path)
                    && !bag_set.contains(*path)
                    && Some(*path) != self.current_path()
            })
            .cloned()
            .collect();
        for path in additions {
            let at = self.rng.next_below(self.bag.len() + 1);
            self.bag.insert(at, path);
        }
        current_survives
    }

    /// 一次成功显示的落位记账：按落位标记分派——新鲜选择记账、历史
    /// 往返只动游标、恢复只落位。历史落位若因期间切模式清了游标而
    /// 落空，退到只落位不记账（这张图已经成功显示，不能反而清成
    /// 无图）。路径不在图库时不记账并返回 false。
    pub fn land(&mut self, landing: Landing, path: &PathBuf) -> bool {
        match landing.kind {
            LandingKind::Fresh => self.record_fresh(path),
            LandingKind::Back => {
                self.land_history(path, Direction::Prev) || self.land_restored(path)
            }
            LandingKind::Forward => {
                self.land_history(path, Direction::Next) || self.land_restored(path)
            }
            LandingKind::Restored => self.land_restored(path),
        }
    }

    /// 下一张的预加载目标（不记账；随机模式可能备袋）：回看中先沿历史向前，
    /// 顺序模式取紧邻下一张，随机模式取袋首。
    pub fn preload_target(&mut self) -> Option<PathBuf> {
        match self.mode {
            PlaybackMode::Sequential => self.next_path(),
            PlaybackMode::Random => {
                let forward = self.history_forward_candidates();
                if self.history_cursor.is_some() && !forward.is_empty() {
                    forward.into_iter().next()
                } else {
                    self.ensure_bag();
                    self.bag.first().cloned()
                }
            }
        }
    }

    /// 只落位不记账（内部共用）。
    fn set_current(&mut self, path: &PathBuf) -> bool {
        match self.items.iter().position(|item| item == path) {
            Some(index) => {
                self.current = Some(index);
                true
            }
            None => false,
        }
    }

    /// 恢复显示的落位：只落位并把游标停在该路径在历史中的最后位置，不记账、
    /// 不动本轮进度（恢复当前图是防重复例外，工单 02 决定 1）。路径不在历史中
    /// 时游标停在最新端之外（Some(len)），上一张可回看整段历史。路径不在
    /// 图库时返回 false。
    fn land_restored(&mut self, path: &PathBuf) -> bool {
        if !self.set_current(path) {
            return false;
        }
        self.history_cursor = if self.history.is_empty() {
            None
        } else {
            match self.history.iter().rposition(|entry| entry == path) {
                Some(index) if index + 1 == self.history.len() => None,
                Some(index) => Some(index),
                None => Some(self.history.len()),
            }
        };
        true
    }

    /// 一次成功显示的记账：落位、记入本轮进度与历史（超 512 条丢最早）、回到最新端。
    /// 随机模式下，上一张当前图若因新一轮开启被排除在袋外，此刻放回袋中成为普通未看成员。
    /// 路径不在图库时不记账并返回 false。
    fn record_fresh(&mut self, path: &PathBuf) -> bool {
        let previous = self.current_path().cloned();
        if !self.set_current(path) {
            return false;
        }
        if self.rollover_pending {
            // 新一轮在此刻才真正开启：备袋阶段不许提前清本轮进度。
            self.seen.clear();
            self.rollover_pending = false;
        }
        self.seen.insert(path.clone());
        self.bag.retain(|candidate| candidate != path);
        self.history.push(path.clone());
        if self.history.len() > HISTORY_LIMIT {
            let overflow = self.history.len() - HISTORY_LIMIT;
            self.history.drain(..overflow);
        }
        self.history_cursor = None;
        if self.mode == PlaybackMode::Random {
            if let Some(previous) = previous {
                if previous != *path
                    && !self.seen.contains(&previous)
                    && !self.bag.contains(&previous)
                    && self.items.contains(&previous)
                {
                    let at = self.rng.next_below(self.bag.len() + 1);
                    self.bag.insert(at, previous);
                }
            }
        }
        true
    }

    /// 紧邻的下一张（顺序模式循环）；单图或无当前时为 None——单图不自转。
    fn next_path(&self) -> Option<PathBuf> {
        let current = self.current?;
        if self.items.len() < 2 {
            return None;
        }
        Some(self.items[(current + 1) % self.items.len()].clone())
    }

    /// 从当前位置沿方向出发、绕一圈回到当前之前的全部候选（顺序模式，不含当前图本身）。
    fn rotation_candidates(&self, direction: Direction) -> Vec<PathBuf> {
        let Some(current) = self.current else {
            return Vec::new();
        };
        let len = self.items.len();
        if len < 2 {
            return Vec::new();
        }
        (1..len)
            .map(|step| {
                let index = match direction {
                    Direction::Next => (current + step) % len,
                    Direction::Prev => (current + len - step) % len,
                };
                self.items[index].clone()
            })
            .collect()
    }

    /// 洗牌袋候选（随机模式下一次新选择的尝试顺序）。袋空时按未看路径重建；
    /// 未看耗尽且图库至少两张时备好新一轮的袋（当前图暂不入袋，等本轮首张
    /// 成功显示后由 record_fresh 提交新一轮并放回当前图）。备袋不记账。
    fn bag_candidates(&mut self) -> Vec<PathBuf> {
        self.ensure_bag();
        self.bag.clone()
    }

    fn ensure_bag(&mut self) {
        if !self.bag.is_empty() {
            return;
        }
        let current = self.current_path().cloned();
        let mut unseen: Vec<PathBuf> = self
            .items
            .iter()
            .filter(|path| !self.seen.contains(*path) && Some(*path) != current.as_ref())
            .cloned()
            .collect();
        if unseen.is_empty() && self.items.len() >= 2 {
            // 当前图库全部看完：备新一轮的袋，只标记待提交，不动本轮进度。
            self.rollover_pending = true;
            unseen = self
                .items
                .iter()
                .filter(|path| Some(*path) != current.as_ref())
                .cloned()
                .collect();
        }
        shuffle(&mut unseen, &mut *self.rng);
        self.bag = unseen;
    }

    /// 当前显示对应的历史下标（在最新端时为最后一条）。
    fn history_index(&self) -> Option<usize> {
        if self.history.is_empty() {
            None
        } else {
            Some(self.history_cursor.unwrap_or(self.history.len() - 1))
        }
    }

    /// 沿历史向更早方向的回看候选（仍在图库的，按回看顺序）。
    fn history_back_candidates(&self) -> Vec<PathBuf> {
        let Some(index) = self.history_index() else {
            return Vec::new();
        };
        (0..index)
            .rev()
            .map(|i| &self.history[i])
            .filter(|path| self.items.contains(path))
            .cloned()
            .collect()
    }

    /// 沿历史向最新方向的返回候选（仍在图库的，按返回顺序）；不在回看中时为空。
    fn history_forward_candidates(&self) -> Vec<PathBuf> {
        let Some(cursor) = self.history_cursor else {
            return Vec::new();
        };
        (cursor + 1..self.history.len())
            .map(|i| &self.history[i])
            .filter(|path| self.items.contains(path))
            .cloned()
            .collect()
    }

    /// 历史导航成功落位：只动游标与当前图，不记账、不动本轮进度。
    /// 落到最新一条时游标归零（回到最新端）。找不到返回 false。
    fn land_history(&mut self, path: &PathBuf, direction: Direction) -> bool {
        let Some(index) = self.history_index() else {
            return false;
        };
        let landed = match direction {
            Direction::Prev => (0..index).rev().find(|&i| &self.history[i] == path),
            Direction::Next => (index + 1..self.history.len()).find(|&i| &self.history[i] == path),
        };
        match landed {
            Some(i) => {
                self.history_cursor = if i + 1 == self.history.len() {
                    None
                } else {
                    Some(i)
                };
                self.set_current(path)
            }
            None => false,
        }
    }
}

fn shuffle(items: &mut [PathBuf], rng: &mut dyn RandomSource) {
    for i in (1..items.len()).rev() {
        let j = rng.next_below(i + 1);
        items.swap(i, j);
    }
}
