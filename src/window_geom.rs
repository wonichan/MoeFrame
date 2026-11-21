//! 窗口几何生命周期状态机（工单 20 落地、architecture-deepening
//! 工单 05 收拢）：启动离屏钳制、首次放置、宽高比锁定修正、几何
//! 落盘防抖、恢复默认的整台迁移在这里，宿主每帧只喂观察值、执行
//! 返回的抽象命令。纯算术（右下角放置、锁定修正、离屏判定）是本
//! module 的内部实现，不再是公开面。
//!
//! 本状态机只产出「要保存的几何值」（GeomCommand::SaveGeometry），
//! 落盘与退出收尾归 config_sync；启动初值从 config_sync 的启动包
//! 领取；它不直接碰 config，也不与 display_data 横向通信（尺寸
//! 观察由宿主转喂）。
//!
//! 已知限制（只搬家不修复，工单 20 口径）：离屏判定把主屏当作原点
//! 在 (0,0) 的矩形——egui 只给得出相框所在屏的尺寸、没有屏幕原点
//! 与枚举。**整窗合法地摆在副屏上的位置也会被判离屏、启动时被移
//! 回主屏右下角**，即不保存副屏摆放（副屏拔掉防丢与副屏保留在
//! 本 API 下不可兼得，取防丢）。多屏用户以实机清单的点名项为准。

use crate::config_sync::GeometryRecord;

/// 窗口最小边（逻辑尺寸，父规格第 4 节）：锁定修正与宿主最小尺寸共用。
pub const MIN_WINDOW_SIDE: f32 = 120.0;

/// 默认窗口尺寸（逻辑，父规格第 4 节）：恢复默认与宽高比锁定的
/// 比例捕获兜底都用它。
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
    /// 宽高比锁定开关要保存的值（经 config_sync 的宿主状态镜像落盘）。
    SaveLockAspect(bool),
    /// 恢复默认时相框处于隐藏：宿主经行为核心显示相框。
    ShowFrame,
}

/// 窗口几何状态机：持有宽高比锁定的比例、上一帧内尺寸、落盘基准、最近
/// 观察到的待落盘几何、防抖候选与一次性的启动钳制/首次放置标记。
pub struct GeometryMachine {
    locked_aspect: Option<f32>,
    last_inner: [f32; 2],
    saved_geom: GeometryRecord,
    observed_geom: Option<GeometryRecord>,
    candidate: Option<(GeometryRecord, f64)>,
    startup_clamp_done: bool,
    placed: bool,
}

impl GeometryMachine {
    /// 以启动包的几何初值装配：落盘基准、宽高比锁定的比例（开启时已由存档
    /// 尺寸推导）、上一帧内尺寸初值、存档位置是否存在（决定首次
    /// 放置是否还要做）。
    pub fn new(
        saved_geom: GeometryRecord,
        locked_aspect: Option<f32>,
        last_inner: [f32; 2],
        has_saved_position: bool,
    ) -> Self {
        Self {
            locked_aspect,
            last_inner,
            saved_geom,
            observed_geom: None,
            candidate: None,
            startup_clamp_done: false,
            placed: has_saved_position,
        }
    }

    /// 宽高比锁定当前是否开启（设置窗渲染勾选状态用）。
    pub fn is_locked(&self) -> bool {
        self.locked_aspect.is_some()
    }

    /// 喂一帧观察，返回本帧命令。迁移顺序与工单 20 的宿主帧序一致：
    /// 启动钳制 → 锁定修正 → 落盘观察（防抖）→ 首次放置。
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
        // 宽高比锁定修正：指针按下时不抢，停稳后按偏离修正一次。
        if let Some(aspect) = self.locked_aspect {
            if !obs.pointer_down && cur != self.last_inner {
                let corrected = aspect_locked_size(self.last_inner, cur, aspect);
                if (corrected[0] - cur[0]).abs() > 0.5 || (corrected[1] - cur[1]).abs() > 0.5 {
                    commands.push(GeomCommand::SetInnerSize(corrected));
                }
            }
        }
        self.last_inner = cur;
        // 落盘观察：隐藏时不观察；变化停稳 0.6 秒产出一次保存命令。
        if obs.frame_visible {
            if let Some(pos) = obs.outer_pos {
                let geom = (
                    Some(pos[0].round() as i64),
                    Some(pos[1].round() as i64),
                    cur[0].round() as u32,
                    cur[1].round() as u32,
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
                let pos = bottom_right_position(cur, monitor);
                commands.push(GeomCommand::SetOuterPosition(pos));
                self.placed = true;
            }
        }
        commands
    }

    /// 宽高比锁定开关（settings_session 的动作经宿主转发）：开启时
    /// 用最近观察到的内尺寸捕获比例（尚无观察时退回默认尺寸比例，
    /// 工单 20 口径），不再由设置窗代码捕获；返回要落盘的开关值。
    pub fn set_lock(&mut self, locked: bool) -> Vec<GeomCommand> {
        if locked {
            self.locked_aspect = Some(if self.last_inner[1] > 0.0 {
                self.last_inner[0] / self.last_inner[1]
            } else {
                DEFAULT_FRAME_SIZE[0] / DEFAULT_FRAME_SIZE[1]
            });
        } else {
            self.locked_aspect = None;
        }
        vec![GeomCommand::SaveLockAspect(locked)]
    }

    /// 恢复默认位置与大小（工单 20）：立即默认尺寸、当前屏右下角
    /// 内缩并产出保存命令；隐藏时多一条 ShowFrame；锁定开启时锁定
    /// 比例同步改为默认尺寸的比例。显示器取不到时不发位置命令、
    /// 保存记录沿用旧 x/y。
    pub fn restore_default(
        &mut self,
        monitor: Option<[f32; 2]>,
        frame_visible: bool,
    ) -> Vec<GeomCommand> {
        let size = DEFAULT_FRAME_SIZE;
        let mut commands = vec![GeomCommand::SetInnerSize(size)];
        let pos = monitor.map(|m| bottom_right_position(size, m));
        if let Some(pos) = pos {
            commands.push(GeomCommand::SetOuterPosition(pos));
        }
        if !frame_visible {
            commands.push(GeomCommand::ShowFrame);
        }
        if self.locked_aspect.is_some() {
            self.locked_aspect = Some(size[0] / size[1]);
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

/// 宽高比锁定下的尺寸修正（工单 20）：以刚变化的那一边为准推导另
/// 一边（两边都变时以宽为准），再套 120 逻辑最小边——一边触底时
/// 按比例反推另一边，两边都触底时最小边优先、比例让步。aspect
/// 非正/非有限时原样返回。状态机只在指针松开、尺寸变化后调用。
fn aspect_locked_size(last: [f32; 2], current: [f32; 2], aspect: f32) -> [f32; 2] {
    if !aspect.is_finite() || aspect <= 0.0 {
        return current;
    }
    let width_changed = (current[0] - last[0]).abs() > 0.5;
    let height_changed = (current[1] - last[1]).abs() > 0.5;
    let (mut w, mut h) = if width_changed || !height_changed {
        (current[0], current[0] / aspect)
    } else {
        (current[1] * aspect, current[1])
    };
    if w < MIN_WINDOW_SIDE {
        w = MIN_WINDOW_SIDE;
        h = w / aspect;
    }
    if h < MIN_WINDOW_SIDE {
        h = MIN_WINDOW_SIDE;
        w = h * aspect;
    }
    if w < MIN_WINDOW_SIDE {
        w = MIN_WINDOW_SIDE;
    }
    [w, h]
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
