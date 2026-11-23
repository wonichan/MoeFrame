//! 持久化同步（architecture-deepening 工单 04）：「哪个值以谁为准、
//! 何时写回」只有一个出处。两头收：
//! (a) 启动翻译——配置经这里翻成一个启动包（核心构造入参 + 宿主
//! 初值 + 自启对账结果），run() 只做分发，不再逐字段翻译；
//! (b) 全部镜像写回——核心的 Save 系列效果与宿主窗口几何、开机自启
//! 镜像都经这里落盘，宿主不再直接调用 ConfigStore 的 setter。ConfigStore
//! 不合并，降为本 module 背后的文件 adapter（字段级修复与手改保留仍是它的事）。
//!
//! 三种镜像模式在本 module 内显式成文，写回口径一致：
//! - 核心状态镜像：核心是准，Save 系列效果是把核心状态写穿到配置
//!   （文件夹、间隔、播放模式、切换动画、续播状态）；
//! - 宿主状态镜像：宿主状态为准，写回只记录窗口当前几何与外观模式；
//! - 注册表反向对齐：系统注册表是准，配置里的 autostart 只是镜像，
//!   启动时对账，不一致就把镜像改写为实际值（决策仍归 autostart
//!   module 的 AutostartSync，本 module 只做装配与镜像落盘）。
//!
//! 写回失败统一口径（三种镜像一致）：不阻塞行为、不弹窗、不重试，
//! 记一行到现有日志文件；镜像偏差靠下一次启动的对账/字段修复收敛。
//! 两套秒数规则（草稿解析与配置修复）不在这里收拢，ADR-0003 的
//! 区分保持不变。

use crate::{
    app_core::{CoreArgs, ResumeData},
    autostart::{AutostartStore, AutostartSync},
    config::{Animation, AppearanceMode, BuiltinFrame, ConfigStore, PlaybackMode, WindowConfig},
    instance::initial_position,
};
use std::{
    io,
    path::{Path, PathBuf},
};

/// 窗口几何的落盘记录：（x, y, 宽, 高），x/y 可缺（首次启动）。
pub type GeometryRecord = (Option<i64>, Option<i64>, u32, u32);

/// 宿主初值包：启动翻译从配置算出的宿主侧初始状态。
#[derive(Clone, Debug, PartialEq)]
pub struct HostInit {
    /// 窗口配置原文：开窗尺寸与位置的来源。
    pub window: WindowConfig,
    /// 落盘记录以存档为准：防抖保存的比较基准。
    pub saved_geom: GeometryRecord,
    /// 存档位置（两坐标齐全时）；None 表示首次启动走默认放置。
    pub saved_position: Option<[f32; 2]>,
    /// 外观模式（V2 相框外观专题工单 01）：宿主持有当前值，
    /// 设置窗切换经 save_appearance_mode 写回。
    pub appearance: AppearanceMode,
    /// 内置画框参数（V2 相框外观专题工单 02）：宿主持有当前值，
    /// 设置窗调整经 save_builtin_frame 写回。
    pub frame: BuiltinFrame,
    /// 自定义相框图的路径（V2 相框外观专题工单 03）：宿主持有，
    /// 启动时据此分析一次；设置窗选新图经 save_frame_art_path 写回。
    pub frame_art: Option<PathBuf>,
    /// 本进程的实例编号（多实例吸附专题工单 01）：宿主装配与日志
    /// 辨识用；几何与图库的读写段由 ConfigStore 按此编号定位。
    pub instance_number: u32,
    /// 窗口吸附全局开关（多实例吸附专题工单 02）：宿主持有当前值，
    /// 设置窗切换经 save_window_snap 写回；判定逻辑只读本进程这份
    /// 启动值与自身切换（跨实例实时同步归工单 04）。
    pub window_snap: bool,
}

/// 启动包：run() 拿到后只分发，不再逐字段翻译配置。
pub struct StartupPackage {
    pub core: CoreArgs,
    pub host: HostInit,
    pub autostart: AutostartSync,
}

/// 持久化同步：持有文件 adapter 与日志路径，全部镜像写回经它。
pub struct ConfigSync {
    store: ConfigStore,
    log_path: PathBuf,
}

impl ConfigSync {
    /// 启动装配（1 号实例口径）：见 startup_for_instance。
    pub fn startup(
        config_path: &Path,
        log_path: &Path,
        autostart_store: &mut dyn AutostartStore,
    ) -> (Self, StartupPackage) {
        Self::startup_for_instance(config_path, log_path, autostart_store, 1)
    }

    /// 启动装配：按实例编号加载并修复配置（图库与几何取该编号的
    /// 实例段）、完成开机自启的注册表↔镜像对账（实际状态优先，
    /// 不一致时回写镜像，回写也走统一失败口径），再把配置翻译成
    /// 启动包。初始位置口径（多实例吸附专题工单 01）：本实例段有
    /// 落盘位置时严格按落盘开；没有时相对上一扇（编号小于本实例
    /// 且有落盘位置的最大编号段）+24/+24 级联，级联结果只作初始
    /// 放置（saved_geom 的位置部分），落盘仍等本实例自己的几何
    /// 变化经既有防抖/退出冲刷写回，钳制走几何机既有工作区口径。
    pub fn startup_for_instance(
        config_path: &Path,
        log_path: &Path,
        autostart_store: &mut dyn AutostartStore,
        instance_number: u32,
    ) -> (Self, StartupPackage) {
        let mut sync = Self {
            store: ConfigStore::load_for_instance(config_path, instance_number),
            log_path: log_path.to_path_buf(),
        };
        let config = sync.store.config();
        // 开机自启启动对齐（工单 21）：实际注册表状态优先于配置镜像。
        let (autostart, rewrite) =
            AutostartSync::startup(autostart_store, config.general.autostart);
        if let Some(value) = rewrite {
            sync.save_autostart_mirror(value);
        }
        let config = sync.store.config();
        let core = CoreArgs {
            saved_folder: config.image_folder.as_ref().map(PathBuf::from),
            interval_secs: config.playback.interval_secs,
            mode: config.playback.mode,
            animation: config.playback.animation,
            resume: resume_data(config),
        };
        let window = config.window.clone();
        let saved_position = match (window.x, window.y) {
            (Some(x), Some(y)) => Some([x as f32, y as f32]),
            _ => None,
        };
        // 上一扇的位置：编号小于本实例、有落盘位置的最大编号段。
        let previous_position = config
            .instances
            .iter()
            .filter(|instance| instance.number < instance_number)
            .filter_map(|instance| match (instance.window.x, instance.window.y) {
                (Some(x), Some(y)) => Some((instance.number, [x as f32, y as f32])),
                _ => None,
            })
            .max_by_key(|(number, _)| *number)
            .map(|(_, position)| position);
        let initial = initial_position(saved_position, previous_position);
        let saved_geom = match (saved_position, initial) {
            (None, Some(position)) => (
                Some(position[0] as i64),
                Some(position[1] as i64),
                window.width,
                window.height,
            ),
            _ => (window.x, window.y, window.width, window.height),
        };
        let host = HostInit {
            saved_geom,
            saved_position: initial,
            appearance: config.display.appearance,
            frame: config.display.frame,
            frame_art: config.display.frame_art.as_ref().map(PathBuf::from),
            window,
            instance_number,
            window_snap: config.general.window_snap,
        };
        (
            sync,
            StartupPackage {
                core,
                host,
                autostart,
            },
        )
    }

    // ---- 核心状态镜像（核心的 Save 系列效果） ----

    pub fn save_folder(&mut self, folder: &Path) {
        let text = folder.to_string_lossy().into_owned();
        let result = self.store.set_image_folder(Some(&text));
        self.note_write_result("image_folder", result);
    }

    pub fn save_interval(&mut self, secs: i64) {
        let result = self.store.set_interval_secs(secs);
        self.note_write_result("interval_secs", result);
    }

    pub fn save_playback_mode(&mut self, mode: PlaybackMode) {
        let result = self.store.set_playback_mode(mode);
        self.note_write_result("playback_mode", result);
    }

    /// 切换动画档位写回（切换动画专题工单 01）：核心改档即落盘。
    pub fn save_animation(&mut self, animation: Animation) {
        let result = self.store.set_animation(animation);
        self.note_write_result("animation", result);
    }

    /// 续播状态落盘：路径以字符串保存，无法表示为 UTF-8 的当前图
    /// 整份跳过（不崩溃），进度与历史中的这类路径逐条滤掉。
    pub fn save_resume_state(&mut self, current: &Path, seen: &[PathBuf], history: &[PathBuf]) {
        let Some(current) = current.to_str() else {
            return;
        };
        let seen = paths_to_strings(seen);
        let history = paths_to_strings(history);
        let result = self.store.save_resume_state(Some(current), &seen, &history);
        self.note_write_result("resume_state", result);
    }

    pub fn clear_resume_state(&mut self) {
        let result = self.store.save_resume_state(None, &[], &[]);
        self.note_write_result("resume_state", result);
    }

    // ---- 宿主状态镜像 ----

    pub fn save_window_geometry(&mut self, geom: GeometryRecord) {
        let result = self
            .store
            .set_window_geometry(geom.0, geom.1, geom.2, geom.3);
        self.note_write_result("window_geometry", result);
    }

    /// 外观模式写回（V2 相框外观专题工单 01）：宿主切换即落盘。
    pub fn save_appearance_mode(&mut self, mode: AppearanceMode) {
        let result = self.store.set_appearance_mode(mode);
        self.note_write_result("appearance", result);
    }

    /// 内置画框参数写回（V2 相框外观专题工单 02）：宿主调整即落盘。
    pub fn save_builtin_frame(&mut self, frame: BuiltinFrame) {
        let result = self.store.set_builtin_frame(frame);
        self.note_write_result("builtin_frame", result);
    }

    /// 窗口吸附开关写回（多实例吸附专题工单 02）：宿主切换即落盘，
    /// 只改全局段的 window_snap 一个键。
    pub fn save_window_snap(&mut self, enabled: bool) {
        let result = self.store.set_window_snap(enabled);
        self.note_write_result("window_snap", result);
    }

    /// 相框图路径写回（V2 相框外观专题工单 03）：宿主选定即落盘，
    /// 只存路径不复制文件。
    pub fn save_frame_art_path(&mut self, path: Option<&Path>) {
        let text = path.map(|path| path.to_string_lossy().into_owned());
        let result = self.store.set_frame_art_path(text.as_deref());
        self.note_write_result("frame_art", result);
    }

    // ---- 多实例收口（工单 04）：在运行标记、全局同步与恢复裁决 ----

    /// 「在运行」标记写回（工单 04）：启动置位、正常退出清除，走
    /// ConfigStore 的写入纪律通道只改本实例段的 running 键。
    pub fn set_running_marker(&mut self, running: bool) {
        let result = self.store.set_instance_running(running);
        self.note_write_result("running_marker", result);
    }

    /// 全部实例段的当前快照（工单 04 自启恢复裁决的输入）：取内存
    /// 视图——启动时即磁盘口径，恢复裁决只在启动瞬间做一次。
    pub fn instance_sections(&self) -> Vec<crate::config::InstanceConfig> {
        self.store.config().instances.clone()
    }

    /// 当前全局段快照（工单 04）：同步状态机的启动基线取它。
    pub fn global_settings(&self) -> crate::global_sync::GlobalSettings {
        crate::global_sync::GlobalSettings::from_config(self.store.config())
    }

    /// 全局同步的只读重读（工单 04）：只换全局段、不碰实例段、
    /// 不写回，口径在 ConfigStore::reload_globals。
    pub fn reload_globals(&mut self) -> crate::global_sync::GlobalSettings {
        self.store.reload_globals()
    }

    /// 配置文件路径（工单 04）：宿主按它 stat 修改时间驱动同步轮询。
    pub fn config_path(&self) -> &Path {
        self.store.path()
    }

    // ---- 注册表反向对齐的镜像落盘 ----

    pub fn save_autostart_mirror(&mut self, value: bool) {
        let result = self.store.set_autostart(value);
        self.note_write_result("autostart", result);
    }

    /// 退出收尾（工单 20 的退出冲刷）：把最后观察到的几何一次结清——
    /// 与落盘记录不同才写；返回结清后的落盘记录供宿主同步基准。
    pub fn shutdown(
        &mut self,
        observed: Option<GeometryRecord>,
        saved: GeometryRecord,
    ) -> GeometryRecord {
        match observed {
            Some(geom) if geom != saved => {
                self.save_window_geometry(geom);
                geom
            }
            _ => saved,
        }
    }

    /// 统一失败口径：写回结果到这里汇合——成功即过，失败不阻塞、
    /// 不弹窗、不重试，只记一行到现有日志文件。
    fn note_write_result(&mut self, what: &str, result: io::Result<()>) {
        if let Err(error) = result {
            let _ = append_log_line(
                &self.log_path,
                &format!("config write failed ({what}): {error}"),
            );
        }
    }
}

/// 续播数据装配（工单 13）：三字段全空视为无恢复数据；任一非空即
/// 按字段原样交给核心，字段级校验在恢复构造里做。
fn resume_data(config: &crate::config::Config) -> Option<ResumeData> {
    let general = &config.general;
    if general.current_image.is_none()
        && general.round_seen.is_empty()
        && general.history.is_empty()
    {
        return None;
    }
    Some(ResumeData {
        current: general.current_image.clone().map(PathBuf::from),
        seen: general.round_seen.iter().map(PathBuf::from).collect(),
        history: general.history.iter().map(PathBuf::from).collect(),
    })
}

/// 路径列表转可落盘的字符串列表；无法表示为 UTF-8 的路径跳过。
fn paths_to_strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| path.to_str().map(str::to_owned))
        .collect()
}

/// 向现有日志文件追加一行（格式与跳过日志同源：毫秒时间戳 + 内容）。
fn append_log_line(log_path: &Path, line: &str) -> io::Result<()> {
    use std::io::Write;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    writeln!(file, "{unix_ms}\t{line}")
}
