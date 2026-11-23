//! 配置契约（父规格第 7 节 / 需求第 6 节）：`%APPDATA%\MoeFrame\config.toml` 的读写与修复。
//!
//! 原则：字段级修复——坏字段回退默认并写回，合法手改值与未知键在任何保存中保留；
//! 整个文件不可解析时恢复为合法默认配置，不崩溃。两个 P1 键（`pause_on_hover`、
//! `show_filename`）只保存与修复，不驱动行为；`animation` 已由切换动画专题
//! 工单 01 转正，驱动核心转场。
//!
//! 多实例口径（多实例吸附专题工单 01）：一份文件加 `[[instances]]`
//! 实例段——每段按实例编号存该实例的 `image_folder` 与 `window`
//! 几何，其余段（recursive / playback / display / general）是全局
//! 共享设置。`Config` 里的 `image_folder` / `window` 永远指本实例
//! 那一段；写入纪律是每次写回先重读磁盘当前文档，只改自己那一段
//! 与全局段中本次确实改动的键，禁止凭内存副本整文件覆写（双开时
//! 后写者不得回滚先写者的字段）。老配置没有实例段时迁移一次：
//! 1 号继承顶层 `image_folder` 与 `[window]`，原字段随之移除。

use crate::{playlist::HISTORY_LIMIT, window_geom::Insets};
use std::{fs, io, path::Path};
use toml_edit::{value, DocumentMut, Item, Table};

pub const MIN_INTERVAL_SECS: i64 = 3;
pub const MAX_INTERVAL_SECS: i64 = 99_999_999;

pub const DEFAULT_INTERVAL_SECS: i64 = 30;

/// 框宽 / 卡纸宽两个滑杆的共同范围（逻辑 px，V2 相框外观专题
/// 工单 02）：0..=40，卡纸可为 0。
pub const MAX_LAYER_WIDTH: f32 = 40.0;
/// 框宽默认值（逻辑 px）：spec 定案的初始裱法，真机再校准。
pub const DEFAULT_FRAME_WIDTH: f32 = 14.0;
/// 卡纸宽默认值（逻辑 px）：同上。
pub const DEFAULT_MAT_WIDTH: f32 = 18.0;

/// 秒数草稿的提交解析（工单 19）：与配置读取修复是两套规则——解析失败
/// （空白、小数、非数字）返回 None，调用方保留上次有效值，绝不回退默认
/// 30；能读成整数但越界的钳制到 [MIN_INTERVAL_SECS, MAX_INTERVAL_SECS]。
/// 入参须已去掉首尾空白。超 i64 位数的纯数字串仍按整数论，按符号钳制。
pub fn parse_interval_draft(text: &str) -> Option<i64> {
    if text.is_empty() {
        return None;
    }
    match text.parse::<i64>() {
        Ok(value) => Some(value.clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS)),
        Err(_) => {
            let digits = text
                .strip_prefix('+')
                .or_else(|| text.strip_prefix('-'))
                .unwrap_or(text);
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                if text.starts_with('-') {
                    Some(MIN_INTERVAL_SECS)
                } else {
                    Some(MAX_INTERVAL_SECS)
                }
            } else {
                None
            }
        }
    }
}

const DEFAULT_TEMPLATE: &str = "\
# MoeFrame 配置文件 —— 程序自动维护，也可手动编辑（重启生效）
recursive = true

[window]
width = 480
height = 320

[playback]
mode = \"random\"
interval_secs = 30
pause_on_hover = true
animation = \"curl\"

[display]
show_filename = false
appearance = \"builtin\"
frame_preset = \"walnut\"
frame_width = 14.0
mat_width = 18.0

[general]
autostart = false
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackMode {
    Sequential,
    Random,
}

impl PlaybackMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PlaybackMode::Sequential => "sequential",
            PlaybackMode::Random => "random",
        }
    }
}

/// 切换动画（`animation` 配置键，切换动画专题工单 01 转正）：8 档，
/// 默认卷页。核心按档位驱动转场；`Random` 由核心在每次转场开始时抽成
/// 具体动画，存盘只记档位本身。`Fade`（淡化）是 2026-11-23 收编合并
/// 从对方交叉淡化收编的一档（工单 05）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Animation {
    None,
    Curl,
    Blinds,
    Ink,
    Card,
    Pixel,
    Fade,
    Random,
}

impl Animation {
    pub fn as_str(self) -> &'static str {
        match self {
            Animation::None => "none",
            Animation::Curl => "curl",
            Animation::Blinds => "blinds",
            Animation::Ink => "ink",
            Animation::Card => "card",
            Animation::Pixel => "pixel",
            Animation::Fade => "fade",
            Animation::Random => "random",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowConfig {
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            width: 480,
            height: 320,
        }
    }
}

/// 一个实例段的落盘内容（多实例吸附专题工单 01）：编号、图库与
/// 窗口几何。全局共享设置不在段里，仍在 `Config` 的其余字段。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceConfig {
    pub number: u32,
    pub image_folder: Option<String>,
    pub window: WindowConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackConfig {
    pub mode: PlaybackMode,
    pub interval_secs: i64,
    /// P1：只保存，不驱动行为。
    pub pause_on_hover: bool,
    /// 切换动画档位（切换动画专题工单 01）：核心状态镜像，核心为准。
    pub animation: Animation,
}

/// 外观模式（`appearance` 配置键，V2 相框外观专题工单 01）：极简 /
/// 内置画框 / 自定义相框图三选一，默认内置画框。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppearanceMode {
    Minimal,
    Builtin,
    Custom,
}

impl AppearanceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AppearanceMode::Minimal => "minimal",
            AppearanceMode::Builtin => "builtin",
            AppearanceMode::Custom => "custom",
        }
    }

    /// 本模式自身的静态四边内距：内置画框按画框参数供给——
    /// 单边 = 框宽 + 卡纸宽、四边相等（工单 02）；极简恒全 0；
    /// 自定义相框图无静态内距（全 0），其生效内距由宿主
    /// effective_insets 在分析成功时换成检测结果（工单 03）。
    pub fn insets(self, frame: BuiltinFrame) -> Insets {
        match self {
            AppearanceMode::Builtin => {
                let side = frame.frame_width + frame.mat_width;
                Insets {
                    left: side,
                    top: side,
                    right: side,
                    bottom: side,
                }
            }
            AppearanceMode::Minimal | AppearanceMode::Custom => Insets::ZERO,
        }
    }
}

/// 框体预设（`frame_preset` 配置键，V2 相框外观专题工单 02）：
/// 胡桃木 / 原木 / 黑 / 白四款；颜色与简易木纹质感由宿主绘制层
/// 合成，不引入图片素材。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePreset {
    Walnut,
    Oak,
    Black,
    White,
}

impl FramePreset {
    pub fn as_str(self) -> &'static str {
        match self {
            FramePreset::Walnut => "walnut",
            FramePreset::Oak => "oak",
            FramePreset::Black => "black",
            FramePreset::White => "white",
        }
    }
}

/// 内置画框参数（`frame_preset` / `frame_width` / `mat_width`
/// 配置键，V2 相框外观专题工单 02）：框体预设与框宽、卡纸宽
/// 两个滑杆值（逻辑 px，各 0..=MAX_LAYER_WIDTH，卡纸可为 0）；
/// 单边内距 = 框宽 + 卡纸宽。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuiltinFrame {
    pub preset: FramePreset,
    pub frame_width: f32,
    pub mat_width: f32,
}

impl Default for BuiltinFrame {
    fn default() -> Self {
        Self {
            preset: FramePreset::Walnut,
            frame_width: DEFAULT_FRAME_WIDTH,
            mat_width: DEFAULT_MAT_WIDTH,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayConfig {
    /// P1：只保存，不驱动行为。
    pub show_filename: bool,
    /// 外观模式（V2 相框外观专题工单 01）。
    pub appearance: AppearanceMode,
    /// 内置画框参数（V2 相框外观专题工单 02）。
    pub frame: BuiltinFrame,
    /// 自定义相框图的路径（V2 相框外观专题工单 03）：只存路径、
    /// 不复制文件（与图库文件夹同一口径），内容每次启动与切换
    /// 时重新分析。
    pub frame_art: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneralConfig {
    pub autostart: bool,
    /// 上次成功显示的图片（工单 13 跨启动恢复）。
    pub current_image: Option<String>,
    /// 本轮进度：本轮成功显示过的路径（工单 13，与历史分开保存，ADR-0002）。
    pub round_seen: Vec<String>,
    /// 实际历史：成功显示顺序，最近 512 条（工单 13）。
    pub history: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// 本实例的图库：取自本实例的实例段，不是全局字段。
    pub image_folder: Option<String>,
    pub recursive: bool,
    /// 本实例的窗口几何：取自本实例的实例段。
    pub window: WindowConfig,
    /// 本进程的实例编号（身份）：本配置视图所属的实例段编号。
    pub instance_number: u32,
    /// 全部实例段的快照（含本实例段）：供宿主找上一扇的位置等
    /// 只读用途；写回永远只改本实例段，不经这份快照落盘。
    pub instances: Vec<InstanceConfig>,
    pub playback: PlaybackConfig,
    pub display: DisplayConfig,
    pub general: GeneralConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            image_folder: None,
            recursive: true,
            window: WindowConfig::default(),
            instance_number: 1,
            instances: vec![InstanceConfig {
                number: 1,
                image_folder: None,
                window: WindowConfig::default(),
            }],
            playback: PlaybackConfig {
                mode: PlaybackMode::Random,
                interval_secs: DEFAULT_INTERVAL_SECS,
                pause_on_hover: true,
                animation: Animation::Curl,
            },
            display: DisplayConfig {
                show_filename: false,
                appearance: AppearanceMode::Builtin,
                frame: BuiltinFrame::default(),
                frame_art: None,
            },
            general: GeneralConfig {
                autostart: false,
                current_image: None,
                round_seen: Vec::new(),
                history: Vec::new(),
            },
        }
    }
}

/// 持有已解析文档的配置存储：保存时只改目标字段，其余内容（合法手改、未知键、注释）原样保留。
///
/// 写入纪律（多实例吸附专题工单 01）：内存里的文档只是上次读写的
/// 快照，绝不直接拿它覆写磁盘。每次写回都先重读磁盘当前文档、在
/// 新文档上重新解析出配置视图，再只施加本次改动后写回——另一个
/// 实例刚写下的字段（它的实例段、它改的全局键）因此原样保留。
/// 磁盘文档读不出或不可解析时才退回内存快照，避免把可读性问题
/// 变成数据丢失。
pub struct ConfigStore {
    path: std::path::PathBuf,
    document: DocumentMut,
    config: Config,
    instance_number: u32,
}

impl ConfigStore {
    /// 以 1 号实例身份读取并修复配置（单实例口径，与多实例前的
    /// 行为一致）。
    pub fn load(path: &Path) -> Self {
        Self::load_for_instance(path, 1)
    }

    /// 读取并修复配置；文件缺失或不可解析时得到默认配置。修复结果立即写回。
    /// 本实例的图库与几何取自编号对应的实例段；没有实例段的老
    /// 配置在此迁移一次（1 号继承顶层图库与 `[window]`）。
    pub fn load_for_instance(path: &Path, instance_number: u32) -> Self {
        let instance_number = instance_number.max(1);
        let mut repaired = false;
        let mut document = match fs::read_to_string(path) {
            Ok(text) => match text.parse::<DocumentMut>() {
                Ok(document) => document,
                Err(_) => {
                    repaired = true;
                    default_document()
                }
            },
            Err(_) => {
                repaired = true;
                default_document()
            }
        };
        let config = parse_document(&mut document, instance_number, &mut repaired);
        let store = Self {
            path: path.to_owned(),
            document,
            config,
            instance_number,
        };
        if repaired {
            // A failed write-back must not crash startup; the in-memory config is already legal.
            let _ = store.write();
        }
        store
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// 本存储所属的实例编号。
    pub fn instance_number(&self) -> u32 {
        self.instance_number
    }

    /// 单字段保存的共用骨架（写入纪律的唯一通道）：重读磁盘当前
    /// 文档 → 在新文档上重解析配置视图 → 只施加本次改动 → 写回。
    /// 几何落盘、设置落盘、退出冲刷与迁移写回全部经这里或 load 的
    /// 修复写回，不存在第二条整文件覆写路径。
    fn update(
        &mut self,
        apply: impl FnOnce(&mut toml_edit::DocumentMut, &mut Config),
    ) -> io::Result<()> {
        let mut document = match fs::read_to_string(&self.path) {
            Ok(text) => match text.parse::<DocumentMut>() {
                Ok(document) => document,
                Err(_) => self.document.clone(),
            },
            Err(_) => self.document.clone(),
        };
        let mut repaired = false;
        let mut config = parse_document(&mut document, self.instance_number, &mut repaired);
        apply(&mut document, &mut config);
        // apply 只改文档与视图的便捷字段；实例段快照以改后的文档为准重读。
        refresh_instance_view(&document, self.instance_number, &mut config);
        self.document = document;
        self.config = config;
        self.write()
    }

    /// 保存图片文件夹（首次选择或菜单重选时调用）：只改本实例段
    /// 的 `image_folder`，其余实例段与全局字段不动。
    pub fn set_image_folder(&mut self, folder: Option<&str>) -> io::Result<()> {
        let folder = folder.map(str::to_owned);
        let number = self.instance_number;
        self.update(|document, config| {
            let Some(entry) = instance_table_mut(document, number) else {
                return;
            };
            match &folder {
                Some(folder) => {
                    entry["image_folder"] = value(folder.clone());
                    config.image_folder = Some(folder.clone());
                }
                None => {
                    entry.remove("image_folder");
                    config.image_folder = None;
                }
            }
        })
    }

    /// 保存开机自启的配置镜像（工单 21）：真实状态在系统 Run 项里，
    /// 这里只保存与实际状态对齐后的镜像值（切换成功后或启动对齐时）。
    pub fn set_autostart(&mut self, enabled: bool) -> io::Result<()> {
        self.update(|document, config| {
            document["general"]["autostart"] = value(enabled);
            config.general.autostart = enabled;
        })
    }

    /// 保存窗口几何（工单 20）：位置与尺寸跨启动恢复；x/y 为 None 时
    /// 删除对应键（回到「无存档位置」）。只动本实例段的 window 表，
    /// 其余实例段与全局字段保留。
    pub fn set_window_geometry(
        &mut self,
        x: Option<i64>,
        y: Option<i64>,
        width: u32,
        height: u32,
    ) -> io::Result<()> {
        let number = self.instance_number;
        self.update(|document, config| {
            let Some(entry) = instance_table_mut(document, number) else {
                return;
            };
            let mut ignored = false;
            let window = ensure_table(entry, "window", &mut ignored);
            for (key, val) in [("x", x), ("y", y)] {
                match val {
                    Some(v) => window[key] = value(v),
                    None => {
                        window.remove(key);
                    }
                }
            }
            window["width"] = value(i64::from(width));
            window["height"] = value(i64::from(height));
            config.window.x = x;
            config.window.y = y;
            config.window.width = width;
            config.window.height = height;
        })
    }

    /// 保存外观模式（V2 相框外观专题工单 01）：只改 display.appearance，
    /// 其余字段原样保留。
    pub fn set_appearance_mode(&mut self, mode: AppearanceMode) -> io::Result<()> {
        self.update(|document, config| {
            document["display"]["appearance"] = value(mode.as_str());
            config.display.appearance = mode;
        })
    }

    /// 保存内置画框参数（V2 相框外观专题工单 02）：只改 display 的
    /// frame_preset / frame_width / mat_width 三键，其余字段原样保留。
    pub fn set_builtin_frame(&mut self, frame: BuiltinFrame) -> io::Result<()> {
        self.update(|document, config| {
            document["display"]["frame_preset"] = value(frame.preset.as_str());
            document["display"]["frame_width"] = value(f64::from(frame.frame_width));
            document["display"]["mat_width"] = value(f64::from(frame.mat_width));
            config.display.frame = frame;
        })
    }

    /// 保存自定义相框图的路径（V2 相框外观专题工单 03）：只改
    /// display.frame_art，None 时删键；其余字段原样保留。
    pub fn set_frame_art_path(&mut self, path: Option<&str>) -> io::Result<()> {
        let path = path.map(str::to_owned);
        self.update(|document, config| match &path {
            Some(path) => {
                document["display"]["frame_art"] = value(path.clone());
                config.display.frame_art = Some(path.clone());
            }
            None => {
                if let Some(table) = document["display"].as_table_mut() {
                    table.remove("frame_art");
                }
                config.display.frame_art = None;
            }
        })
    }

    /// 保存切换动画档位（切换动画专题工单 01）：只改 playback.animation，
    /// 其余字段原样保留。
    pub fn set_animation(&mut self, animation: Animation) -> io::Result<()> {
        self.update(|document, config| {
            document["playback"]["animation"] = value(animation.as_str());
            config.playback.animation = animation;
        })
    }

    /// 保存播放模式（工单 12）：只改 playback.mode，其余字段原样保留。
    pub fn set_playback_mode(&mut self, mode: PlaybackMode) -> io::Result<()> {
        self.update(|document, config| {
            document["playback"]["mode"] = value(mode.as_str());
            config.playback.mode = mode;
        })
    }

    /// 保存跨启动恢复状态（工单 13）：当前图、本轮进度与历史一并落盘（同属
    /// 一次成功显示的提交），历史只留最近 512 条；其余字段原样保留。
    pub fn save_resume_state(
        &mut self,
        current: Option<&str>,
        seen: &[String],
        history: &[String],
    ) -> io::Result<()> {
        let current = current.map(str::to_owned);
        let seen = seen.to_vec();
        let mut history = history.to_vec();
        keep_newest_history(&mut history);
        self.update(|document, config| {
            let seen_item = string_array_item(&seen);
            let history_item = string_array_item(&history);
            let Some(general) = document["general"].as_table_mut() else {
                return;
            };
            match &current {
                Some(path) => general["current_image"] = value(path.clone()),
                None => {
                    general.remove("current_image");
                }
            }
            general["round_seen"] = seen_item;
            general["history"] = history_item;
            config.general.current_image = current;
            config.general.round_seen = seen;
            config.general.history = history;
        })
    }

    /// 保存轮播间隔（工单 11）：只改 playback.interval_secs，其余字段原样保留。
    pub fn set_interval_secs(&mut self, secs: i64) -> io::Result<()> {
        self.update(|document, config| {
            document["playback"]["interval_secs"] = value(secs);
            config.playback.interval_secs = secs;
        })
    }

    fn write(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, self.document.to_string())
    }
}

fn default_document() -> DocumentMut {
    DEFAULT_TEMPLATE
        .parse::<DocumentMut>()
        .expect("built-in default config template must parse")
}

/// 逐字段校验：合法值保留，缺失或非法值写入默认值/钳制值并标记 repaired。
/// 实例段在此一并处理：无实例段时迁移一次（见 repair_instances），
/// 本实例段缺失时补一段默认值；返回的视图里图库与几何取本实例段。
fn parse_document(document: &mut DocumentMut, instance_number: u32, repaired: &mut bool) -> Config {
    let root = document.as_table_mut();

    let recursive = repair_bool(root, "recursive", true, repaired);
    let instances = repair_instances(root, instance_number, repaired);
    let mine = instances
        .iter()
        .find(|instance| instance.number == instance_number)
        .expect("repair_instances ensures the own section exists");
    let image_folder = mine.image_folder.clone();
    let window = mine.window.clone();

    let playback_table = ensure_table(root, "playback", repaired);
    let mode = match playback_table.get("mode").and_then(Item::as_str) {
        Some("sequential") => PlaybackMode::Sequential,
        Some("random") => PlaybackMode::Random,
        _ => {
            playback_table["mode"] = value("random");
            *repaired = true;
            PlaybackMode::Random
        }
    };
    let interval_secs = match playback_table.get("interval_secs") {
        Some(item) => match item.as_integer() {
            Some(raw) => {
                let clamped = raw.clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS);
                if clamped != raw {
                    playback_table["interval_secs"] = value(clamped);
                    *repaired = true;
                }
                clamped
            }
            None => {
                playback_table["interval_secs"] = value(DEFAULT_INTERVAL_SECS);
                *repaired = true;
                DEFAULT_INTERVAL_SECS
            }
        },
        None => {
            playback_table["interval_secs"] = value(DEFAULT_INTERVAL_SECS);
            *repaired = true;
            DEFAULT_INTERVAL_SECS
        }
    };
    let pause_on_hover = repair_bool(playback_table, "pause_on_hover", true, repaired);
    // 切换动画迁移（专题工单 01 + 收编工单 05）：none 是新旧共有的
    // 值，保留；fade 在对方两档时代是真实行为（交叉淡化），2026-11-23
    // 收编后映射到新 Fade 档，不再落卷页；旧值 slide 从未在我们的
    // 体系驱动过真实行为，与未知值一并落默认卷页并写回。
    let animation = match playback_table.get("animation").and_then(Item::as_str) {
        Some("none") => Animation::None,
        Some("curl") => Animation::Curl,
        Some("blinds") => Animation::Blinds,
        Some("ink") => Animation::Ink,
        Some("card") => Animation::Card,
        Some("pixel") => Animation::Pixel,
        Some("fade") => Animation::Fade,
        Some("random") => Animation::Random,
        _ => {
            playback_table["animation"] = value(Animation::Curl.as_str());
            *repaired = true;
            Animation::Curl
        }
    };

    let display_table = ensure_table(root, "display", repaired);
    if display_table.remove("fit_mode").is_some() {
        *repaired = true;
    }
    let show_filename = repair_bool(display_table, "show_filename", false, repaired);
    let appearance = match display_table.get("appearance").and_then(Item::as_str) {
        Some("minimal") => AppearanceMode::Minimal,
        Some("builtin") => AppearanceMode::Builtin,
        Some("custom") => AppearanceMode::Custom,
        _ => {
            display_table["appearance"] = value(AppearanceMode::Builtin.as_str());
            *repaired = true;
            AppearanceMode::Builtin
        }
    };
    let frame_preset = match display_table.get("frame_preset").and_then(Item::as_str) {
        Some("walnut") => FramePreset::Walnut,
        Some("oak") => FramePreset::Oak,
        Some("black") => FramePreset::Black,
        Some("white") => FramePreset::White,
        _ => {
            display_table["frame_preset"] = value(FramePreset::Walnut.as_str());
            *repaired = true;
            FramePreset::Walnut
        }
    };
    let frame_width =
        repair_layer_width(display_table, "frame_width", DEFAULT_FRAME_WIDTH, repaired);
    let mat_width = repair_layer_width(display_table, "mat_width", DEFAULT_MAT_WIDTH, repaired);
    // 相框图路径（工单 03）：缺失即未选（不写键）；不是字符串时
    // 删键按未选处理，不牵连同表其他字段。
    let frame_art = match display_table.get("frame_art") {
        Some(item) => match item.as_str() {
            Some(text) => Some(text.to_owned()),
            None => {
                display_table.remove("frame_art");
                *repaired = true;
                None
            }
        },
        None => None,
    };

    let general_table = ensure_table(root, "general", repaired);
    let autostart = repair_bool(general_table, "autostart", false, repaired);
    let current_image = match general_table.get("current_image") {
        Some(item) => match item.as_str() {
            Some(text) => Some(text.to_owned()),
            None => {
                general_table.remove("current_image");
                *repaired = true;
                None
            }
        },
        None => None,
    };
    // 本轮进度与历史分别校验：一个字段损坏只丢弃它自己（工单 13 准则 7）。
    let round_seen = repair_path_list(general_table, "round_seen", repaired);
    let mut history = repair_path_list(general_table, "history", repaired);
    if history.len() > HISTORY_LIMIT {
        keep_newest_history(&mut history);
        general_table["history"] = string_array_item(&history);
        *repaired = true;
    }

    Config {
        image_folder,
        recursive,
        window,
        instance_number,
        instances,
        playback: PlaybackConfig {
            mode,
            interval_secs,
            pause_on_hover,
            animation,
        },
        display: DisplayConfig {
            show_filename,
            appearance,
            frame: BuiltinFrame {
                preset: frame_preset,
                frame_width,
                mat_width,
            },
            frame_art,
        },
        general: GeneralConfig {
            autostart,
            current_image,
            round_seen,
            history,
        },
    }
}

/// 一个 window 表的字段修复（顶层迁移源与实例段共用）：x/y 非整数
/// 按无存档处理，宽高非法回默认，旧 `lock_aspect` 键移除。
fn repair_window_table(table: &mut Table, repaired: &mut bool) -> WindowConfig {
    if table.remove("lock_aspect").is_some() {
        *repaired = true;
    }
    WindowConfig {
        x: optional_i64(table, "x"),
        y: optional_i64(table, "y"),
        width: repair_dimension(table, "width", 480, repaired),
        height: repair_dimension(table, "height", 320, repaired),
    }
}

/// 实例段的修复与迁移：返回全部实例段的解析快照，并保证编号为
/// `instance_number` 的段存在（缺失时补默认段并标记 repaired）。
///
/// 迁移（只发生一次）：文档里没有 `instances` 数组时，把顶层
/// `image_folder` 与 `[window]` 按字段修复口径读出、写成 1 号段，
/// 再移除顶层两处原字段——之后文档已有实例段，本函数不会再走
/// 迁移分支。`instances` 不是数组表时整段丢弃后按无实例段处理。
/// 段内修复：编号缺失/非整数/小于 1/与先出现段重复的段整段丢弃；
/// 图库非字符串删键按未选；window 表走 repair_window_table。
fn repair_instances(
    root: &mut Table,
    instance_number: u32,
    repaired: &mut bool,
) -> Vec<InstanceConfig> {
    if !matches!(root.get("instances"), Some(Item::ArrayOfTables(_))) {
        if root.get("instances").is_some() {
            root.remove("instances");
            *repaired = true;
        }
        migrate_legacy_into_instance_one(root, repaired);
    }
    let array = root["instances"]
        .as_array_of_tables_mut()
        .expect("instances array ensured by migration");
    let mut instances: Vec<InstanceConfig> = Vec::new();
    let mut index = 0;
    while index < array.len() {
        let entry = array.get_mut(index).expect("index within array");
        let number = entry.get("number").and_then(Item::as_integer);
        let number = match number {
            Some(raw) if raw >= 1 && raw <= i64::from(u32::MAX) => raw as u32,
            _ => {
                array.remove(index);
                *repaired = true;
                continue;
            }
        };
        if instances.iter().any(|seen| seen.number == number) {
            array.remove(index);
            *repaired = true;
            continue;
        }
        let image_folder = match entry.get("image_folder") {
            Some(item) => match item.as_str() {
                Some(text) => Some(text.to_owned()),
                None => {
                    entry.remove("image_folder");
                    *repaired = true;
                    None
                }
            },
            None => None,
        };
        let window_table = ensure_table(entry, "window", repaired);
        let window = repair_window_table(window_table, repaired);
        instances.push(InstanceConfig {
            number,
            image_folder,
            window,
        });
        index += 1;
    }
    if !instances.iter().any(|seen| seen.number == instance_number) {
        let mut entry = Table::new();
        entry["number"] = value(i64::from(instance_number));
        let mut window_table = Table::new();
        window_table["width"] = value(480);
        window_table["height"] = value(320);
        entry["window"] = Item::Table(window_table);
        array.push(entry);
        instances.push(InstanceConfig {
            number: instance_number,
            image_folder: None,
            window: WindowConfig::default(),
        });
        *repaired = true;
    }
    instances
}

/// 迁移本体：顶层图库与 `[window]` 读出后写成 1 号段并移除原字段。
/// 顶层两处都缺失时 1 号段就是默认段（新装首启与老配置同路）。
fn migrate_legacy_into_instance_one(root: &mut Table, repaired: &mut bool) {
    let image_folder = root
        .get("image_folder")
        .and_then(Item::as_str)
        .map(str::to_owned);
    let mut legacy_window = match root.get_mut("window") {
        Some(Item::Table(table)) => repair_window_table(table, repaired),
        _ => WindowConfig::default(),
    };
    if root.remove("image_folder").is_some() {
        *repaired = true;
    }
    if root.remove("window").is_some() {
        *repaired = true;
    }
    let mut entry = Table::new();
    entry["number"] = value(1);
    if let Some(folder) = &image_folder {
        entry["image_folder"] = value(folder.clone());
    }
    let mut window_table = Table::new();
    if let Some(x) = legacy_window.x.take() {
        window_table["x"] = value(x);
    }
    if let Some(y) = legacy_window.y.take() {
        window_table["y"] = value(y);
    }
    window_table["width"] = value(i64::from(legacy_window.width));
    window_table["height"] = value(i64::from(legacy_window.height));
    entry["window"] = Item::Table(window_table);
    let mut array = toml_edit::ArrayOfTables::new();
    array.push(entry);
    root["instances"] = Item::ArrayOfTables(array);
    *repaired = true;
}

/// 按编号找实例段的可变引用：写回路径只允许经它碰实例段。
/// parse_document 已保证本实例段存在，找不到返回 None 供调用方
/// 安全跳过（不凭空造段覆写文档）。
fn instance_table_mut(document: &mut DocumentMut, number: u32) -> Option<&mut Table> {
    let array = document
        .as_table_mut()
        .get_mut("instances")?
        .as_array_of_tables_mut()?;
    array
        .iter_mut()
        .find(|entry| entry.get("number").and_then(Item::as_integer) == Some(i64::from(number)))
}

/// 以文档当前内容刷新配置视图的实例部分（图库、几何、段快照）：
/// update 在 apply 之后调用，保证内存视图与刚写回的文档一致。
fn refresh_instance_view(document: &DocumentMut, number: u32, config: &mut Config) {
    let mut instances = Vec::new();
    if let Some(array) = document
        .as_table()
        .get("instances")
        .and_then(Item::as_array_of_tables)
    {
        for entry in array.iter() {
            let Some(raw) = entry.get("number").and_then(Item::as_integer) else {
                continue;
            };
            if raw < 1 || raw > i64::from(u32::MAX) {
                continue;
            }
            let image_folder = entry
                .get("image_folder")
                .and_then(Item::as_str)
                .map(str::to_owned);
            let window = match entry.get("window").and_then(Item::as_table) {
                Some(table) => WindowConfig {
                    x: optional_i64(table, "x"),
                    y: optional_i64(table, "y"),
                    width: table
                        .get("width")
                        .and_then(Item::as_integer)
                        .filter(|raw| *raw > 0 && *raw <= i64::from(u32::MAX))
                        .map(|raw| raw as u32)
                        .unwrap_or(480),
                    height: table
                        .get("height")
                        .and_then(Item::as_integer)
                        .filter(|raw| *raw > 0 && *raw <= i64::from(u32::MAX))
                        .map(|raw| raw as u32)
                        .unwrap_or(320),
                },
                None => WindowConfig::default(),
            };
            instances.push(InstanceConfig {
                number: raw as u32,
                image_folder,
                window,
            });
        }
    }
    if let Some(mine) = instances.iter().find(|seen| seen.number == number) {
        config.image_folder = mine.image_folder.clone();
        config.window = mine.window.clone();
    }
    config.instances = instances;
}

/// 历史只留最近 512 条（保存与修复共用同一条上限规则）。
fn keep_newest_history(history: &mut Vec<String>) {
    if history.len() > HISTORY_LIMIT {
        let overflow = history.len() - HISTORY_LIMIT;
        history.drain(..overflow);
    }
}

/// 路径列表字段（本轮进度/历史）的修复：缺失即空；不是字符串数组时整字段
/// 丢弃并按缺失处理，不牵连其他字段。
fn repair_path_list(table: &mut Table, key: &str, repaired: &mut bool) -> Vec<String> {
    let parsed: Option<Vec<String>> = match table.get(key) {
        None => return Vec::new(),
        Some(item) => match item.as_array() {
            Some(array) => {
                let mut values = Vec::with_capacity(array.len());
                let mut valid = true;
                for entry in array.iter() {
                    match entry.as_str() {
                        Some(text) => values.push(text.to_owned()),
                        None => {
                            valid = false;
                            break;
                        }
                    }
                }
                valid.then_some(values)
            }
            None => None,
        },
    };
    match parsed {
        Some(values) => values,
        None => {
            table.remove(key);
            *repaired = true;
            Vec::new()
        }
    }
}

fn string_array_item(values: &[String]) -> Item {
    let mut array = toml_edit::Array::new();
    for value in values {
        array.push(value.as_str());
    }
    Item::Value(toml_edit::Value::Array(array))
}

fn ensure_table<'a>(root: &'a mut Table, key: &str, repaired: &mut bool) -> &'a mut Table {
    if !matches!(root.get(key), Some(Item::Table(_))) {
        if root.get(key).is_some() {
            root.remove(key);
        }
        root[key] = toml_edit::Item::Table(Table::new());
        *repaired = true;
    }
    root[key].as_table_mut().expect("table just ensured")
}

fn repair_bool(table: &mut Table, key: &str, default: bool, repaired: &mut bool) -> bool {
    match table.get(key).and_then(Item::as_bool) {
        Some(value) => value,
        None => {
            table[key] = value(default);
            *repaired = true;
            default
        }
    }
}

fn repair_dimension(table: &mut Table, key: &str, default: u32, repaired: &mut bool) -> u32 {
    match table.get(key).and_then(Item::as_integer) {
        Some(raw) if raw > 0 && raw <= i64::from(u32::MAX) => raw as u32,
        _ => {
            table[key] = value(i64::from(default));
            *repaired = true;
            default
        }
    }
}

/// 画框层宽字段（框宽 / 卡纸宽）的修复：整数与浮点都收，钳制到
/// [0, MAX_LAYER_WIDTH]；缺失、非数值或非有限值回默认。钳制与
/// 回退都写回并标记 repaired（与 interval_secs 的修复口径一致）；
/// 合法整数原样保留，不改写其表示。
fn repair_layer_width(table: &mut Table, key: &str, default: f32, repaired: &mut bool) -> f32 {
    let raw = table.get(key).and_then(|item| {
        item.as_float()
            .or_else(|| item.as_integer().map(|raw| raw as f64))
    });
    match raw {
        Some(raw) if raw.is_finite() => {
            let clamped = (raw as f32).clamp(0.0, MAX_LAYER_WIDTH);
            if clamped != raw as f32 {
                table[key] = value(f64::from(clamped));
                *repaired = true;
            }
            clamped
        }
        _ => {
            table[key] = value(f64::from(default));
            *repaired = true;
            default
        }
    }
}

fn optional_i64(table: &Table, key: &str) -> Option<i64> {
    table.get(key).and_then(Item::as_integer)
}
