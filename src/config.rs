//! 配置契约（父规格第 7 节 / 需求第 6 节）：`%APPDATA%\MoeFrame\config.toml` 的读写与修复。
//!
//! 原则：字段级修复——坏字段回退默认并写回，合法手改值与未知键在任何保存中保留；
//! 整个文件不可解析时恢复为合法默认配置，不崩溃。三个 P1 键（`pause_on_hover`、
//! `animation`、`show_filename`）只保存与修复，不驱动行为。

use crate::decode_validation::DisplayMode;
use std::{fs, io, path::Path};
use toml_edit::{value, DocumentMut, Item, Table};

pub const MIN_INTERVAL_SECS: i64 = 3;
pub const MAX_INTERVAL_SECS: i64 = 99_999_999;
pub const DEFAULT_INTERVAL_SECS: i64 = 30;

const DEFAULT_TEMPLATE: &str = "\
# MoeFrame 配置文件 —— 程序自动维护，也可手动编辑（重启生效）
recursive = true

[window]
width = 480
height = 320
lock_aspect = false

[playback]
mode = \"random\"
interval_secs = 30
pause_on_hover = true
animation = \"slide\"

[display]
fit_mode = \"fit\"
show_filename = false

[general]
autostart = false
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackMode {
    Sequential,
    Random,
}

/// P1 切换动画（`animation` 配置键）：只保存，不驱动行为。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Animation {
    Slide,
    Fade,
    None,
}

impl Animation {
    fn as_str(self) -> &'static str {
        match self {
            Animation::Slide => "slide",
            Animation::Fade => "fade",
            Animation::None => "none",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowConfig {
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub width: u32,
    pub height: u32,
    pub lock_aspect: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackConfig {
    pub mode: PlaybackMode,
    pub interval_secs: i64,
    /// P1：只保存，不驱动行为。
    pub pause_on_hover: bool,
    /// P1：只保存，不驱动行为。
    pub animation: Animation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayConfig {
    pub fit_mode: DisplayMode,
    /// P1：只保存，不驱动行为。
    pub show_filename: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneralConfig {
    pub autostart: bool,
    /// 上次播放到的图片；工单 10 只保留该字段，跨启动恢复由后续票实现。
    pub current_image: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
                lock_aspect: false,
            },
            playback: PlaybackConfig {
                mode: PlaybackMode::Random,
                interval_secs: DEFAULT_INTERVAL_SECS,
                pause_on_hover: true,
                animation: Animation::Slide,
            },
            display: DisplayConfig {
                fit_mode: DisplayMode::Fit,
                show_filename: false,
            },
            general: GeneralConfig {
                autostart: false,
                current_image: None,
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

    /// 保存图片文件夹（首次选择或菜单重选时调用），其余字段不动。
    pub fn set_image_folder(&mut self, folder: Option<&str>) -> io::Result<()> {
        match folder {
            Some(folder) => {
                self.document["image_folder"] = value(folder);
                self.config.image_folder = Some(folder.to_owned());
            }
            None => {
                self.document.remove("image_folder");
                self.config.image_folder = None;
            }
        }
        self.write()
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
    let window = WindowConfig {
        x: optional_i64(window_table, "x"),
        y: optional_i64(window_table, "y"),
        width: repair_dimension(window_table, "width", 480, repaired),
        height: repair_dimension(window_table, "height", 320, repaired),
        lock_aspect: repair_bool(window_table, "lock_aspect", false, repaired),
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
    let animation = match playback_table.get("animation").and_then(Item::as_str) {
        Some("slide") => Animation::Slide,
        Some("fade") => Animation::Fade,
        Some("none") => Animation::None,
        _ => {
            playback_table["animation"] = value(Animation::Slide.as_str());
            *repaired = true;
            Animation::Slide
        }
    };

    let display_table = ensure_table(root, "display", repaired);
    let fit_mode = match display_table.get("fit_mode").and_then(Item::as_str) {
        Some("fit") => DisplayMode::Fit,
        Some("fill") => DisplayMode::Fill,
        _ => {
            display_table["fit_mode"] = value("fit");
            *repaired = true;
            DisplayMode::Fit
        }
    };
    let show_filename = repair_bool(display_table, "show_filename", false, repaired);

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
            fit_mode,
            show_filename,
        },
        general: GeneralConfig {
            autostart,
            current_image,
        },
    }
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

fn optional_i64(table: &Table, key: &str) -> Option<i64> {
    table.get(key).and_then(Item::as_integer)
}
