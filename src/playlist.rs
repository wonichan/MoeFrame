//! 播放序列（工单 11）：图库扫描给出的完整路径自然序列表 + 当前位置。
//!
//! 纯逻辑：不做 IO。顺序由 `library::scan_library` 保证，这里只管位置与轮转。

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Next,
    Prev,
}

/// 有序图片列表与当前显示位置。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Playlist {
    items: Vec<PathBuf>,
    current: Option<usize>,
}

impl Playlist {
    pub fn new(items: Vec<PathBuf>) -> Self {
        Self {
            items,
            current: None,
        }
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

    pub fn current_path(&self) -> Option<&PathBuf> {
        self.current.and_then(|index| self.items.get(index))
    }

    /// 按实际显示的路径落位（加载可能跳过坏图，落点不一定是相邻项）。
    /// 路径不在列表中时不改变现状并返回 false。
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

    /// 紧邻的下一张（循环）；单图或无当前时为 None——单图不自转。
    pub fn next_path(&self) -> Option<PathBuf> {
        let current = self.current?;
        if self.items.len() < 2 {
            return None;
        }
        Some(self.items[(current + 1) % self.items.len()].clone())
    }

    /// 从当前位置沿方向出发、绕一圈回到当前之前的全部候选（不含当前图本身）。
    /// 加载按此顺序逐张尝试，坏图自动落到下一候选。单图或无当前时为空。
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
}
