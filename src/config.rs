//! 配置契约（父规格第 7 节 / 需求第 6 节）：`%APPDATA%\MoeFrame\config.toml` 的读写与修复。
//!
//! 原则：字段级修复——坏字段回退默认并写回，合法手改值与未知键在任何保存中保留；
//! 整个文件不可解析时恢复为合法默认配置，不崩溃。两个 P1 键（`pause_on_hover`、
//! `show_filename`）只保存与修复，不驱动行为；`animation` 已由切换动画专题
//! 工单 01 转正，驱动核心转场。

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

/// 切换动画（`animation` 配置键，切换动画专题工单 01 转正）：7 档，
/// 默认卷页。核心按档位驱动转场；`Random` 由核心在每次转场开始时抽成
/// 具体动画，存盘只记档位本身。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Animation {
    None,
    Curl,
    Blinds,
    Ink,
    Card,
    Pixel,
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
    pub image_folder: Option<String>,
    pub recursive: bool,
    pub window: WindowConfig,
    pub playback: PlaybackConfig,
    pub display: DisplayConfig,
    pub general: GeneralConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            image_folder: None,
            recursive: true,
            window: WindowConfig {
                x: None,
                y: None,
                width: 480,
                height: 320,
            },
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
pub struct ConfigStore {
    path: std::path::PathBuf,
    document: DocumentMut,
    config: Config,
}

impl ConfigStore {
    /// 读取并修复配置；文件缺失或不可解析时得到默认配置。修复结果立即写回。
    pub fn load(path: &Path) -> Self {
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
        let config = repair_document(&mut document, &mut repaired);
        let store = Self {
            path: path.to_owned(),
            document,
            config,
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

    /// 单字段保存的共用骨架：改文档与内存模型后写回，其余字段原样保留。
    fn update(
        &mut self,
        apply: impl FnOnce(&mut toml_edit::DocumentMut, &mut Config),
    ) -> io::Result<()> {
        apply(&mut self.document, &mut self.config);
        self.write()
    }

    /// 保存图片文件夹（首次选择或菜单重选时调用），其余字段不动。
    pub fn set_image_folder(&mut self, folder: Option<&str>) -> io::Result<()> {
        let folder = folder.map(str::to_owned);
        self.update(|document, config| match &folder {
            Some(folder) => {
                document["image_folder"] = value(folder.clone());
                config.image_folder = Some(folder.clone());
            }
            None => {
                document.remove("image_folder");
                config.image_folder = None;
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
    /// 删除对应键（回到「无存档位置」）。只动 window 表，其余字段保留。
    pub fn set_window_geometry(
        &mut self,
        x: Option<i64>,
        y: Option<i64>,
        width: u32,
        height: u32,
    ) -> io::Result<()> {
        self.update(|document, config| {
            for (key, val) in [("x", x), ("y", y)] {
                match val {
                    Some(v) => document["window"][key] = value(v),
                    None => {
                        if let Some(table) = document["window"].as_table_mut() {
                            table.remove(key);
                        }
                    }
                }
            }
            document["window"]["width"] = value(i64::from(width));
            document["window"]["height"] = value(i64::from(height));
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
fn repair_document(document: &mut DocumentMut, repaired: &mut bool) -> Config {
    let root = document.as_table_mut();

    let image_folder = match root.get("image_folder") {
        Some(item) => match item.as_str() {
            Some(text) => Some(text.to_owned()),
            None => {
                root.remove("image_folder");
                *repaired = true;
                None
            }
        },
        None => None,
    };
    let recursive = repair_bool(root, "recursive", true, repaired);

    let window_table = ensure_table(root, "window", repaired);
    if window_table.remove("lock_aspect").is_some() {
        *repaired = true;
    }
    let window = WindowConfig {
        x: optional_i64(window_table, "x"),
        y: optional_i64(window_table, "y"),
        width: repair_dimension(window_table, "width", 480, repaired),
        height: repair_dimension(window_table, "height", 320, repaired),
    };

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
    // 切换动画迁移（专题工单 01）：none 是新旧共有的值，保留；旧值
    // slide/fade 从未驱动过真实行为，与未知值一并落默认卷页并写回。
    let animation = match playback_table.get("animation").and_then(Item::as_str) {
        Some("none") => Animation::None,
        Some("curl") => Animation::Curl,
        Some("blinds") => Animation::Blinds,
        Some("ink") => Animation::Ink,
        Some("card") => Animation::Card,
        Some("pixel") => Animation::Pixel,
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
