//! 工单 09 的验证装配：使用显式 Limits 调用产品解码、观测阶段，
//! 用两槽轮播验证当前图与下一张显示数据的驻留。

use crate::loading::{load_image_observed, DecodeError, DecodePhase, DisplayImage, LoadRequest};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// 实验入口只传请求并接收阶段事件；解码和 Limits 的语义与产品相同。
pub fn load_display(
    path: &Path,
    request: LoadRequest,
    observe: impl FnMut(DecodePhase),
) -> Result<DisplayImage, DecodeError> {
    load_image_observed(path, request, observe)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeRole {
    Current,
    Next,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CycleEvent {
    Phase {
        path: PathBuf,
        role: DecodeRole,
        phase: DecodePhase,
    },
    Prepared {
        path: PathBuf,
        role: DecodeRole,
        source_pixels: [u32; 2],
        scaled_pixels: [u32; 2],
        display_pixels: [u32; 2],
        rgba_bytes: usize,
    },
    Rejected {
        error: DecodeError,
        role: DecodeRole,
    },
    Retired {
        path: PathBuf,
        other_owners_at_retirement: usize,
    },
}

/// 快照只共享当前/下一张显示数据，不包含原尺寸图或历史缓存。
#[derive(Clone, Debug, Default)]
pub struct CycleSnapshot {
    pub current: Option<Arc<DisplayImage>>,
    pub next: Option<Arc<DisplayImage>>,
}
impl CycleSnapshot {
    pub fn resident_rgba_bytes(&self) -> usize {
        self.current
            .iter()
            .chain(self.next.iter())
            .map(|image| image.rgba.len())
            .sum()
    }
}

/// 真实轮播接缝；宿主在单个工作线程调用，快照可送给 GUI。
pub struct ValidationCycle {
    paths: Vec<PathBuf>,
    cursor: usize,
    request: LoadRequest,
    current: Option<Arc<DisplayImage>>,
    next: Option<Arc<DisplayImage>>,
}
impl ValidationCycle {
    pub fn new(paths: Vec<PathBuf>, request: LoadRequest) -> Self {
        Self {
            paths,
            cursor: 0,
            request,
            current: None,
            next: None,
        }
    }

    pub fn snapshot(&self) -> CycleSnapshot {
        CycleSnapshot {
            current: self.current.clone(),
            next: self.next.clone(),
        }
    }

    /// 只准备当前图并立即发布快照，不触碰下一张；
    /// 宿主在当前图显示提交后才调用 preload_next（父规格：当前图显示后预加载下一张）。
    pub fn prime_current(&mut self, mut observe: impl FnMut(CycleEvent)) -> CycleSnapshot {
        if self.current.is_none() {
            self.current = self.load_available(DecodeRole::Current, &mut observe);
        }
        self.snapshot()
    }

    /// 当前图发布后才准备下一张；没有当前图或下一张已就绪时不做遍历。
    pub fn preload_next(&mut self, mut observe: impl FnMut(CycleEvent)) -> CycleSnapshot {
        if self.current.is_some() && self.next.is_none() {
            self.next = self.load_available(DecodeRole::Next, &mut observe);
        }
        self.snapshot()
    }

    /// 只完成退役与提升并立即发布，不解码新的下一张；
    /// 宿主在提升后的当前图显示提交后调用 preload_next。
    pub fn advance(&mut self, mut observe: impl FnMut(CycleEvent)) -> CycleSnapshot {
        Self::retire(&mut self.current, &mut observe);
        self.current = self.next.take();
        if self.current.is_none() {
            self.current = self.load_available(DecodeRole::Current, &mut observe);
        }
        self.snapshot()
    }

    /// Remove both cycle slots; release of caller snapshots is the caller's responsibility.
    pub fn clear(&mut self, mut observe: impl FnMut(CycleEvent)) {
        Self::retire(&mut self.current, &mut observe);
        Self::retire(&mut self.next, &mut observe);
    }

    fn retire(slot: &mut Option<Arc<DisplayImage>>, observe: &mut impl FnMut(CycleEvent)) {
        if let Some(image) = slot.take() {
            let path = image.path.clone();
            let other_owners_at_retirement = Arc::strong_count(&image) - 1;
            drop(image);
            // This retires the cycle slot. A snapshot owner may still retain the display buffer.
            observe(CycleEvent::Retired {
                path,
                other_owners_at_retirement,
            });
        }
    }

    fn load_available(
        &mut self,
        role: DecodeRole,
        observe: &mut impl FnMut(CycleEvent),
    ) -> Option<Arc<DisplayImage>> {
        // At most one traversal per request: all-bad input cannot spin forever or retain errors.
        for _ in 0..self.paths.len() {
            let path = self.paths[self.cursor].clone();
            self.cursor = (self.cursor + 1) % self.paths.len();
            match load_display(&path, self.request, |phase| {
                observe(CycleEvent::Phase {
                    path: path.clone(),
                    role,
                    phase,
                });
            }) {
                Ok(image) => {
                    observe(CycleEvent::Prepared {
                        path: image.path.clone(),
                        role,
                        source_pixels: image.source_pixels,
                        scaled_pixels: image.scaled_pixels,
                        display_pixels: image.display_pixels,
                        rgba_bytes: image.rgba.len(),
                    });
                    return Some(Arc::new(image));
                }
                Err(error) => observe(CycleEvent::Rejected { error, role }),
            }
        }
        None
    }
}
