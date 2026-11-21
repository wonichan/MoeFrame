//! 指针手势分界（工单 15；决定见工单 01 与 ADR-0001）：宿主把左键按下、移动、
//! 松开与右键按下的原始事件送进来，这里用 Windows 系统阈值判定拖动、单击与
//! 双击，只产出判定结果，不碰窗口移动与图片打开本身。时间由事件携带，测试可控。
//!
//! 规则要点：移动超过系统拖动阈值即拖动，松开不算点击；侧边单击要等系统双击
//! 窗口结束才成立，双击窗口按两次「按下」之间的间隔量（锚在第一下按下时刻）；
//! 等待期间的新按下超出双击距离、或移动超过拖动阈值，先把第一下按单击结算，
//! 新按下重新判断；双击打开的是第一下按下时正在显示的图。

use std::path::PathBuf;

/// Windows 系统手势阈值，由宿主读取系统设置后注入：拖动阈值
/// （SM_CXDRAG/SM_CYDRAG）、双击时间（GetDoubleClickTime）与双击距离
/// （SM_CXDOUBLECLK/SM_CYDOUBLECLK）。不写死在判定里，用户改系统设置即跟随。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureThresholds {
    pub drag_threshold: [f32; 2],
    pub double_click_ms: u64,
    pub double_click_dist: [f32; 2],
}

/// 点击落点分区：左、右各 20% 是导航区，中间 60% 单击无动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Left,
    Middle,
    Right,
}

impl Zone {
    /// 按按下点横坐标占相框宽度的比例分区（按下时刻的几何，缩放不追溯）。
    pub fn from_position(x: f32, frame_width: f32) -> Zone {
        if frame_width <= 0.0 {
            return Zone::Middle;
        }
        if x < frame_width * 0.2 {
            Zone::Left
        } else if x > frame_width * 0.8 {
            Zone::Right
        } else {
            Zone::Middle
        }
    }
}

/// 手势判定结果，交行为核心执行（单击→导航，双击→打开原图）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GestureOutcome {
    Click(Zone),
    DoubleClick(PathBuf),
}

/// 一次按下的记录：位置、按下时刻、分区与按下时正在显示的图（双击打开它，
/// 不随后续切换变化）。等待结算与进行中的按下共用这份记录。
#[derive(Clone, Debug)]
struct PressRecord {
    down_pos: [f32; 2],
    down_at: u64,
    zone: Zone,
    displayed: Option<PathBuf>,
}

/// 一次等待结算的单击：第一下已松开，正在等双击窗口结束或第二下。
/// 结算时刻锚在第一下按下时刻 + 系统双击时间，与系统双击窗口同口径。
#[derive(Clone, Debug)]
struct Waiting {
    record: PressRecord,
    settle_at: u64,
}

/// 一次进行中的按下。
#[derive(Clone, Debug)]
struct Press {
    record: PressRecord,
    dragging: bool,
    /// 若这次按下是等待单击的候选第二下，这里带着第一下的记录。
    second_of: Option<Waiting>,
}

#[derive(Clone, Debug)]
enum State {
    Idle,
    Pressed(Press),
    Waiting(Waiting),
}

/// 任一轴超出阈值即算超出（系统阈值本身就是一个矩形范围）。
fn exceeds(from: [f32; 2], to: [f32; 2], threshold: [f32; 2]) -> bool {
    (to[0] - from[0]).abs() > threshold[0] || (to[1] - from[1]).abs() > threshold[1]
}

pub struct GestureMachine {
    thresholds: GestureThresholds,
    state: State,
}

impl GestureMachine {
    pub fn new(thresholds: GestureThresholds) -> Self {
        Self {
            thresholds,
            state: State::Idle,
        }
    }

    /// 左键按下。等待单击时：窗口内且距离内 → 成为候选第二下；否则先结算
    /// 第一下（产出 Click），这次按下重新判断。已有按下未松开时忽略新按下。
    pub fn pointer_down(
        &mut self,
        pos: [f32; 2],
        frame_width: f32,
        displayed: Option<PathBuf>,
        now_ms: u64,
    ) -> Vec<GestureOutcome> {
        let record = PressRecord {
            down_pos: pos,
            down_at: now_ms,
            zone: Zone::from_position(pos[0], frame_width),
            displayed,
        };
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Pressed(press) => {
                self.state = State::Pressed(press);
                Vec::new()
            }
            State::Waiting(waiting) => {
                let in_window = now_ms <= waiting.settle_at;
                let in_distance = !exceeds(
                    waiting.record.down_pos,
                    pos,
                    self.thresholds.double_click_dist,
                );
                if in_window && in_distance {
                    self.state = State::Pressed(Press {
                        record,
                        dragging: false,
                        second_of: Some(waiting),
                    });
                    Vec::new()
                } else {
                    let settled = waiting.record.zone;
                    self.state = State::Pressed(Press {
                        record,
                        dragging: false,
                        second_of: None,
                    });
                    vec![GestureOutcome::Click(settled)]
                }
            }
            State::Idle => {
                self.state = State::Pressed(Press {
                    record,
                    dragging: false,
                    second_of: None,
                });
                Vec::new()
            }
        }
    }

    /// 左键移动。按下后移动超过拖动阈值即成为拖动；若这是候选第二下，
    /// 成为拖动的这一刻先把第一下按单击结算（决定 01 第 4 条）。
    pub fn pointer_moved(&mut self, pos: [f32; 2]) -> Vec<GestureOutcome> {
        let State::Pressed(press) = &mut self.state else {
            return Vec::new();
        };
        if press.dragging || !exceeds(press.record.down_pos, pos, self.thresholds.drag_threshold) {
            return Vec::new();
        }
        press.dragging = true;
        match press.second_of.take() {
            Some(first) => vec![GestureOutcome::Click(first.record.zone)],
            None => Vec::new(),
        }
    }

    /// 左键松开。拖动松开不产出任何点击；候选第二下干净松开 → 双击成立，
    /// 打开第一下按下时显示的图；普通按下干净松开 → 进入等待结算。
    /// 松开位置与按下位置的位移也参与拖动判定：窗口由系统拖动期间移动事件
    /// 不送达应用，松开时的总位移是那时唯一可用的依据；这种情况下候选
    /// 第二下的第一下在这一刻按单击结算（决定 01 第 4 条）。
    pub fn pointer_up(&mut self, pos: [f32; 2]) -> Vec<GestureOutcome> {
        let State::Pressed(press) = std::mem::replace(&mut self.state, State::Idle) else {
            return Vec::new();
        };
        if press.dragging || exceeds(press.record.down_pos, pos, self.thresholds.drag_threshold) {
            return match press.second_of {
                Some(first) => vec![GestureOutcome::Click(first.record.zone)],
                None => Vec::new(),
            };
        }
        if let Some(first) = press.second_of {
            return match first.record.displayed {
                Some(path) => vec![GestureOutcome::DoubleClick(path)],
                None => Vec::new(),
            };
        }
        let settle_at = press.record.down_at + self.thresholds.double_click_ms;
        self.state = State::Waiting(Waiting {
            record: press.record,
            settle_at,
        });
        Vec::new()
    }

    /// 右键按下：取消一切未完成的左键动作（进行中的按下、等待中的单击）。
    /// 返回这次按下是否已成为拖动——是则宿主还要把相框位置回滚到按下时。
    pub fn right_pressed(&mut self) -> bool {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Pressed(press) => press.dragging,
            _ => false,
        }
    }

    /// 双击窗口结束时结算等待中的单击（由行为核心的 Tick 驱动）。
    pub fn settle_due(&mut self, now_ms: u64) -> Option<Zone> {
        let State::Waiting(waiting) = &self.state else {
            return None;
        };
        if now_ms < waiting.settle_at {
            return None;
        }
        let State::Waiting(waiting) = std::mem::replace(&mut self.state, State::Idle) else {
            return None;
        };
        Some(waiting.record.zone)
    }

    /// 等待单击的结算时刻（宿主据此安排唤醒，不必空转）。
    pub fn settle_deadline(&self) -> Option<u64> {
        match &self.state {
            State::Waiting(waiting) => Some(waiting.settle_at),
            _ => None,
        }
    }
}
