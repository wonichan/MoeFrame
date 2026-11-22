//! 设置窗会话机（architecture-deepening 工单 02）：整个设置窗会话——
//! 打开/关闭/重开播种、两个草稿字段的全部时序、即时控件的点选收集与
//! 应用顺序——收进一个 egui-free 状态机。宿主每帧采样事件喂进来、
//! 渲染借它的草稿文字、执行它返回的动作；定位居中留宿主。
//!
//! 时序规则（工单 19 定案，本模块是唯一实现）：打字只是草稿；失焦或
//! 回车才结算，其中失焦延迟一帧在帧尾结算，给同帧的关闭/重扫/选文件
//! 夹留否决位——关闭丢弃、重扫与选文件夹不提交，都是本模块内部的事
//! 件顺序，不靠宿主的调用顺序。外部改值不设通知通道：宿主每帧帧首递交生效值快照，
//! 基线与快照不一致的字段草稿作废重播种。原 settings_draft 的两条
//! 结算函数已吸收为本模块的内部实现，不再是公开面。

use crate::config::{
    parse_interval_draft, AppearanceMode, BuiltinFrame, FramePreset, PlaybackMode,
};

/// 带草稿的输入字段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Path,
    Seconds,
}

/// 宿主每帧递交的当前生效值快照：帧首作废比对、结算与模式点选
/// 守卫都以它为准（点选在本帧内改动的生效值由 end_frame 内部推进）。
#[derive(Clone, Debug)]
pub struct EffectiveValues {
    pub folder_text: String,
    pub interval_secs: i64,
    pub mode: PlaybackMode,
    pub appearance: AppearanceMode,
    /// 内置画框参数（工单 02）：预设与双滑杆的同值守卫以它为准。
    pub frame: BuiltinFrame,
}

/// 打开请求的裁决。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenDecision {
    /// 此前未打开：已按生效值播种，宿主开窗。
    Opened,
    /// 已打开：宿主只需把窗带到前台，状态不动。
    FocusOnly,
}

/// 宿主在渲染中收集到的一帧点选（即时控件与窗口级动作）。
#[derive(Clone, Debug, Default)]
pub struct Picks {
    pub mode: Option<PlaybackMode>,
    pub appearance: Option<AppearanceMode>,
    pub interval: Option<i64>,
    pub restore_default: bool,
    pub autostart: Option<bool>,
    pub rescan: bool,
    pub folder_pick: bool,
    pub open_image: bool,
    pub open_folder: bool,
    pub copy_path: bool,
    pub close: bool,
    /// 内置画框参数点选（工单 02）：预设单选与框宽 / 卡纸宽滑杆。
    pub frame_preset: Option<FramePreset>,
    pub frame_width: Option<f32>,
    pub mat_width: Option<f32>,
}

/// 本帧要由宿主执行的动作，按返回顺序执行。顺序即定案的应用顺序：
/// 回车结算 → 模式 → 外观 → 内置画框参数（预设 → 框宽 → 卡纸宽）→
/// 间隔预设 → 恢复默认 → 自启 → 当前图三项 →
/// 关闭/重扫/选文件夹（后三者否决待结算）→ 帧尾待结算结算。
#[derive(Clone, Debug, PartialEq)]
pub enum SessionAction {
    CommitFolder(String),
    CommitInterval(i64),
    ApplyMode(PlaybackMode),
    ApplyAppearance(AppearanceMode),
    ApplyIntervalPreset(i64),
    RestoreDefaultGeometry,
    ApplyAutostart(bool),
    OpenCurrentImage,
    OpenCurrentFolder,
    CopyCurrentPath,
    Rescan,
    PickFolder,
    CloseWindow,
    ApplyFramePreset(FramePreset),
    ApplyFrameWidth(f32),
    ApplyMatWidth(f32),
}

/// 一个草稿字段的文字与播种基线。
#[derive(Default)]
struct Draft<T> {
    text: String,
    base: T,
}

#[derive(Default)]
pub struct SettingsSession {
    open: bool,
    path: Draft<String>,
    seconds: Draft<i64>,
    /// 上一帧失焦、本帧帧尾待结算的字段。
    pending: Option<Field>,
    /// 本帧渲染中记录到的回车/失焦（帧尾消费）。
    enter: Option<Field>,
    blurred: Option<Field>,
    /// 两字段的焦点记忆：宿主的焦点跟踪每帧借此读写，视口失焦
    /// 合成失焦时按它定位字段。
    path_focused: bool,
    seconds_focused: bool,
    /// 设置窗视口上一帧是否持有系统焦点。
    viewport_focused: bool,
    /// 帧首快照的播放模式：模式点选的同值守卫用（点选应用后随之推进）。
    mode_now: Option<PlaybackMode>,
    /// 帧首快照的外观模式：外观点选的同值守卫用（与 mode_now 同规）。
    appearance_now: Option<AppearanceMode>,
    /// 帧首快照的内置画框参数：预设与滑杆点选的同值守卫用（同规）。
    frame_now: Option<BuiltinFrame>,
}

impl SettingsSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// 打开请求：未开则按生效值播种两草稿并清全部时序状态；
    /// 已开则什么都不动，宿主只聚焦。
    pub fn open_requested(&mut self, effective: &EffectiveValues) -> OpenDecision {
        if self.open {
            return OpenDecision::FocusOnly;
        }
        self.open = true;
        self.path = Draft {
            text: effective.folder_text.clone(),
            base: effective.folder_text.clone(),
        };
        self.seconds = Draft {
            text: effective.interval_secs.to_string(),
            base: effective.interval_secs,
        };
        self.pending = None;
        self.enter = None;
        self.blurred = None;
        self.path_focused = false;
        self.seconds_focused = false;
        self.viewport_focused = false;
        self.mode_now = Some(effective.mode);
        self.appearance_now = Some(effective.appearance);
        self.frame_now = Some(effective.frame);
        OpenDecision::Opened
    }

    /// 帧首：生效值快照比对——某字段的基线与快照不一致，说明生效值
    /// 已被别处（相框菜单、预设）改动，该字段草稿作废重播种，
    /// 其待结算一并取消。
    pub fn begin_frame(&mut self, effective: &EffectiveValues) {
        self.mode_now = Some(effective.mode);
        self.appearance_now = Some(effective.appearance);
        self.frame_now = Some(effective.frame);
        if self.path.base != effective.folder_text {
            self.path.base = effective.folder_text.clone();
            self.path.text = effective.folder_text.clone();
            if self.pending == Some(Field::Path) {
                self.pending = None;
            }
        }
        if self.seconds.base != effective.interval_secs {
            self.seconds.base = effective.interval_secs;
            self.seconds.text = effective.interval_secs.to_string();
            if self.pending == Some(Field::Seconds) {
                self.pending = None;
            }
        }
    }

    pub fn path_text(&self) -> &str {
        &self.path.text
    }

    pub fn path_text_mut(&mut self) -> &mut String {
        &mut self.path.text
    }

    pub fn seconds_text(&self) -> &str {
        &self.seconds.text
    }

    pub fn seconds_text_mut(&mut self) -> &mut String {
        &mut self.seconds.text
    }

    /// 焦点记忆借给宿主的焦点跟踪逐帧读写（回车抢焦点等 egui 机制
    /// 在宿主侧，记忆本身归会话）。
    pub fn path_focus_mut(&mut self) -> &mut bool {
        &mut self.path_focused
    }

    pub fn seconds_focus_mut(&mut self) -> &mut bool {
        &mut self.seconds_focused
    }

    /// 渲染中记录到某字段回车：帧尾在点选之前立即结算（焦点保留
    /// 在框内是宿主的 egui 动作，不经本模块）。
    pub fn note_enter(&mut self, field: Field) {
        self.enter = Some(field);
    }

    /// 渲染中记录到某字段失焦：帧尾记为待结算，下一帧帧尾结算。
    pub fn note_blur(&mut self, field: Field) {
        self.blurred = Some(field);
    }

    /// 帧尾：按定案顺序产出本帧动作——视口失焦合成 → 回车结算 →
    /// 点选应用 → 关闭（丢弃待结算）/ 重扫、选文件夹（否决待结算）/
    /// 待结算结算 → 记录本帧失焦为下帧待结算。
    pub fn end_frame(&mut self, picks: Picks, viewport_focused: bool) -> Vec<SessionAction> {
        let mut actions = Vec::new();
        // 点相框（另一个系统窗口）算失焦：视口焦点 true→false 且某
        // 字段还持有焦点记忆时，合成该字段的失焦（已有回车/失焦
        // 记录时不合成，与工单 19 的宿主口径一致）。
        if self.viewport_focused
            && !viewport_focused
            && self.enter.is_none()
            && self.blurred.is_none()
        {
            if self.path_focused {
                self.blurred = Some(Field::Path);
            } else if self.seconds_focused {
                self.blurred = Some(Field::Seconds);
            }
        }
        self.viewport_focused = viewport_focused;

        // 本帧的生效值在本模块内随点选推进：帧尾结算看到的是点选
        // 应用后的生效值（预设改间隔后，待结算的秒数草稿按新基线
        // 比对作废——与旧宿主在 commit 时现读生效值等价）。interval_now
        // 与 folder_now 的初值即帧首生效值：begin_frame 已把两字段的
        // 基线与快照对齐，基线就是快照。
        let mut interval_now = self.seconds.base;
        let folder_now = self.path.base.clone();

        if let Some(field) = self.enter.take() {
            if self.pending == Some(field) {
                self.pending = None;
            }
            self.settle(field, &folder_now, interval_now, &mut actions);
        }
        if let Some(mode) = picks.mode {
            // 点中已选中的模式不产生动作（旧宿主的同值守卫）。
            if self.mode_now != Some(mode) {
                self.mode_now = Some(mode);
                actions.push(SessionAction::ApplyMode(mode));
            }
        }
        if let Some(appearance) = picks.appearance {
            // 外观点选与播放模式同规：同值守卫，点选后内部推进。
            if self.appearance_now != Some(appearance) {
                self.appearance_now = Some(appearance);
                actions.push(SessionAction::ApplyAppearance(appearance));
            }
        }
        // 内置画框参数与外观点选同规：逐项同值守卫、点选后内部
        // 推进，顺序为预设 → 框宽 → 卡纸宽。
        if let Some(preset) = picks.frame_preset {
            if self.frame_now.map(|frame| frame.preset) != Some(preset) {
                if let Some(frame) = &mut self.frame_now {
                    frame.preset = preset;
                }
                actions.push(SessionAction::ApplyFramePreset(preset));
            }
        }
        if let Some(width) = picks.frame_width {
            if self.frame_now.map(|frame| frame.frame_width) != Some(width) {
                if let Some(frame) = &mut self.frame_now {
                    frame.frame_width = width;
                }
                actions.push(SessionAction::ApplyFrameWidth(width));
            }
        }
        if let Some(width) = picks.mat_width {
            if self.frame_now.map(|frame| frame.mat_width) != Some(width) {
                if let Some(frame) = &mut self.frame_now {
                    frame.mat_width = width;
                }
                actions.push(SessionAction::ApplyMatWidth(width));
            }
        }
        if let Some(secs) = picks.interval {
            interval_now = secs;
            actions.push(SessionAction::ApplyIntervalPreset(secs));
        }
        if picks.restore_default {
            actions.push(SessionAction::RestoreDefaultGeometry);
        }
        if let Some(target) = picks.autostart {
            actions.push(SessionAction::ApplyAutostart(target));
        }
        // 当前图三项是即时动作：不碰草稿与待结算，只按点选产出动作，
        // 作用对象由宿主执行时按核心当前视图取（与渲染同帧同图）。
        if picks.open_image {
            actions.push(SessionAction::OpenCurrentImage);
        }
        if picks.open_folder {
            actions.push(SessionAction::OpenCurrentFolder);
        }
        if picks.copy_path {
            actions.push(SessionAction::CopyCurrentPath);
        }
        if picks.close {
            self.pending = None;
            self.open = false;
            actions.push(SessionAction::CloseWindow);
            self.enter = None;
            self.blurred = None;
            return actions;
        }
        if picks.rescan {
            self.pending = None;
            actions.push(SessionAction::Rescan);
            self.blurred = None;
            return actions;
        }
        if picks.folder_pick {
            // 与重扫同规：选文件夹是离开草稿的动作，待结算被否决，
            // 草稿文字保留；选定生效后由快照作废重播种（宿主执行
            // PickFolder 走核心的 FolderPickRequested 既有路径）。
            self.pending = None;
            actions.push(SessionAction::PickFolder);
            self.blurred = None;
            return actions;
        }
        if let Some(field) = self.pending.take() {
            self.settle(field, &folder_now, interval_now, &mut actions);
        }
        if let Some(field) = self.blurred.take() {
            if self.pending.is_none() {
                self.pending = Some(field);
            }
        }
        actions
    }

    /// 结算一个字段的草稿（内部实现，原 settings_draft 两条结算
    /// 函数的语义）：作废时框里改显生效值；提交时产出动作并把
    /// 基线推进到提交值；同值/解析失败无动作、框里恢复生效值。
    fn settle(
        &mut self,
        field: Field,
        folder_effective: &str,
        interval_effective: i64,
        actions: &mut Vec<SessionAction>,
    ) {
        match field {
            Field::Path => {
                if self.path.base != folder_effective {
                    self.path.text = folder_effective.to_owned();
                    self.path.base = folder_effective.to_owned();
                    return;
                }
                let text = self.path.text.trim().to_owned();
                if text == folder_effective {
                    self.path.text = text;
                } else {
                    self.path.base = text.clone();
                    self.path.text = text.clone();
                    actions.push(SessionAction::CommitFolder(text));
                }
            }
            Field::Seconds => {
                if self.seconds.base != interval_effective {
                    self.seconds.text = interval_effective.to_string();
                    self.seconds.base = interval_effective;
                    return;
                }
                match parse_interval_draft(self.seconds.text.trim()) {
                    Some(value) => {
                        self.seconds.text = value.to_string();
                        self.seconds.base = value;
                        if value != interval_effective {
                            actions.push(SessionAction::CommitInterval(value));
                        }
                    }
                    None => {
                        self.seconds.text = interval_effective.to_string();
                        self.seconds.base = interval_effective;
                    }
                }
            }
        }
    }
}
