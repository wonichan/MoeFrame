//! 窗口几何生命周期状态机（工单 20 落地、深挖工单 05 收拢）：启动离屏钳制、首次放置、按当前图像比例从窗口高度推导宽度、几何落盘防抖与恢复默认都在这里；宿主每帧只喂观察值、执行返回的抽象命令。
//!
//! 本状态机只产出要保存的几何值；落盘与退出收尾归 config_sync。启动初值来自配置；图像比例由宿主在新图到达时更新。
//!
//! 已知限制：离屏判定把主屏当作原点 (0,0) 的矩形。egui 只提供相框所在屏尺寸，不提供屏幕原点与屏幕枚举，因此整窗放在副屏重启后会被移回主屏右下角，作为防丢行为保留。

use crate::config_sync::GeometryRecord;

/// 窗口最小边（逻辑尺寸）：保持无黑边时仍满足宿主最小窗口约束。
pub const MIN_WINDOW_SIDE: f32 = 120.0;
/// 默认窗口尺寸（逻辑）：无图时使用此尺寸，有图时高度作为缩放基准。
pub const DEFAULT_FRAME_SIZE: [f32; 2] = [480.0, 320.0];

/// 几何落盘防抖（工单 20）：观察值停稳满此时长才产出保存命令。
const SAVE_SETTLE_SECS: f64 = 0.6;

/// 宿主每帧喂入的观察值：窗口矩形、显示器尺寸、指针与可见性。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GeomObservation {
    /// 窗口外矩形的位置（左上，逻辑坐标）；取不到时为 None。
    pub outer_pos: Option<[f32; 2]>,
    /// 窗口外矩形的尺寸；取不到时为 None。
    pub outer_size: Option<[f32; 2]>,
    /// 窗口内尺寸；取不到时为 None（本帧不做修正与落盘观察）。
    pub inner: Option<[f32; 2]>,
    /// 相框所在屏的尺寸；取不到时为 None。
    pub monitor: Option<[f32; 2]>,
    /// 指针是否按下（拖动/缩放进行中）：按下时不做锁定修正。
    pub pointer_down: bool,
    /// 相框是否可见：隐藏时不观察待落盘几何（工单 20）。
    pub frame_visible: bool,
    /// UI 时钟（秒）：防抖计时用。
    pub now_secs: f64,
}

/// 状态机返回的抽象命令：宿主翻译成 ViewportCommand 执行，保存类
/// 命令经 config_sync 落盘。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeomCommand {
    /// 设置窗口外位置（左上，逻辑坐标）。
    SetOuterPosition([f32; 2]),
    /// 设置窗口内尺寸（逻辑）。
    SetInnerSize([f32; 2]),
    /// 要保存的几何值（落盘基准由状态机同步推进，宿主不必回告）。
    SaveGeometry(GeometryRecord),
    /// 恢复默认时相框处于隐藏：宿主经行为核心显示相框。
    ShowFrame,
}

/// 持有当前图像比例、最近稳定内尺寸、落盘基准、待落盘几何与一次性的启动钳制/首次放置标记。
pub struct GeometryMachine {
    image_aspect: Option<f32>,
    aspect_applied: bool,
    last_inner: [f32; 2],
    saved_geom: GeometryRecord,
    observed_geom: Option<GeometryRecord>,
    candidate: Option<(GeometryRecord, f64)>,
    startup_clamp_done: bool,
    placed: bool,
}

impl GeometryMachine {
    /// 以启动几何装配；收到首张图后再用图像比例修正宽度。
    pub fn new(
        saved_geom: GeometryRecord,
        image_aspect: Option<f32>,
        has_saved_position: bool,
    ) -> Self {
        let image_aspect = valid_aspect(image_aspect);
        Self {
            image_aspect,
            aspect_applied: image_aspect.is_none(),
            last_inner: [saved_geom.2 as f32, saved_geom.3 as f32],
            saved_geom,
            observed_geom: None,
            candidate: None,
            startup_clamp_done: false,
            placed: has_saved_position,
        }
    }

    /// 新图到达时更新比例；比例变化后下一帧按当前高度调整窗口。
    pub fn set_image_aspect(&mut self, aspect: Option<f32>) {
        let aspect = valid_aspect(aspect);
        if self.image_aspect != aspect {
            self.image_aspect = aspect;
            self.aspect_applied = aspect.is_none();
        }
    }

    /// 喂一帧观察，返回本帧命令：启动钳制 → 图像比例校正 → 落盘观察 → 首次放置。
    pub fn observe(&mut self, obs: &GeomObservation) -> Vec<GeomCommand> {
        let mut commands = Vec::new();
        // 启动离屏钳制：只查一次，等外矩形与显示器都取到才查。
        if !self.startup_clamp_done {
            if let (Some(pos), Some(size), Some(monitor)) =
                (obs.outer_pos, obs.outer_size, obs.monitor)
            {
                self.startup_clamp_done = true;
                if let Some(clamped) = offscreen_position(pos, size, monitor) {
                    commands.push(GeomCommand::SetOuterPosition(clamped));
                }
            }
        }
        let Some(cur) = obs.inner else {
            return commands;
        };
        let mut effective_inner = cur;
        if !obs.pointer_down {
            if let Some(aspect) = self.image_aspect {
                if !self.aspect_applied || cur != self.last_inner {
                    let corrected = aspect_size_from_height(cur, aspect);
                    if (corrected[0] - cur[0]).abs() > 0.5 || (corrected[1] - cur[1]).abs() > 0.5 {
                        commands.push(GeomCommand::SetInnerSize(corrected));
                    }
                    effective_inner = corrected;
                    self.aspect_applied = true;
                }
            }
            self.last_inner = cur;
        }
        if obs.frame_visible {
            if let Some(pos) = obs.outer_pos {
                let geom = (
                    Some(pos[0].round() as i64),
                    Some(pos[1].round() as i64),
                    effective_inner[0].round() as u32,
                    effective_inner[1].round() as u32,
                );
                self.observed_geom = Some(geom);
                if geom == self.saved_geom {
                    self.candidate = None;
                } else {
                    match self.candidate {
                        Some((value, since)) if value == geom => {
                            if obs.now_secs - since >= SAVE_SETTLE_SECS {
                                self.saved_geom = geom;
                                self.candidate = None;
                                commands.push(GeomCommand::SaveGeometry(geom));
                            }
                        }
                        _ => {
                            self.candidate = Some((geom, obs.now_secs));
                        }
                    }
                }
            }
        }
        // 首次放置：无存档位置时，第一帧取到显示器就放右下角内缩。
        if !self.placed {
            if let Some(monitor) = obs.monitor {
                let pos = bottom_right_position(effective_inner, monitor);
                commands.push(GeomCommand::SetOuterPosition(pos));
                self.placed = true;
            }
        }
        commands
    }

    /// 新图的比例延迟至非拖动帧应用，避免抢占系统窗口缩放。
    /// 恢复默认位置与大小：高度回到 320 逻辑像素，并按当前图像比例推导宽度；
    /// 位置放在当前屏右下角内缩处，隐藏时多一条 ShowFrame。显示器取不到时
    /// 不发位置命令，保存记录沿用旧 x/y。
    pub fn restore_default(
        &mut self,
        monitor: Option<[f32; 2]>,
        frame_visible: bool,
    ) -> Vec<GeomCommand> {
        let size = self
            .image_aspect
            .map(|aspect| aspect_size_from_height(DEFAULT_FRAME_SIZE, aspect))
            .unwrap_or(DEFAULT_FRAME_SIZE);
        self.last_inner = size;
        self.aspect_applied = true;
        let mut commands = vec![GeomCommand::SetInnerSize(size)];
        let pos = monitor.map(|m| bottom_right_position(size, m));
        if let Some(pos) = pos {
            commands.push(GeomCommand::SetOuterPosition(pos));
        }
        if !frame_visible {
            commands.push(GeomCommand::ShowFrame);
        }
        let (x, y) = match pos {
            Some(p) => (Some(p[0].round() as i64), Some(p[1].round() as i64)),
            None => (self.saved_geom.0, self.saved_geom.1),
        };
        let record = (x, y, size[0] as u32, size[1] as u32);
        self.saved_geom = record;
        self.candidate = None;
        commands.push(GeomCommand::SaveGeometry(record));
        commands
    }

    /// 退出结算的交接（工单 04）：把（最近观察到的几何、落盘基准）
    /// 交给 config_sync 的 shutdown 一次结清。
    pub fn exit_settle(&self) -> (Option<GeometryRecord>, GeometryRecord) {
        (self.observed_geom, self.saved_geom)
    }
}

/// 右下角内缩放置（首次启动、恢复默认、离屏钳制共用）：给定窗口
/// 尺寸与显示器尺寸，返回右下角内缩 24px 的位置（不小于 0）。
fn bottom_right_position(size: [f32; 2], monitor: [f32; 2]) -> [f32; 2] {
    [
        (monitor[0] - size[0] - 24.0).max(0.0),
        (monitor[1] - size[1] - 24.0).max(0.0),
    ]
}

/// 按当前高度推导图像比例尺寸，并保持最小边约束。
fn aspect_size_from_height(current: [f32; 2], aspect: f32) -> [f32; 2] {
    let Some(aspect) = valid_aspect(Some(aspect)) else {
        return current;
    };
    let height = if current[1].is_finite() && current[1] > 0.0 {
        current[1].max(MIN_WINDOW_SIDE)
    } else {
        DEFAULT_FRAME_SIZE[1]
    };
    let width = height * aspect;
    if width < MIN_WINDOW_SIDE {
        [MIN_WINDOW_SIDE, MIN_WINDOW_SIDE / aspect]
    } else {
        [width, height]
    }
}

fn valid_aspect(aspect: Option<f32>) -> Option<f32> {
    aspect.filter(|value| value.is_finite() && *value > 0.0)
}

/// 启动恢复位置的主屏钳制（工单 20）：窗口与主屏完全无交集时返回
/// 主屏右下角内缩 24px 的位置；有交集返回 None。限制口径见模块头。
fn offscreen_position(position: [f32; 2], size: [f32; 2], monitor: [f32; 2]) -> Option<[f32; 2]> {
    let intersects = position[0] < monitor[0]
        && position[1] < monitor[1]
        && position[0] + size[0] > 0.0
        && position[1] + size[1] > 0.0;
    if intersects {
        None
    } else {
        Some(bottom_right_position(size, monitor))
    }
}
