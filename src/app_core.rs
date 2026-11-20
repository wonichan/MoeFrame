//! 相框应用行为核心（工单 10/11）：不依赖 GUI 与线程的用户意图 → 状态/效果转换。
//!
//! 宿主（二进制）负责执行效果（弹文件夹选择框、扫描、后台加载、保存配置、退出），
//! 并把结果作为意图送回。核心本身不做任何 IO，因此行为测试可完整驱动这条接缝。
//! 时间由意图携带的单调毫秒值推进（宿主取其 UI 时钟），核心不读系统时间。
//! 顺序与随机（工单 12）共用同一套计时与失败语义；随机性经 `RandomSource` 接缝注入，
//! 宿主提供系统随机，测试提供确定性实现。

use crate::{
    config::PlaybackMode,
    playlist::{Direction, Playlist, RandomSource},
};
use std::path::PathBuf;

/// 用户与宿主送入的意图/结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    /// 宿主完成启动装配后送入一次。
    Started,
    /// 菜单「选择文件夹」。
    FolderPickRequested,
    /// 文件夹选择完成。
    FolderPicked(PathBuf),
    /// 文件夹选择被取消。
    FolderPickCancelled,
    /// 宿主执行完扫描的效果回送。
    ScanCompleted(Vec<PathBuf>),
    /// 宿主执行完加载的效果回送：成功显示的路径，或全部候选失败。
    LoadCompleted { shown: Option<PathBuf>, now_ms: u64 },
    /// 上一张（菜单、左方向键、滚轮向上共用同一意图）。
    PrevRequested,
    /// 下一张（菜单、右方向键、滚轮向下共用同一意图）。
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
    SaveInterval(i64),
    SavePlaybackMode(PlaybackMode),
    Exit,
}

/// 相框当前呈现给用户的视图。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    /// 无图状态：可操作，提示右键选择文件夹。
    Empty,
    Showing(PathBuf),
}

/// 导航的落位方式：新鲜选择记账，历史回看/返回只动游标。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavKind {
    Fresh,
    Back,
    Forward,
}

/// 一次在飞加载的性质，决定失败回送如何收尾。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    /// 扫描后的首图加载：全失败即无图状态。
    Initial,
    /// 导航加载：全失败保留当前图；auto 区分自动轮播与手动切换的计时处理。
    Navigate { auto: bool, kind: NavKind },
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
    /// 加载在飞时新到的导航意图只留最后一个，加载收尾后接着执行。
    queued_nav: Option<Direction>,
    /// 在飞期间改过间隔：失败收尾时也要按新间隔重计（成功与自动失败路径天然重计）。
    recount_on_settle: bool,
    rng: Box<dyn RandomSource>,
}

impl FrameCore {
    pub fn new(
        saved_folder: Option<PathBuf>,
        interval_secs: i64,
        mode: PlaybackMode,
        rng: Box<dyn RandomSource>,
    ) -> Self {
        Self {
            folder: saved_folder,
            playlist: Playlist::new(Vec::new(), mode),
            playing: true,
            interval_secs,
            deadline_ms: None,
            pending: None,
            queued_nav: None,
            recount_on_settle: false,
            rng,
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

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn interval_secs(&self) -> i64 {
        self.interval_secs
    }

    pub fn playback_mode(&self) -> PlaybackMode {
        self.playlist.mode()
    }

    /// 下一次自动切换时刻，供宿主安排重绘唤醒。
    pub fn next_deadline_ms(&self) -> Option<u64> {
        self.deadline_ms
    }

    fn interval_ms(&self) -> u64 {
        (self.interval_secs.max(1) as u64) * 1000
    }

    fn navigate(&mut self, direction: Direction, auto: bool) -> Vec<Effect> {
        if self.pending.is_some() {
            // 加载期间保留当前图；手动导航不丢弃，留最后一个，收尾后执行。
            if !auto {
                self.queued_nav = Some(direction);
            }
            return Vec::new();
        }
        let (candidates, kind) = match (self.playlist.mode(), direction) {
            (PlaybackMode::Sequential, _) => {
                (self.playlist.rotation_candidates(direction), NavKind::Fresh)
            }
            (PlaybackMode::Random, Direction::Prev) => {
                (self.playlist.history_back_candidates(), NavKind::Back)
            }
            (PlaybackMode::Random, Direction::Next) => {
                // 回看中先沿历史返回最新端；无路可返时才作新鲜选择。
                let forward = self.playlist.history_forward_candidates();
                if self.playlist.history_cursor().is_some() && !forward.is_empty() {
                    (forward, NavKind::Forward)
                } else {
                    (self.playlist.bag_candidates(&mut *self.rng), NavKind::Fresh)
                }
            }
        };
        if candidates.is_empty() {
            return Vec::new();
        }
        self.pending = Some(Pending::Navigate { auto, kind });
        vec![Effect::Load { candidates }]
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
                Some(folder) => vec![Effect::Scan(folder.clone())],
                // 首次无配置：只引导选择这一次。
                None => vec![Effect::PickFolder],
            },
            Intent::FolderPickRequested => vec![Effect::PickFolder],
            Intent::FolderPicked(folder) => {
                self.folder = Some(folder.clone());
                vec![Effect::SaveFolder(folder.clone()), Effect::Scan(folder)]
            }
            // 取消不改变任何现状：无图时停在无图状态，显示中则继续显示，且不再自动弹窗。
            Intent::FolderPickCancelled => Vec::new(),
            Intent::ScanCompleted(paths) => {
                self.playlist = Playlist::new(paths, self.playlist.mode());
                self.deadline_ms = None;
                self.queued_nav = None;
                self.recount_on_settle = false;
                if self.playlist.is_empty() {
                    self.pending = None;
                    Vec::new()
                } else {
                    self.pending = Some(Pending::Initial);
                    // 随机模式的首图也是新选择：候选来自洗牌袋，而非自然序（工单 12）。
                    let candidates = match self.playlist.mode() {
                        PlaybackMode::Random => self.playlist.bag_candidates(&mut *self.rng),
                        PlaybackMode::Sequential => self.playlist.items().to_vec(),
                    };
                    vec![Effect::Load { candidates }]
                }
            }
            Intent::LoadCompleted { shown, now_ms } => {
                let pending = self.pending.take().unwrap_or(Pending::Initial);
                let recount = std::mem::take(&mut self.recount_on_settle);
                let mut effects = match shown {
                    Some(path) => {
                        // 落位方式由在飞导航的性质决定：新鲜选择记账，历史往返只动游标。
                        let landed = match pending {
                            Pending::Navigate {
                                kind: NavKind::Back,
                                ..
                            } => self.playlist.land_history(&path, Direction::Prev),
                            Pending::Navigate {
                                kind: NavKind::Forward,
                                ..
                            } => self.playlist.land_history(&path, Direction::Next),
                            _ => self.playlist.record_fresh(&path, &mut *self.rng),
                        };
                        if !landed {
                            self.playlist.clear_current();
                            self.deadline_ms = None;
                            Vec::new()
                        } else {
                            // 成功切换（首图或导航）都从完成时刻计满一个间隔。
                            self.deadline_ms = if self.playing {
                                Some(now_ms + self.interval_ms())
                            } else {
                                None
                            };
                            match self.playlist.preload_target(&mut *self.rng) {
                                Some(next) => vec![Effect::Preload { path: next }],
                                None => Vec::new(),
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
                if !self.playing || self.pending.is_some() || self.playlist.len() < 2 {
                    return Vec::new();
                }
                match self.deadline_ms {
                    Some(deadline) if now_ms >= deadline => self.navigate(Direction::Next, true),
                    _ => Vec::new(),
                }
            }
            Intent::ExitRequested => vec![Effect::Exit],
        }
    }
}
