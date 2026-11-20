//! 播放序列（工单 11/12）：图库扫描给出的完整路径自然序列表、当前位置，
//! 以及随机模式的洗牌袋、本轮进度与实际历史。
//!
//! 纯逻辑：不做 IO。顺序由 `library::scan_library` 保证，这里只管位置与轮转。
//! 本轮进度（本轮成功显示过的路径集合）与历史（成功显示顺序，最近 512 条）
//! 是两份独立状态（ADR-0002）：回看只动历史游标，不消耗或回退本轮进度；
//! 只有成功显示才记账，候选计算与预加载不记账。

use crate::config::PlaybackMode;
use std::{collections::HashSet, path::PathBuf};

/// 历史保留上限（父规格 §3）。
pub const HISTORY_LIMIT: usize = 512;

/// 随机性接缝：生产实现由宿主提供（系统随机），测试提供确定性实现。
pub trait RandomSource {
    /// 返回 [0, bound) 内的索引；bound 至少为 1。
    fn next_below(&mut self, bound: usize) -> usize;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Next,
    Prev,
}

/// 有序图片列表与当前显示位置，外加随机轮次与历史状态。
#[derive(Clone, Debug)]
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
}

impl Default for Playlist {
    fn default() -> Self {
        Self::new(Vec::new(), PlaybackMode::Random)
    }
}

impl Playlist {
    pub fn new(items: Vec<PathBuf>, mode: PlaybackMode) -> Self {
        Self {
            items,
            current: None,
            mode,
            seen: HashSet::new(),
            bag: Vec::new(),
            rollover_pending: false,
            history: Vec::new(),
            history_cursor: None,
        }
    }

    /// 按已保存的当前图、本轮进度与历史恢复（工单 13；本轮进度先按当前图库过滤，
    /// 历史原样保留、回看时跳过不在图库的路径）。当前图给出且仍在图库时就位，
    /// 游标停在它在历史中的最后位置（不在历史中则视为在最新端之外，上一张可
    /// 回看整段历史）；当前图缺失（已不在图库，或字段缺失/损坏被丢弃）时不落位，
    /// 首图由核心按模式另行决定——按缺失处理，不拿历史尾部冒充（工单 02 决定 10）。
    pub fn restored(
        items: Vec<PathBuf>,
        mode: PlaybackMode,
        current: Option<PathBuf>,
        seen: Vec<PathBuf>,
        history: Vec<PathBuf>,
    ) -> Self {
        let seen: HashSet<PathBuf> = seen
            .into_iter()
            .filter(|path| items.contains(path))
            .collect();
        let mut list = Self {
            items,
            current: None,
            mode,
            seen,
            bag: Vec::new(),
            rollover_pending: false,
            history,
            history_cursor: None,
        };
        if let Some(path) = current {
            if list.items.contains(&path) {
                list.land_restored(&path);
            }
        }
        list
    }

    /// 恢复显示的落位：只落位并把游标停在该路径在历史中的最后位置，不记账、
    /// 不动本轮进度（恢复当前图是防重复例外，工单 02 决定 1）。路径不在历史中
    /// 时游标停在最新端之外（Some(len)），上一张可回看整段历史。路径不在
    /// 图库时返回 false。
    pub fn land_restored(&mut self, path: &PathBuf) -> bool {
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

    /// 本轮进度按图库自然序导出（跨启动保存用）。
    pub fn seen_paths(&self) -> Vec<PathBuf> {
        self.items
            .iter()
            .filter(|path| self.seen.contains(*path))
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[PathBuf] {
        &self.items
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

    /// 只落位不记账（顺序模式定位与内部使用）。
    pub fn set_current(&mut self, path: &PathBuf) -> bool {
        match self.items.iter().position(|item| item == path) {
            Some(index) => {
                self.current = Some(index);
                true
            }
            None => false,
        }
    }

    pub fn clear_current(&mut self) {
        self.current = None;
    }

    pub fn is_seen(&self, path: &PathBuf) -> bool {
        self.seen.contains(path)
    }

    pub fn history(&self) -> &[PathBuf] {
        &self.history
    }

    pub fn history_cursor(&self) -> Option<usize> {
        self.history_cursor
    }

    /// 一次成功显示的记账：落位、记入本轮进度与历史（超 512 条丢最早）、回到最新端。
    /// 随机模式下，上一张当前图若因新一轮开启被排除在袋外，此刻放回袋中成为普通未看成员。
    /// 路径不在图库时不记账并返回 false。
    pub fn record_fresh(&mut self, path: &PathBuf, rng: &mut dyn RandomSource) -> bool {
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
                    let at = rng.next_below(self.bag.len() + 1);
                    self.bag.insert(at, previous);
                }
            }
        }
        true
    }

    /// 紧邻的下一张（顺序模式循环）；单图或无当前时为 None——单图不自转。
    pub fn next_path(&self) -> Option<PathBuf> {
        let current = self.current?;
        if self.items.len() < 2 {
            return None;
        }
        Some(self.items[(current + 1) % self.items.len()].clone())
    }

    /// 从当前位置沿方向出发、绕一圈回到当前之前的全部候选（顺序模式，不含当前图本身）。
    pub fn rotation_candidates(&self, direction: Direction) -> Vec<PathBuf> {
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
    pub fn bag_candidates(&mut self, rng: &mut dyn RandomSource) -> Vec<PathBuf> {
        self.ensure_bag(rng);
        self.bag.clone()
    }

    fn ensure_bag(&mut self, rng: &mut dyn RandomSource) {
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
        shuffle(&mut unseen, rng);
        self.bag = unseen;
    }

    /// 下一张的预加载目标（不记账；随机模式可能备袋）：回看中先沿历史向前，
    /// 顺序模式取紧邻下一张，随机模式取袋首。
    pub fn preload_target(&mut self, rng: &mut dyn RandomSource) -> Option<PathBuf> {
        match self.mode {
            PlaybackMode::Sequential => self.next_path(),
            PlaybackMode::Random => {
                let forward = self.history_forward_candidates();
                if self.history_cursor.is_some() && !forward.is_empty() {
                    forward.into_iter().next()
                } else {
                    self.ensure_bag(rng);
                    self.bag.first().cloned()
                }
            }
        }
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
    pub fn history_back_candidates(&self) -> Vec<PathBuf> {
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
    pub fn history_forward_candidates(&self) -> Vec<PathBuf> {
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
    pub fn land_history(&mut self, path: &PathBuf, direction: Direction) -> bool {
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
