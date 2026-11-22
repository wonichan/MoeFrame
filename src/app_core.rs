//! 相框应用行为核心（工单 10/11）：不依赖 GUI 与线程的用户意图 → 状态/效果转换。
//!
//! 宿主（二进制）负责执行效果（弹文件夹选择框、扫描、后台加载、保存配置、退出），
//! 并把结果作为意图送回。核心本身不做任何 IO，因此行为测试可完整驱动这条接缝。
//! 时间由意图携带的单调毫秒值推进（宿主取其 UI 时钟），核心不读系统时间。
//! 顺序与随机（工单 12）共用同一套计时与失败语义；导航决策（候选计算、
//! 落位方式、落位记账）归播放序列（工单 03 深挖），随机源由播放序列
//! 持有，核心只管时序、在飞与转场状态。
//! 跨启动恢复（工单 13）：启动时宿主把配置里保存的当前图、本轮进度与历史经
//! `ResumeData` 送入，首次扫描后按模式与当前图是否仍在图库决定首图；每次成功
//! 显示都经 `SaveResumeState` 效果请宿主落盘（预加载与失败不落盘）。
//! 导航加载成功后核心发送完成事件供宿主提交显示；宿主立即回送完成，所有滑动动画关闭，转场等待态只维持到加载成功。转场期间的新导航意图仍只保留最后一个。
//! 指针手势（工单 15）：按下/移动/松开/右键事件经手势机判定拖动/取消；单击不切图，双击仍打开原图，右键取消未完成动作并在拖动中产出位置回滚效果。

use crate::{
    config::PlaybackMode,
    gesture::{GestureMachine, GestureOutcome, GestureThresholds},
    playlist::{Direction, Landing, Playlist, RescanPlan, SystemRandom},
};
use std::path::PathBuf;

/// 行为核心的构造入参包（深挖工单 04）：由 config_sync 的启动翻译
/// 从配置产出，run() 只分发；测试可直接按字段构造。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreArgs {
    pub saved_folder: Option<PathBuf>,
    pub interval_secs: i64,
    pub mode: PlaybackMode,
    pub resume: Option<ResumeData>,
}

/// 跨启动保存的续播状态（工单 13）：当前图、本轮进度与历史，只存路径。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResumeData {
    pub current: Option<PathBuf>,
    pub seen: Vec<PathBuf>,
    pub history: Vec<PathBuf>,
}

/// 用户与宿主送入的意图/结果（手势意图带浮点坐标，故只 PartialEq）。
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// 宿主完成启动装配后送入一次。
    Started,
    /// 菜单「选择文件夹」。
    FolderPickRequested,
    /// 文件夹选择完成。
    FolderPicked(PathBuf),
    /// 菜单「重新扫描」（工单 16）：对当前文件夹重扫并合并图库增删。
    RescanRequested,
    /// 文件夹选择被取消。
    FolderPickCancelled,
    /// 宿主执行完扫描的效果回送。
    ScanCompleted(Vec<PathBuf>),
    /// 宿主执行完加载的效果回送：成功显示的路径，或全部候选失败。
    LoadCompleted { shown: Option<PathBuf>, now_ms: u64 },
    /// 宿主渲染完一段滑动转场的效果回送：核心此刻才提交这次显示（工单 14）。
    TransitionCompleted { now_ms: u64 },
    /// 左键在内容区按下（工单 15）：位置为窗口内逻辑坐标，附按下时的相框宽度。
    PointerDown {
        pos: [f32; 2],
        frame_width: f32,
        now_ms: u64,
    },
    /// 左键移动（工单 15）。
    PointerMoved { pos: [f32; 2] },
    /// 左键松开（工单 15）：附松开位置，位移超阈值同样按拖动处理。
    PointerUp { pos: [f32; 2], now_ms: u64 },
    /// 右键按下（工单 15）：取消未完成的左键动作；菜单由宿主照常打开。
    RightPressed,
    /// 上一张（菜单与左方向键共用同一意图）。
    PrevRequested,
    /// 下一张（菜单与右方向键共用同一意图）。
    NextRequested,
    /// 播放/暂停切换（菜单与空格共用同一意图）。
    PlayPauseToggled { now_ms: u64 },
    /// 在菜单里选定新的轮播间隔（秒）。
    IntervalSelected { secs: i64, now_ms: u64 },
    /// 在菜单里切换播放模式（顺序/随机二选一）。
    PlaybackModeSelected(PlaybackMode),
    /// 宿主的周期时钟推进，驱动自动轮播判定。
    Tick { now_ms: u64 },
    /// 菜单「退出」。
    ExitRequested,
    /// 关闭相框（标题栏关闭、Alt+F4）：只隐藏，不退出（工单 17）。
    CloseRequested,
    /// 托盘左键单击或托盘菜单「显示/关闭相框」：按焦点三分支召回
    /// （tray-only 工单 01）：隐藏 → 显示；显示且有焦点 → 隐藏；显示
    /// 但无焦点 → 只唤回聚焦。focused 是宿主在点击帧采样的视口焦点，
    /// 未知按无焦点处理（只唤回不误藏）。
    ToggleVisibilityRequested { focused: bool },
    /// 托盘菜单「关闭相框 / 显示相框」：按标签给出的终态执行，不参与
    /// 焦点三分支。菜单弹出时相框必然失焦，若沿用 ToggleVisibilityRequested
    /// 就会被判成「只唤回」，导致菜单项点在可见但无焦点的窗口上毫无反应。
    SetFrameVisibleRequested { visible: bool },
}

/// 核心要求宿主执行的效果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    PickFolder,
    SaveFolder(PathBuf),
    Scan(PathBuf),
    /// 按候选顺序逐张尝试，首张可显示者胜出。
    Load {
        candidates: Vec<PathBuf>,
    },
    /// 当前图已显示，预加载这一张（长期只多留这一份显示数据）。
    Preload {
        path: PathBuf,
    },
    /// 双击成立：用系统默认看图器打开这张原图（工单 15）。路径是第一下
    /// 按下时正在显示的图，不随转场目标变化。
    OpenImage {
        path: PathBuf,
    },
    /// 右键在拖动中按下：宿主把相框位置恢复到该次按下时的位置（工单 15）。
    RollbackFrameDrag,
    /// 目标图已就绪：宿主开始渲染从当前图到目标图的滑动转场（工单 14）。
    /// 方向来自导航方向：Next 时目标从右向左进入，Prev 相反。
    StartTransition {
        from: PathBuf,
        to: PathBuf,
        direction: Direction,
    },
    SaveInterval(i64),
    SavePlaybackMode(PlaybackMode),
    /// 一次成功显示已提交：把当前图、本轮进度与历史落盘（工单 13）。
    SaveResumeState {
        current: PathBuf,
        seen: Vec<PathBuf>,
        history: Vec<PathBuf>,
    },
    /// 图库已空：清掉落盘的当前图、本轮进度与历史，重启不恢复已剔除状态（工单 16）。
    ClearResumeState,
    /// 隐藏相框窗口（进程与轮播继续，工单 17）。
    HideFrame,
    /// 显示相框窗口（工单 17）。
    ShowFrame,
    /// 相框已可见但无焦点：宿主把它召回到最前并聚焦，不改可见性
    /// （tray-only 工单 01）。
    RaiseFrame,
    Exit,
}

/// 相框当前呈现给用户的视图。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    /// 无图状态：可操作，提示右键打开设置、选择图库文件夹。
    Empty,
    Showing(PathBuf),
}

/// 一次在飞加载的性质，决定失败回送如何收尾。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    /// 扫描后的首图加载：全失败即无图状态。
    Initial,
    /// 导航加载：全失败保留当前图；auto 区分自动轮播与手动切换的计时处理。
    /// landing 是播放序列产出的落位标记，核心只原样携带。
    Navigate {
        auto: bool,
        landing: Landing,
        direction: Direction,
    },
}

/// 一段已开始、等待宿主渲染完成的滑动转场（工单 14）：目标图已就绪但尚未
/// 提交显示，记账、落盘与重计都等 `TransitionCompleted`。
#[derive(Clone, Debug, PartialEq, Eq)]
struct Transition {
    target: PathBuf,
    landing: Landing,
}

pub struct FrameCore {
    folder: Option<PathBuf>,
    playlist: Playlist,
    /// 默认播放中（启动显示首图后即按间隔轮播）。
    playing: bool,
    interval_secs: i64,
    /// 下一次自动切换的触发时刻；暂停或无图时为 None。
    deadline_ms: Option<u64>,
    pending: Option<Pending>,
    /// 进行中的滑动转场；转场期间不提交显示、不触发自动轮播。
    transition: Option<Transition>,
    /// 加载或转场在飞时新到的导航意图只留最后一个，收尾后接着执行。
    queued_nav: Option<Direction>,
    /// 在飞期间改过间隔：失败收尾时也要按新间隔重计（成功与自动失败路径天然重计）。
    recount_on_settle: bool,
    /// 跨启动恢复数据：首次扫描时消费；用户改选文件夹后作废。
    resume: Option<ResumeData>,
    /// 指针手势判定（工单 15）：阈值由宿主按系统设置注入。
    gesture: GestureMachine,
    /// 在飞的扫描种类（工单 16）：None 表示无扫描在飞。重扫的回送按合并
    /// 语义处理，不走首图/恢复流程；扫描在飞期间不重复发起扫描。
    scan: Option<ScanKind>,
    /// 相框当前是否可见（工单 17）：隐藏只影响可见性，播放与历史照常推进。
    frame_visible: bool,
}

/// 扫描种类：决定扫描回送走首图/恢复流程还是重扫合并（工单 16）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanKind {
    Initial,
    Rescan,
}

impl FrameCore {
    pub fn new(args: CoreArgs, gesture_thresholds: GestureThresholds) -> Self {
        Self {
            folder: args.saved_folder,
            playlist: Playlist::new(Vec::new(), args.mode, Box::new(SystemRandom::new())),
            playing: true,
            interval_secs: args.interval_secs,
            deadline_ms: None,
            pending: None,
            transition: None,
            queued_nav: None,
            recount_on_settle: false,
            resume: args.resume,
            gesture: GestureMachine::new(gesture_thresholds),
            scan: None,
            frame_visible: true,
        }
    }

    pub fn view(&self) -> View {
        match self.playlist.current_path() {
            Some(path) => View::Showing(path.clone()),
            None => View::Empty,
        }
    }

    pub fn folder(&self) -> Option<&PathBuf> {
        self.folder.as_ref()
    }

    /// 相框当前是否可见（工单 17）：宿主据此同步托盘菜单文案。
    pub fn frame_visible(&self) -> bool {
        self.frame_visible
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn interval_secs(&self) -> i64 {
        self.interval_secs
    }

    pub fn playback_mode(&self) -> PlaybackMode {
        self.playlist.mode()
    }

    /// 下一次需要唤醒的时刻（自动切换或等待单击结算），供宿主安排重绘唤醒。
    pub fn next_deadline_ms(&self) -> Option<u64> {
        match (self.deadline_ms, self.gesture.settle_deadline()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn interval_ms(&self) -> u64 {
        (self.interval_secs.max(1) as u64) * 1000
    }

    /// 单击不导航，双击打开原图。
    fn apply_gesture(&mut self, outcomes: Vec<GestureOutcome>) -> Vec<Effect> {
        outcomes
            .into_iter()
            .filter_map(|outcome| match outcome {
                GestureOutcome::Click(_) => None,
                GestureOutcome::DoubleClick(path) => Some(Effect::OpenImage { path }),
            })
            .collect()
    }

    fn navigate(&mut self, direction: Direction, auto: bool) -> Vec<Effect> {
        if self.pending.is_some() || self.transition.is_some() {
            // 加载或转场期间保留当前图；手动导航不丢弃，留最后一个，收尾后执行，
            // 不并行开启第二段转场（工单 14）。
            if !auto {
                self.queued_nav = Some(direction);
            }
            return Vec::new();
        }
        // 候选与落位方式由播放序列决定（工单 03 深挖），核心只登记在飞。
        let Some(plan) = self.playlist.plan_navigation(direction) else {
            return Vec::new();
        };
        self.pending = Some(Pending::Navigate {
            auto,
            landing: plan.landing,
            direction,
        });
        vec![Effect::Load {
            candidates: plan.candidates,
        }]
    }

    /// 一次成功显示的提交：落位记账归播放序列，核心只做计时与效果
    /// 编排（从完成时刻计满一个间隔，产出预加载与跨启动落盘效果）。
    /// 落位失败（路径不在图库）回到无图状态。
    fn commit_display(&mut self, path: &PathBuf, landing: Landing, now_ms: u64) -> Vec<Effect> {
        if !self.playlist.land(landing, path) {
            self.playlist.clear_current();
            self.deadline_ms = None;
            return Vec::new();
        }
        // 成功切换都从完成时刻计满一个间隔（转场提交以转场完成时为准）。
        self.deadline_ms = if self.playing {
            Some(now_ms + self.interval_ms())
        } else {
            None
        };
        let mut effects = match self.playlist.preload_target() {
            Some(next) => vec![Effect::Preload { path: next }],
            None => Vec::new(),
        };
        // 成功显示即提交落盘：当前图、本轮进度与历史（工单 13），
        // 内容取播放序列的续播快照。
        let snapshot = self.playlist.resume_snapshot();
        effects.push(Effect::SaveResumeState {
            current: snapshot
                .current
                .expect("a landed display has a current path"),
            seen: snapshot.seen,
            history: snapshot.history,
        });
        effects
    }

    /// 加载收尾后执行排队中的导航（若有）。
    fn drain_queued_nav(&mut self) -> Vec<Effect> {
        match self.queued_nav.take() {
            Some(direction) => self.navigate(direction, false),
            None => Vec::new(),
        }
    }

    pub fn handle(&mut self, intent: Intent) -> Vec<Effect> {
        match intent {
            Intent::Started => match &self.folder {
                // 已保存文件夹：直接扫描，不再引导选择。
                Some(folder) => {
                    self.scan = Some(ScanKind::Initial);
                    vec![Effect::Scan(folder.clone())]
                }
                // 首次无配置：只引导选择这一次。
                None => vec![Effect::PickFolder],
            },
            Intent::FolderPickRequested => vec![Effect::PickFolder],
            Intent::FolderPicked(folder) => {
                // 改选文件夹不是重启恢复：旧文件夹的恢复数据作废。
                self.resume = None;
                self.scan = Some(ScanKind::Initial);
                self.folder = Some(folder.clone());
                vec![Effect::SaveFolder(folder.clone()), Effect::Scan(folder)]
            }
            Intent::RescanRequested => match &self.folder {
                // 扫描或加载在飞时不发起（回送会冲掉在飞状态）；无文件夹无可扫。
                Some(folder) if self.scan.is_none() && self.pending.is_none() => {
                    self.scan = Some(ScanKind::Rescan);
                    vec![Effect::Scan(folder.clone())]
                }
                _ => Vec::new(),
            },
            // 取消不改变任何现状：无图时停在无图状态，显示中则继续显示，且不再自动弹窗。
            Intent::FolderPickCancelled => Vec::new(),
            Intent::ScanCompleted(paths) => {
                let was_rescan = self.scan.take() == Some(ScanKind::Rescan);
                let kept_deadline = self.deadline_ms;
                self.deadline_ms = None;
                self.queued_nav = None;
                self.transition = None;
                self.recount_on_settle = false;
                let mode = self.playlist.mode();
                if was_rescan {
                    // 手动重扫（工单 16）：增删合并与替代候选由播放序列
                    // 决定（工单 03 深挖），核心只按结果编排时序与效果。
                    self.pending = None;
                    match self.playlist.rescan(paths) {
                        RescanPlan::EmptyLibrary => {
                            // 图库已空：落盘状态同步清掉，重启不恢复已剔除的路径。
                            vec![Effect::ClearResumeState]
                        }
                        RescanPlan::Survived => {
                            // 当前图不打断：轮播时刻保持，只刷新预加载与落盘状态。
                            self.deadline_ms = kept_deadline;
                            let snapshot = self.playlist.resume_snapshot();
                            let mut effects = Vec::new();
                            if let Some(target) = self.playlist.preload_target() {
                                effects.push(Effect::Preload { path: target });
                            }
                            effects.push(Effect::SaveResumeState {
                                current: snapshot
                                    .current
                                    .expect("current confirmed by the rescan survival check"),
                                seen: snapshot.seen,
                                history: snapshot.history,
                            });
                            effects
                        }
                        RescanPlan::Missing { candidates } => {
                            // 当前图缺失：替代图成功显示后才提交状态。
                            self.pending = Some(Pending::Initial);
                            vec![Effect::Load { candidates }]
                        }
                    }
                } else if let Some(resume) = self.resume.take() {
                    // 跨启动恢复（工单 13）：播放序列按保存状态重建并
                    // 产出首图计划（恢复当前图优先，缺失按模式规则）。
                    self.playlist = Playlist::restored(
                        paths,
                        mode,
                        Box::new(SystemRandom::new()),
                        resume.current,
                        resume.seen,
                        resume.history,
                    );
                    if self.playlist.is_empty() {
                        self.pending = None;
                        return vec![Effect::ClearResumeState];
                    }
                    self.pending = Some(Pending::Initial);
                    let candidates = self.playlist.plan_resume_load();
                    vec![Effect::Load { candidates }]
                } else {
                    self.playlist = Playlist::new(paths, mode, Box::new(SystemRandom::new()));
                    if self.playlist.is_empty() {
                        self.pending = None;
                        return vec![Effect::ClearResumeState];
                    }
                    self.pending = Some(Pending::Initial);
                    let candidates = self.playlist.plan_first_load();
                    vec![Effect::Load { candidates }]
                }
            }
            Intent::LoadCompleted { shown, now_ms } => {
                let pending = self.pending.take().unwrap_or(Pending::Initial);
                let recount = std::mem::take(&mut self.recount_on_settle);
                let mut effects = match shown {
                    Some(path) => {
                        // 导航加载成功且已有当前图：先起滑动转场，完成时才提交显示
                        //（工单 14）；首图加载与无当前图的防御路径直接提交。
                        let start = match pending {
                            Pending::Navigate {
                                landing, direction, ..
                            } => self
                                .playlist
                                .current_path()
                                .cloned()
                                .map(|from| (from, landing, direction)),
                            _ => None,
                        };
                        match start {
                            Some((from, landing, direction)) => {
                                self.transition = Some(Transition {
                                    target: path.clone(),
                                    landing,
                                });
                                vec![Effect::StartTransition {
                                    from,
                                    to: path,
                                    direction,
                                }]
                            }
                            None => {
                                // 首图加载的落位由播放序列按胜出者判定
                                // （恢复当前图只落位，其余新鲜记账）。
                                let landing = match pending {
                                    Pending::Navigate { landing, .. } => landing,
                                    _ => self.playlist.initial_landing(&path),
                                };
                                self.commit_display(&path, landing, now_ms)
                            }
                        }
                    }
                    None => match pending {
                        Pending::Initial => {
                            self.playlist.clear_current();
                            self.deadline_ms = None;
                            Vec::new()
                        }
                        Pending::Navigate { auto: true, .. } => {
                            // 自动轮播全失败：保留当前图，隔满一个间隔再试，不忙循环。
                            if self.playing {
                                self.deadline_ms = Some(now_ms + self.interval_ms());
                            }
                            Vec::new()
                        }
                        Pending::Navigate { auto: false, .. } => {
                            // 手动切换全失败：不算成功切换，计时保持原样；
                            // 但在飞期间改过间隔时，按新间隔从收尾时刻重计。
                            if recount && self.playing {
                                self.deadline_ms = Some(now_ms + self.interval_ms());
                            }
                            Vec::new()
                        }
                    },
                };
                // 转场已开启时排队意图等转场完成再处理，不并行启动下一次加载。
                if self.transition.is_none() {
                    effects.extend(self.drain_queued_nav());
                }
                effects
            }
            Intent::PointerDown {
                pos,
                frame_width,
                now_ms,
            } => {
                // 双击要打开的图在第一下按下时就固定为当时正在显示的图。
                let displayed = self.playlist.current_path().cloned();
                let outcomes = self
                    .gesture
                    .pointer_down(pos, frame_width, displayed, now_ms);
                self.apply_gesture(outcomes)
            }
            Intent::PointerMoved { pos } => {
                let outcomes = self.gesture.pointer_moved(pos);
                self.apply_gesture(outcomes)
            }
            Intent::PointerUp { pos, .. } => {
                let outcomes = self.gesture.pointer_up(pos);
                self.apply_gesture(outcomes)
            }
            Intent::RightPressed => {
                // 取消未完成的左键动作；拖动中还要回滚位置。已在播的转场不取消。
                if self.gesture.right_pressed() {
                    vec![Effect::RollbackFrameDrag]
                } else {
                    Vec::new()
                }
            }
            Intent::TransitionCompleted { now_ms } => {
                // 无转场在飞的回送（宿主在重扫后取消了渲染等）直接忽略。
                let Some(transition) = self.transition.take() else {
                    return Vec::new();
                };
                let mut effects =
                    self.commit_display(&transition.target, transition.landing, now_ms);
                effects.extend(self.drain_queued_nav());
                effects
            }
            Intent::PrevRequested => self.navigate(Direction::Prev, false),
            Intent::NextRequested => self.navigate(Direction::Next, false),
            Intent::PlayPauseToggled { now_ms } => {
                self.playing = !self.playing;
                // 恢复播放从此刻计满；暂停即取消待触发的自动切换。
                self.deadline_ms = if self.playing && self.playlist.current_path().is_some() {
                    Some(now_ms + self.interval_ms())
                } else {
                    None
                };
                Vec::new()
            }
            Intent::IntervalSelected { secs, now_ms } => {
                if secs == self.interval_secs {
                    // 相同值不重计、不重写。
                    return Vec::new();
                }
                self.interval_secs = secs;
                if self.playing && self.pending.is_none() && self.playlist.current_path().is_some()
                {
                    self.deadline_ms = Some(now_ms + self.interval_ms());
                }
                if self.pending.is_some() {
                    // 在飞期间改间隔：成功/自动失败收尾会自然重计，手动失败路径靠这个标记补计。
                    self.recount_on_settle = true;
                }
                vec![Effect::SaveInterval(secs)]
            }
            Intent::PlaybackModeSelected(mode) => {
                if self.playlist.mode() == mode {
                    return Vec::new();
                }
                // 切模式不额外重置计时：截止时刻原样保留（父规格 §3/工单 12）。
                self.playlist.set_mode(mode);
                vec![Effect::SavePlaybackMode(mode)]
            }
            Intent::Tick { now_ms } => {
                let mut effects = Vec::new();
                // 双击窗口到期后清除等待态；单击本身不触发动作。
                if let Some(zone) = self.gesture.settle_due(now_ms) {
                    effects.extend(self.apply_gesture(vec![GestureOutcome::Click(zone)]));
                }
                if self.playing
                    && self.pending.is_none()
                    && self.transition.is_none()
                    && self.playlist.len() >= 2
                {
                    if let Some(deadline) = self.deadline_ms {
                        if now_ms >= deadline {
                            effects.extend(self.navigate(Direction::Next, true));
                        }
                    }
                }
                effects
            }
            Intent::ExitRequested => vec![Effect::Exit],
            Intent::CloseRequested => {
                // 关闭即隐藏（工单 17）：播放状态、当前图与截止时刻都不动。
                if self.frame_visible {
                    self.frame_visible = false;
                    vec![Effect::HideFrame]
                } else {
                    Vec::new()
                }
            }
            Intent::ToggleVisibilityRequested { focused } => {
                if !self.frame_visible {
                    self.frame_visible = true;
                    vec![Effect::ShowFrame]
                } else if focused {
                    self.frame_visible = false;
                    vec![Effect::HideFrame]
                } else {
                    vec![Effect::RaiseFrame]
                }
            }
            Intent::SetFrameVisibleRequested { visible } => {
                if visible == self.frame_visible {
                    if visible {
                        vec![Effect::RaiseFrame]
                    } else {
                        Vec::new()
                    }
                } else if visible {
                    self.frame_visible = true;
                    vec![Effect::ShowFrame]
                } else {
                    self.frame_visible = false;
                    vec![Effect::HideFrame]
                }
            }
        }
    }
}
