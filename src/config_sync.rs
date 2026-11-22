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
//!   （文件夹、间隔、播放模式、续播状态）；
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
    config::{AppearanceMode, BuiltinFrame, ConfigStore, PlaybackMode, WindowConfig},
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
    /// 启动装配：加载并修复配置、完成开机自启的注册表↔镜像对账
    /// （实际状态优先，不一致时回写镜像，回写也走统一失败口径），
    /// 再把配置翻译成启动包。
    pub fn startup(
        config_path: &Path,
        log_path: &Path,
        autostart_store: &mut dyn AutostartStore,
    ) -> (Self, StartupPackage) {
        let mut sync = Self {
            store: ConfigStore::load(config_path),
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
            resume: resume_data(config),
        };
        let window = config.window.clone();
        let host = HostInit {
            saved_geom: (window.x, window.y, window.width, window.height),
            saved_position: match (window.x, window.y) {
                (Some(x), Some(y)) => Some([x as f32, y as f32]),
                _ => None,
            },
            appearance: config.display.appearance,
            frame: config.display.frame,
            window,
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
