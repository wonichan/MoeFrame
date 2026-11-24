//! 全局设置跨实例同步的纯判定（多实例吸附专题工单 04，spec 第 11
//! 节）：任一实例改了全局设置，其他实例轮询 `config.toml` 的修改
//! 时间、发现变化时重读磁盘、只应用全局段新值。同步是只读应用，
//! 不碰任何实例段、不触发写回（写回仍只走工单 01 的写入纪律）。
//!
//! 轮询成本口径（与工单 03 的轮询成本口径协调，定死）：本状态机
//! 不产生任何唤醒——宿主只在既有调度已经唤醒的帧上顺手 stat 一次
//! 文件修改时间（多实例时工单 03 的跟随心跳 1000ms 帧天然承载它；
//! 单实例静止时靠 incidental 帧），不新增定时器、不把静止零重绘顶
//! 成常开。stat 本身只是一次元数据查询，成本可忽略。

use crate::config::{Animation, AppearanceMode, BuiltinFrame, Config, PlaybackMode};
use std::time::SystemTime;

/// 全局段的一份快照：跨实例同步的比较与应用都以它为单位。含
/// recursive 与 general 里的续播状态之外的全部全局键——快照求全
/// 是为了「全局段变没变」的判定不漏项；宿主实际应用的只是它持有
/// 的那部分（外观、画框、相框图、窗口吸附开关与核心的播放三项），
/// autostart 以注册表实际状态为准（工单 21 口径）、续播状态归各
/// 实例自己的显示流程，都不在同步应用之列。
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalSettings {
    pub recursive: bool,
    pub mode: PlaybackMode,
    pub interval_secs: i64,
    /// P1：只保存，不驱动行为（与 config 口径一致）。
    pub pause_on_hover: bool,
    pub animation: Animation,
    /// P1：只保存，不驱动行为。
    pub show_filename: bool,
    pub appearance: AppearanceMode,
    pub frame: BuiltinFrame,
    pub frame_art: Option<String>,
    pub autostart: bool,
    pub window_snap: bool,
}

impl GlobalSettings {
    /// 从一份配置视图取全局段快照（实例段字段不入快照）。
    pub fn from_config(config: &Config) -> Self {
        Self {
            recursive: config.recursive,
            mode: config.playback.mode,
            interval_secs: config.playback.interval_secs,
            pause_on_hover: config.playback.pause_on_hover,
            animation: config.playback.animation,
            show_filename: config.display.show_filename,
            appearance: config.display.appearance,
            frame: config.display.frame,
            frame_art: config.display.frame_art.clone(),
            autostart: config.general.autostart,
            window_snap: config.general.window_snap,
        }
    }
}

/// 同步轮询状态机：记住上次见到的修改时间与上次应用的全局快照。
/// 文件未变不发信号（不重复应用）；文件变了但全局段没变（例如对
/// 方只写了自己的实例段几何）时，apply_reloaded 返回 None，同样
/// 不重复应用。
pub struct GlobalSyncState {
    last_mtime: Option<SystemTime>,
    applied: GlobalSettings,
}

impl GlobalSyncState {
    /// 以启动时已应用的全局值与启动时文件的修改时间为基线：基线
    /// 之前的变化已包含在启动值里，不重复应用。
    pub fn new(applied: GlobalSettings, baseline_mtime: Option<SystemTime>) -> Self {
        Self {
            last_mtime: baseline_mtime,
            applied,
        }
    }

    /// 宿主每帧（既有唤醒帧）喂入当前修改时间：与上次不同才返回
    /// true（该重读了），并把基线推进到新值——同一修改时间只报
    /// 一次。取不到修改时间（文件暂缺等）返回 false 且不推进基线。
    pub fn mtime_changed(&mut self, mtime: Option<SystemTime>) -> bool {
        match mtime {
            Some(mtime) if Some(mtime) != self.last_mtime => {
                self.last_mtime = Some(mtime);
                true
            }
            _ => false,
        }
    }

    /// 应用一份重读到的全局快照：与上次应用值不同才返回新值供宿
    /// 主落地，并更新已应用基线；相同返回 None。
    pub fn apply_reloaded(&mut self, reloaded: GlobalSettings) -> Option<GlobalSettings> {
        if reloaded == self.applied {
            None
        } else {
            self.applied = reloaded.clone();
            Some(reloaded)
        }
    }

    /// 当前已应用的全局快照（宿主比对单项差异时用）。
    pub fn applied(&self) -> &GlobalSettings {
        &self.applied
    }
}
