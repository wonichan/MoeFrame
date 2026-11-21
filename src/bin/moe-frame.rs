#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

//! 正式相框（工单 10）：首次启动选文件夹，看到第一张图。
//! 行为规则在 `moe_frame::app_core`，本文件只负责窗口、纹理与效果执行。

#[cfg(not(windows))]
compile_error!("MoeFrame supports Windows 10/11 only");

#[cfg(windows)]
#[path = "moe-frame/worker.rs"]
mod worker;

#[cfg(windows)]
use eframe::egui;
#[cfg(windows)]
use moe_frame::{
    app_core::{Effect, FrameCore, Intent, View},
    autostart::{autostart_command, AutostartStore, AutostartSync, RUN_KEY_PATH, RUN_VALUE_NAME},
    config::PlaybackMode,
    config_sync::ConfigSync,
    decode_validation::DisplayMode,
    display::display_size,
    display_data::{DisplayKey, DisplayPipeline, NavDecision, Observation, RefreshDecision},
    gesture::GestureThresholds,
    loading::LoadRequest,
    playlist::Direction,
    resize_cursor, resize_direction,
    settings_session::{
        EffectiveValues, Field, OpenDecision, Picks, SessionAction, SettingsSession,
    },
    window_geom::{aspect_locked_size, bottom_right_position, offscreen_position, MIN_WINDOW_SIDE},
};
#[cfg(windows)]
use std::{
    error::Error,
    io,
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};
#[cfg(windows)]
use winit::platform::windows::WindowExtWindows;
#[cfg(windows)]
use worker::{Command, Reply};

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        // A GUI subsystem process has no console; surface startup failure via the log file.
        if let Some(directory) = app_directory() {
            let _ = std::fs::create_dir_all(&directory);
            let _ = std::fs::write(
                directory.join("startup-error.txt"),
                format!("MoeFrame failed to start: {error}"),
            );
        }
    }
}

/// 宿主 UI 时钟的单调毫秒值，是核心唯一的计时来源。
#[cfg(windows)]
fn now_ms(ctx: &egui::Context) -> u64 {
    (ctx.input(|input| input.time) * 1000.0) as u64
}

/// 路径列表转可落盘的字符串列表；无法表示为 UTF-8 的路径跳过。
#[cfg(windows)]
#[cfg(windows)]
fn app_directory() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("MoeFrame"))
}

#[cfg(windows)]
fn run() -> Result<(), Box<dyn Error>> {
    let directory = app_directory().ok_or_else(|| io::Error::other("APPDATA is unavailable"))?;
    let config_path = directory.join("config.toml");
    let log_path = directory.join("moe_frame.log");
    // 启动装配（工单 04 深挖）：配置加载/修复、自启对账与全部初值
    // 翻译归 config_sync，run() 只分发启动包。
    let (sync, package) = ConfigSync::startup(&config_path, &log_path, &mut RegAutostartStore);
    let core_args = package.core;
    let host_init = package.host;
    let autostart_sync = package.autostart;
    let fit_mode = host_init.fit_mode;
    let window = host_init.window.clone();
    let saved_position = host_init.saved_position.map(|p| egui::pos2(p[0], p[1]));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("MoeFrame")
        .with_decorations(false)
        .with_resizable(true)
        .with_inner_size([window.width as f32, window.height as f32])
        .with_min_inner_size([MIN_WINDOW_SIDE, MIN_WINDOW_SIDE])
        .with_maximized(false)
        .with_fullscreen(false)
        .with_window_level(egui::WindowLevel::Normal);
    if let Some(position) = saved_position {
        viewport = viewport.with_position(position);
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport,
        window_builder: Some(Box::new(|builder| {
            builder.with_active(false).with_drag_and_drop(false)
        })),
        ..Default::default()
    };
    eframe::run_native(
        "MoeFrame",
        options,
        Box::new(move |cc| {
            if let Some(window) = cc.winit_window() {
                // egui-winit enables a Windows shadow even without decorations; the frame must be bare.
                window.set_undecorated_shadow(false);
            }
            cc.egui_ctx
                .options_mut(|options| options.zoom_with_keyboard = false);
            cc.egui_ctx.set_zoom_factor(1.0);
            let (command_tx, reply_rx) = worker::spawn(log_path);
            Ok(Box::new(FrameApp {
                core: FrameCore::new(
                    core_args,
                    system_gesture_thresholds(cc.egui_ctx.pixels_per_point()),
                ),
                sync,
                fit_mode,
                command_tx,
                reply_rx,
                started: false,
                placed: saved_position.is_some(),
                texture: None,
                incoming: None,
                slide: None,
                press_window_pos: None,
                wheel_acc: 0.0,
                tray: None,
                tray_tried: false,
                tray_state: None,
                exiting: false,
                settings_open: false,
                settings_positioned: false,
                pipeline: DisplayPipeline::new(),
                locked_aspect: host_init.locked_aspect,
                last_inner: host_init.last_inner,
                saved_geom: host_init.saved_geom,
                geom_candidate: Debounce::new(),
                observed_geom: None,
                startup_clamp_done: false,
                autostart: autostart_sync,
                autostart_store: RegAutostartStore,
                session: SettingsSession::new(),
            }))
        }),
    )?;
    Ok(())
}

/// 托盘菜单项句柄（工单 17）：留住菜单项以便按核心状态更新文案。
struct TrayHandles {
    _icon: tray_icon::TrayIcon,
    show_hide: tray_icon::menu::MenuItem,
    play_pause: tray_icon::menu::MenuItem,
    prev: tray_icon::menu::MenuItem,
    next: tray_icon::menu::MenuItem,
    settings: tray_icon::menu::MenuItem,
    exit: tray_icon::menu::MenuItem,
}

/// 托盘图标（工单 17）：需求文档 8.3 定的内置 PNG（仓库 assets/tray-icon.png，
/// 红框白底、琥珀色太阳、青色山峦），编译时嵌入，运行时解码为 RGBA。
fn tray_icon_image() -> tray_icon::Icon {
    let image = image::load_from_memory(include_bytes!("../../assets/tray-icon.png"))
        .expect("内置托盘图标 PNG 固定可解码")
        .to_rgba8();
    let (width, height) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), width, height).expect("托盘图标像素格式固定")
}

/// 开机自启的注册表实现（工单 21）：当前用户 Run 项的 MoeFrame 值，
/// 存在即启用。业务配置不进注册表，只有这一项。
#[cfg(windows)]
struct RegAutostartStore;

#[cfg(windows)]
impl RegAutostartStore {
    /// 打开 Run 项：读用只读打开（项不存在即确定未启用，返回 None），
    /// 写用打开或创建。权限分开，不互相冒充。
    fn run_key(&self, write: bool) -> io::Result<Option<winreg::RegKey>> {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        if write {
            hkcu.create_subkey_with_flags(RUN_KEY_PATH, winreg::enums::KEY_WRITE)
                .map(|(key, _)| Some(key))
        } else {
            match hkcu.open_subkey_with_flags(RUN_KEY_PATH, winreg::enums::KEY_READ) {
                Ok(key) => Ok(Some(key)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        }
    }
}

#[cfg(windows)]
impl AutostartStore for RegAutostartStore {
    fn read_enabled(&mut self) -> io::Result<bool> {
        let Some(run) = self.run_key(false)? else {
            return Ok(false);
        };
        // 值存在即启用：字符串读不到时再看原始值——值在但类型不对也算
        // 「项存在」，不落进读失败退镜像的分支。
        match run.get_value::<String, _>(RUN_VALUE_NAME) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(_) => match run.get_raw_value(RUN_VALUE_NAME) {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error),
            },
        }
    }

    fn set_enabled(&mut self, enabled: bool, command: &str) -> io::Result<()> {
        let run = self
            .run_key(true)?
            .expect("写模式打开 Run 项必得键（打开或创建）");
        if enabled {
            run.set_value(RUN_VALUE_NAME, &command)
        } else {
            match run.delete_value(RUN_VALUE_NAME) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        }
    }
}

/// 建托盘图标与右键菜单（工单 17）：菜单项与相框菜单走同一套核心意图；
/// 「设置」打开工单 18 的设置窗（宿主窗口，不经核心）。失败返回 None，
/// 相框本体照常运行（托盘是常驻入口，不是相框的前置条件）。
fn build_tray(frame_visible: bool, playing: bool) -> Option<TrayHandles> {
    use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::new();
    // 文案用词汇表的动作名：可见时是「关闭」（只隐藏不退出），播放时是「暂停」。
    let show_hide = MenuItem::new(
        if frame_visible {
            "关闭相框"
        } else {
            "显示相框"
        },
        true,
        None,
    );
    let play_pause = MenuItem::new(if playing { "暂停" } else { "播放" }, true, None);
    let prev = MenuItem::new("上一张", true, None);
    let next = MenuItem::new("下一张", true, None);
    let settings = MenuItem::new("设置", true, None);
    let exit = MenuItem::new("退出", true, None);
    menu.append_items(&[
        &show_hide,
        &play_pause,
        &PredefinedMenuItem::separator(),
        &prev,
        &next,
        &PredefinedMenuItem::separator(),
        &settings,
        &exit,
    ])
    .ok()?;
    let icon = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(tray_icon_image())
        .with_tooltip("萌框 MoeFrame")
        .build()
        .ok()?;
    Some(TrayHandles {
        _icon: icon,
        show_hide,
        play_pause,
        prev,
        next,
        settings,
        exit,
    })
}

/// 用系统默认看图器打开原图（工单 15）：菜单项与双击效果共用这一处；
/// 失败不崩溃。路径含中文、空格与特殊字符时由系统打开命令原样处理。
#[cfg(windows)]
fn open_image(path: &std::path::Path) {
    let _ = open::that(path);
}

/// 手势阈值取 Windows 系统设置（工单 15）：拖动阈值、双击时间与距离，
/// 不写死另一套值；系统接口异常时退到与系统默认同量级的值。系统值是物理
/// 像素，按当前缩放换算成 egui 逻辑点，与指针事件坐标同口径。
#[cfg(windows)]
fn system_gesture_thresholds(pixels_per_point: f32) -> GestureThresholds {
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::GetDoubleClickTime,
        WindowsAndMessaging::{
            GetSystemMetrics, SM_CXDOUBLECLK, SM_CXDRAG, SM_CYDOUBLECLK, SM_CYDRAG,
        },
    };
    let scale = if pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    };
    let metric = |index: i32| {
        let value = unsafe { GetSystemMetrics(index) };
        if value > 0 {
            value as f32 / scale
        } else {
            4.0
        }
    };
    let double_click_ms = unsafe { GetDoubleClickTime() };
    GestureThresholds {
        drag_threshold: [metric(SM_CXDRAG), metric(SM_CYDRAG)],
        double_click_ms: if double_click_ms > 0 {
            u64::from(double_click_ms)
        } else {
            500
        },
        double_click_dist: [metric(SM_CXDOUBLECLK), metric(SM_CYDOUBLECLK)],
    }
}

/// 滑动转场时长（秒）：约 300ms、ease-out（父规格 §5）。
#[cfg(windows)]
const SLIDE_SECONDS: f64 = 0.3;

/// 间隔六档预设（工单 11/18）：相框菜单与设置窗共用同一份，不各写一份。
const INTERVAL_PRESETS: [(&str, i64); 6] = [
    ("10 秒", 10),
    ("30 秒", 30),
    ("1 分钟", 60),
    ("5 分钟", 300),
    ("10 分钟", 600),
    ("30 分钟", 1800),
];

/// 设置窗尺寸（工单 18）：建窗与屏幕居中计算共用。
const SETTINGS_SIZE: [f32; 2] = [420.0, 630.0];

/// 相框默认尺寸（工单 20 恢复默认用）：480×320 逻辑尺寸。
#[cfg(windows)]
const DEFAULT_FRAME_SIZE: [f32; 2] = [480.0, 320.0];

/// 设置窗的视口标识（工单 18）：全程只有一块。
fn settings_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("moe-frame-settings")
}

/// 输入字段的焦点跟踪（工单 19）：返回（回车，失焦）——回车记 enter
/// 并把焦点抢回框内（回车提交后焦点留在输入框）；焦点离开记 blurred。
/// 事件的时序裁决在库 settings_session，这里只做 egui 焦点机制。
#[cfg(windows)]
fn track_draft_field(
    response: &egui::Response,
    was_focused: &mut bool,
    enter: bool,
) -> (bool, bool) {
    if enter && (*was_focused || response.lost_focus()) {
        response.request_focus();
        *was_focused = true;
        (true, false)
    } else {
        let blurred = (*was_focused && !response.has_focus()) || response.lost_focus();
        *was_focused = response.has_focus();
        (false, blurred)
    }
}

/// 已落盘/待落盘的窗口几何（x, y, 宽, 高）（工单 20）。
#[cfg(windows)]
type SavedGeom = (Option<i64>, Option<i64>, u32, u32);

/// 停稳防抖（工单 20）：候选值保持不变满 settle_secs 后 observe 返回
/// true（结算一次）；值一变就重新计时。窗口几何落盘用（工单 20）。
#[cfg(windows)]
struct Debounce<T> {
    candidate: Option<(T, f64)>,
}

#[cfg(windows)]
impl<T: PartialEq> Debounce<T> {
    fn new() -> Self {
        Self { candidate: None }
    }

    fn observe(&mut self, value: T, now_secs: f64, settle_secs: f64) -> bool {
        match &self.candidate {
            Some((candidate, since)) if *candidate == value => now_secs - since >= settle_secs,
            _ => {
                self.candidate = Some((value, now_secs));
                false
            }
        }
    }

    fn clear(&mut self) {
        self.candidate = None;
    }
}

/// 一段进行中的滑动转场（工单 14）：方向与起始时刻（egui UI 时钟，秒）。
#[cfg(windows)]
#[derive(Clone, Copy)]
struct Slide {
    direction: Direction,
    started_secs: f64,
}

#[cfg(windows)]
struct FrameApp {
    core: FrameCore,
    sync: ConfigSync,
    fit_mode: DisplayMode,
    command_tx: Sender<Command>,
    reply_rx: Receiver<Reply>,
    started: bool,
    placed: bool,
    texture: Option<egui::TextureHandle>,
    /// 刚加载完成、等待核心决定直接提交还是开始转场的目标图纹理（工单 14）。
    incoming: Option<egui::TextureHandle>,
    /// 进行中的滑动转场渲染状态；源图是 texture、目标图是 incoming（工单 14）。
    slide: Option<Slide>,
    /// 本次左键按下时窗口的外框位置：右键取消拖动时回滚到这里（工单 15）。
    press_window_pos: Option<egui::Pos2>,
    /// 滚轮平滑滚动量的累积，攒够一格才触发一次切换。
    wheel_acc: f32,
    /// 托盘（工单 17）：首帧懒建（须在事件循环内建）；建失败为 None，相框照常。
    tray: Option<TrayHandles>,
    tray_tried: bool,
    /// 上次同步到托盘菜单的（可见性、播放中）状态：None 表示尚未同步，
    /// 首帧从核心现读一次，之后只在变化时改文案。
    tray_state: Option<(bool, bool)>,
    /// 已明确选择退出（工单 17）：关闭请求不再拦截，托盘图标先行移除。
    exiting: bool,
    /// 设置窗（工单 18）：是否打开、打开时是否已定位到屏幕中央。
    settings_open: bool,
    settings_positioned: bool,
    /// 显示数据管线（architecture-deepening 工单 01）：显示数据的记账、
    /// 转正、过期判定与刷新决策都在库 display_data 里；纹理句柄归宿主，
    /// 按它的当前/传入槽位对应持有。
    pipeline: DisplayPipeline,
    /// 宽高比锁定（工单 20）：Some(比例) 为锁定中，比例在打开开关那一刻
    /// 捕获（或启动时由存档尺寸推导）；开关本身落盘在 config。
    locked_aspect: Option<f32>,
    /// 上一帧观察到的窗口逻辑内尺寸：锁定修正的「哪边变了」基准。
    last_inner: [f32; 2],
    /// 最近一次落盘的窗口几何（x, y, 宽, 高）。
    saved_geom: SavedGeom,
    /// 几何变化的停稳防抖：停稳 0.6 秒才落盘。
    geom_candidate: Debounce<SavedGeom>,
    /// 最近一次观察到的窗口几何：退出时冲刷未落盘的最后位置（工单 20）。
    observed_geom: Option<SavedGeom>,
    /// 启动离屏钳制是否已检查（工单 20，只查一次）。
    startup_clamp_done: bool,
    /// 开机自启的真实状态（工单 21）：菜单与设置窗都读它；注册表实现
    /// 经接缝注入，切换失败时状态不动（回滚）。
    autostart: AutostartSync,
    autostart_store: RegAutostartStore,
    /// 设置窗会话（工单 19 的时序，architecture-deepening 工单 02 起归
    /// 库 settings_session）：草稿、基线、待结算与焦点记忆都在其中，
    /// 宿主只采样事件、渲染、执行它返回的动作。
    session: SettingsSession,
}

#[cfg(windows)]
impl FrameApp {
    fn dispatch(&mut self, ctx: &egui::Context, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::PickFolder => {
                    let picked = rfd::FileDialog::new()
                        .set_title("选择图片文件夹")
                        .pick_folder();
                    let intent = match picked {
                        Some(folder) => Intent::FolderPicked(folder),
                        None => Intent::FolderPickCancelled,
                    };
                    let follow_up = self.core.handle(intent);
                    self.dispatch(ctx, follow_up);
                }
                Effect::SaveFolder(folder) => {
                    self.sync.save_folder(&folder);
                }
                Effect::Scan(folder) => {
                    // 新一轮扫描开始：在飞的旧加载结果从此作废；
                    // 进行中的转场一并取消，画面回到核心仍在显示的源图。
                    self.pipeline.invalidate();
                    self.pipeline.drop_incoming();
                    self.slide = None;
                    self.incoming = None;
                    let _ = self.command_tx.send(Command::Scan { folder });
                }
                Effect::StartTransition { direction, .. } => {
                    self.slide = Some(Slide {
                        direction,
                        started_secs: ctx.input(|input| input.time),
                    });
                    ctx.request_repaint();
                }
                Effect::Load { candidates } => {
                    // 序号与在飞口径记账归显示数据管线：回送按序号裁决，
                    // 转正时当前槽记为这次加载的口径。
                    let target = self.physical_target(ctx);
                    let seq = self.pipeline.begin_load(self.fit_mode, target);
                    let request = LoadRequest::product(target, self.fit_mode);
                    let _ = self.command_tx.send(Command::Load {
                        seq,
                        candidates,
                        request,
                    });
                }
                Effect::Preload { path } => {
                    let key = DisplayKey {
                        path,
                        target: self.physical_target(ctx),
                        mode: self.fit_mode,
                    };
                    let _ = self.command_tx.send(Command::Preload { key });
                }
                Effect::SaveInterval(secs) => {
                    self.sync.save_interval(secs);
                }
                Effect::SavePlaybackMode(mode) => {
                    self.sync.save_playback_mode(mode);
                }
                Effect::ClearResumeState => {
                    // 图库已空：当前图、本轮进度与历史一并清掉（工单 16）。
                    self.sync.clear_resume_state();
                }
                Effect::SaveResumeState {
                    current,
                    seen,
                    history,
                } => {
                    self.sync.save_resume_state(&current, &seen, &history);
                }
                Effect::OpenImage { path } => {
                    open_image(&path);
                }
                Effect::RollbackFrameDrag => {
                    if let Some(pos) = self.press_window_pos.take() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                    }
                }
                Effect::HideFrame => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                Effect::ShowFrame => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Effect::Exit => {
                    // 退出收尾归 config_sync：最后观察到的几何一次结清
                    // （工单 20 的退出冲刷），宿主只同步落盘基准。
                    self.saved_geom = self.sync.shutdown(self.observed_geom, self.saved_geom);
                    // 明确退出（工单 17）：先摘托盘图标再关窗口，关闭请求不拦截。
                    self.exiting = true;
                    self.tray = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn physical_target(&self, ctx: &egui::Context) -> [u32; 2] {
        let rect = ctx.viewport_rect();
        let ppp = ctx.pixels_per_point();
        [
            ((rect.width() * ppp).round() as u32).max(1),
            ((rect.height() * ppp).round() as u32).max(1),
        ]
    }

    /// 应用显示方式（工单 18）：设置窗与相框菜单「显示方式」共用——改
    /// 宿主状态、落盘、触发当前图按新方式刷新；同值不动。
    fn apply_fit_mode(&mut self, ctx: &egui::Context, mode: DisplayMode) {
        if mode != self.fit_mode {
            self.fit_mode = mode;
            self.sync.save_fit_mode(mode);
            self.refresh_display_data(ctx);
        }
    }

    /// 打开设置窗或把它带到前台（工单 18）：只有一块，已开时只聚焦，
    /// 不重置位置（位置不保存、也不随相框移动）。从托盘打开不唤出相框。
    /// 开/不开的裁决与播种在会话里（工单 02），这里只执行窗口动作。
    fn request_settings(&mut self, ctx: &egui::Context) {
        match self.session.open_requested(&self.effective_values()) {
            OpenDecision::FocusOnly => {
                ctx.send_viewport_cmd_to(settings_viewport_id(), egui::ViewportCommand::Focus);
            }
            OpenDecision::Opened => {
                self.settings_open = true;
                self.settings_positioned = false;
            }
        }
    }

    /// 当前生效值快照（工单 02）：每帧递交会话做作废比对与结算。
    fn effective_values(&self) -> EffectiveValues {
        EffectiveValues {
            folder_text: self.effective_folder_text(),
            interval_secs: self.core.interval_secs(),
            mode: self.core.playback_mode(),
        }
    }

    /// 当前生效的图库文件夹文字（无文件夹时为空串）。
    fn effective_folder_text(&self) -> String {
        self.core
            .folder()
            .map(|folder| folder.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// 执行会话返回的本帧动作（工单 02）：顺序由会话定，这里只把每个
    /// 动作落到核心意图或宿主机制上。落盘类动作经 config_sync 落盘
    /// （工单 04 改道）：文件夹/间隔/模式经核心的 Save 系列效果，
    /// 显示方式、自启镜像、几何与宽高比锁定经同步的宿主镜像方法。
    fn execute_session_actions(
        &mut self,
        ctx: &egui::Context,
        actions: Vec<SessionAction>,
        now: u64,
    ) {
        for action in actions {
            match action {
                SessionAction::CommitFolder(text) => {
                    let effects = self.core.handle(Intent::FolderPicked(PathBuf::from(text)));
                    self.dispatch(ctx, effects);
                }
                SessionAction::CommitInterval(secs) | SessionAction::ApplyIntervalPreset(secs) => {
                    let effects = self
                        .core
                        .handle(Intent::IntervalSelected { secs, now_ms: now });
                    self.dispatch(ctx, effects);
                }
                SessionAction::ApplyMode(mode) => {
                    let effects = self.core.handle(Intent::PlaybackModeSelected(mode));
                    self.dispatch(ctx, effects);
                }
                SessionAction::ApplyFitMode(mode) => {
                    self.apply_fit_mode(ctx, mode);
                }
                SessionAction::ApplyAspectLock(locked) => {
                    if locked {
                        // 打开那一刻捕获当前窗口比例（工单 20），不固定成默认比例。
                        self.locked_aspect = Some(if self.last_inner[1] > 0.0 {
                            self.last_inner[0] / self.last_inner[1]
                        } else {
                            DEFAULT_FRAME_SIZE[0] / DEFAULT_FRAME_SIZE[1]
                        });
                    } else {
                        self.locked_aspect = None;
                    }
                    self.sync.save_lock_aspect(locked);
                }
                SessionAction::RestoreDefaultGeometry => {
                    self.restore_default_geometry(ctx);
                }
                SessionAction::ApplyAutostart(target) => {
                    self.apply_autostart(target);
                }
                SessionAction::Rescan => {
                    let effects = self.core.handle(Intent::RescanRequested);
                    self.dispatch(ctx, effects);
                }
                SessionAction::CloseWindow => {
                    self.settings_open = false;
                }
            }
        }
    }

    /// 设置窗本体（工单 18/19）：非模态独立视口。会话语义（草稿时序、
    /// 点选顺序、关闭/重扫否决）在库 settings_session（工单 02）；这里
    /// 只采样 egui 事件、借会话的草稿文字渲染、把点选收集成 Picks，
    /// 帧尾执行会话返回的动作。定位居中留在这里（工单 18 口径）。
    /// 三项 P1 控件不出现（ADR-0003）。
    fn show_settings(&mut self, ctx: &egui::Context, now: u64) {
        self.session.begin_frame(&self.effective_values());
        let mut builder = egui::ViewportBuilder::default()
            .with_title("萌框设置")
            .with_inner_size(SETTINGS_SIZE)
            .with_resizable(false);
        if !self.settings_positioned {
            self.settings_positioned = true;
            // 默认在相框所在屏幕中央：egui 只给得到相框所在屏的尺寸，原点
            // 按该屏左上角计（单屏精确）；拿不到尺寸就不指定位置。
            if let Some(monitor) = ctx.input(|input| input.viewport().monitor_size) {
                builder = builder.with_position([
                    ((monitor.x - SETTINGS_SIZE[0]) / 2.0).max(0.0),
                    ((monitor.y - SETTINGS_SIZE[1]) / 2.0).max(0.0),
                ]);
            }
        }
        let mode_now = self.core.playback_mode();
        let interval_now = self.core.interval_secs();
        let fit_now = self.fit_mode;
        let lock_now = self.locked_aspect.is_some();
        let autostart_now = self.autostart.enabled();
        let mut picks = Picks::default();
        let mut viewport_focused_now = true;
        let session = &mut self.session;
        let picks_ref = &mut picks;
        let focused_ref = &mut viewport_focused_now;
        ctx.show_viewport_immediate(settings_viewport_id(), builder, |ui, _class| {
            let sctx = ui.ctx().clone();
            if sctx.input(|input| input.viewport().close_requested())
                || sctx.input(|input| input.key_pressed(egui::Key::Escape))
            {
                picks_ref.close = true;
            }
            let enter = sctx.input(|input| input.key_pressed(egui::Key::Enter));
            *focused_ref = sctx.input(|input| input.viewport().focused).unwrap_or(true);
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label("图库文件夹");
                let path_resp = ui.add(
                    egui::TextEdit::singleline(session.path_text_mut())
                        .desired_width(f32::INFINITY),
                );
                let (entered, blurred) =
                    track_draft_field(&path_resp, session.path_focus_mut(), enter);
                if entered {
                    session.note_enter(Field::Path);
                }
                if blurred {
                    session.note_blur(Field::Path);
                }
                if ui.button("重新扫描").clicked() {
                    picks_ref.rescan = true;
                }
                ui.separator();
                ui.label("播放模式");
                ui.horizontal(|ui| {
                    if ui
                        .radio(mode_now == PlaybackMode::Sequential, "顺序")
                        .clicked()
                    {
                        picks_ref.mode = Some(PlaybackMode::Sequential);
                    }
                    if ui.radio(mode_now == PlaybackMode::Random, "随机").clicked() {
                        picks_ref.mode = Some(PlaybackMode::Random);
                    }
                });
                ui.separator();
                ui.label("切换间隔");
                for (label, secs) in INTERVAL_PRESETS {
                    if ui.radio(interval_now == secs, label).clicked() {
                        picks_ref.interval = Some(secs);
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("自定义");
                    let seconds_resp = ui.add(
                        egui::TextEdit::singleline(session.seconds_text_mut()).desired_width(110.0),
                    );
                    ui.label("秒");
                    let (entered, blurred) =
                        track_draft_field(&seconds_resp, session.seconds_focus_mut(), enter);
                    if entered {
                        session.note_enter(Field::Seconds);
                    }
                    if blurred {
                        session.note_blur(Field::Seconds);
                    }
                });
                ui.separator();
                ui.label("窗口");
                let mut lock_checked = lock_now;
                if ui
                    .checkbox(&mut lock_checked, "宽高比锁定（按当前比例）")
                    .clicked()
                {
                    picks_ref.aspect_lock = Some(lock_checked);
                }
                if ui.button("恢复默认位置与大小").clicked() {
                    picks_ref.restore_default = true;
                }
                ui.separator();
                ui.label("显示方式");
                ui.horizontal(|ui| {
                    if ui.radio(fit_now == DisplayMode::Fit, "适应").clicked() {
                        picks_ref.fit = Some(DisplayMode::Fit);
                    }
                    if ui.radio(fit_now == DisplayMode::Fill, "填充").clicked() {
                        picks_ref.fit = Some(DisplayMode::Fill);
                    }
                });
                ui.separator();
                let mut autostart_checked = autostart_now;
                if ui.checkbox(&mut autostart_checked, "开机自启").clicked() {
                    picks_ref.autostart = Some(autostart_checked);
                }
            });
        });
        let actions = self.session.end_frame(picks, viewport_focused_now);
        self.execute_session_actions(ctx, actions, now);
    }

    /// 当前纹理的显示数据过期时刷新当前图（工单 18/20 的行为，记账与
    /// 决策在库 display_data）：宿主每帧把观察值喂给管线，它决定是否
    /// 发起刷新；刷新走 worker 的独立 Refresh 类别，回送只换纹理、
    /// 不经核心提交。apply_fit_mode 与 drain_replies 都经这里。
    fn refresh_display_data(&mut self, ctx: &egui::Context) {
        let showing = match self.core.view() {
            View::Showing(path) => Some(path.clone()),
            _ => None,
        };
        let observation = Observation {
            showing,
            fit_mode: self.fit_mode,
            target: self.physical_target(ctx),
            slide_active: self.slide.is_some(),
            now_secs: ctx.input(|input| input.time),
        };
        if let Some(key) = self.pipeline.tick(&observation) {
            let _ = self.command_tx.send(Command::Refresh { key });
        }
    }

    /// 窗口几何维护（工单 20）：启动离屏钳制（只查一次）、宽高比锁定
    /// 修正、几何落盘（变化停稳 0.6 秒写一次，拖动中不反复写盘）。
    fn update_window_geometry(&mut self, ctx: &egui::Context) {
        let (outer, inner, monitor) = ctx.input(|input| {
            let viewport = input.viewport();
            (
                viewport.outer_rect,
                viewport.inner_rect,
                viewport.monitor_size,
            )
        });
        if !self.startup_clamp_done {
            if let (Some(outer), Some(monitor)) = (outer, monitor) {
                self.startup_clamp_done = true;
                if let Some(pos) = offscreen_position(
                    [outer.min.x, outer.min.y],
                    [outer.width(), outer.height()],
                    [monitor.x, monitor.y],
                ) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                        pos[0], pos[1],
                    )));
                }
            }
        }
        let Some(inner) = inner else {
            return;
        };
        let cur = [inner.width(), inner.height()];
        if let Some(aspect) = self.locked_aspect {
            // 指针按下（拖动/缩放进行中）时不抢，停稳后按偏离修正一次。
            let pointer_down = ctx.input(|input| input.pointer.any_down());
            if !pointer_down && cur != self.last_inner {
                let corrected = aspect_locked_size(self.last_inner, cur, aspect);
                if (corrected[0] - cur[0]).abs() > 0.5 || (corrected[1] - cur[1]).abs() > 0.5 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        corrected[0],
                        corrected[1],
                    )));
                }
            }
        }
        self.last_inner = cur;
        if !self.core.frame_visible() {
            return;
        }
        let Some(outer) = outer else {
            return;
        };
        let geom = (
            Some(outer.min.x.round() as i64),
            Some(outer.min.y.round() as i64),
            cur[0].round() as u32,
            cur[1].round() as u32,
        );
        self.observed_geom = Some(geom);
        if geom == self.saved_geom {
            self.geom_candidate.clear();
            return;
        }
        let now_secs = ctx.input(|input| input.time);
        if self.geom_candidate.observe(geom, now_secs, 0.6) {
            self.sync.save_window_geometry(geom);
            self.saved_geom = geom;
            self.geom_candidate.clear();
        }
    }

    /// 切换开机自启（工单 21）：先写注册表，成功才保存配置镜像
    /// （保存失败不回滚系统状态——界面与系统仍一致，镜像靠下次启动
    /// 对齐自愈）；
    /// 失败时状态保持操作前——菜单与设置窗都从 autostart 状态渲染，
    /// 自然回到原值，不弹窗、不伪报。目标命令每次按当前程序路径生成。
    fn apply_autostart(&mut self, target: bool) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let command = autostart_command(&exe);
        if let Some(value) = self
            .autostart
            .apply(&mut self.autostart_store, target, &command)
        {
            self.sync.save_autostart_mirror(value);
        }
    }

    /// 恢复默认位置与大小（工单 20）：立即 480×320、当前屏右下角内缩
    /// 24px 并落盘；相框隐藏时按主屏口径放置并显示相框（与托盘开设置
    /// 不唤出相框相区别）；锁定开启时，锁定的宽高比同步改为默认尺寸的比例。
    fn restore_default_geometry(&mut self, ctx: &egui::Context) {
        let size = DEFAULT_FRAME_SIZE;
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            size[0], size[1],
        )));
        let monitor = ctx.input(|input| input.viewport().monitor_size);
        let pos = monitor.map(|m| bottom_right_position(size, [m.x, m.y]));
        if let Some(pos) = pos {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                pos[0], pos[1],
            )));
        }
        if !self.core.frame_visible() {
            let effects = self.core.handle(Intent::ToggleVisibilityRequested);
            self.dispatch(ctx, effects);
        }
        if self.locked_aspect.is_some() {
            self.locked_aspect = Some(size[0] / size[1]);
        }
        let (x, y) = match pos {
            Some(p) => (Some(p[0].round() as i64), Some(p[1].round() as i64)),
            None => (self.saved_geom.0, self.saved_geom.1),
        };
        self.sync
            .save_window_geometry((x, y, size[0] as u32, size[1] as u32));
        self.saved_geom = (x, y, size[0] as u32, size[1] as u32);
        self.geom_candidate.clear();
    }

    fn drain_replies(&mut self, ctx: &egui::Context) {
        while let Ok(reply) = self.reply_rx.try_recv() {
            match reply {
                Reply::ScanCompleted(paths) => {
                    let effects = self.core.handle(Intent::ScanCompleted(paths));
                    self.dispatch(ctx, effects);
                    // 重扫后图库为空：核心回到无图状态，画面同步清掉（工单 16）。
                    if self.core.view() == View::Empty {
                        self.texture = None;
                        self.incoming = None;
                        self.slide = None;
                        self.pipeline.drop_incoming();
                        self.pipeline.clear_current();
                    }
                }
                Reply::Loaded { seq, image } => {
                    // 序号裁决归管线：过期回送（换文件夹之前）不碰当前
                    // 画面与状态，也不通知核心。
                    let decision = self
                        .pipeline
                        .nav_reply(seq, image.as_ref().map(|image| image.path.as_path()));
                    if decision == NavDecision::Stale {
                        continue;
                    }
                    let shown = image.as_ref().map(|image| image.path.clone());
                    if let Some(image) = image {
                        self.upload(ctx, image);
                    }
                    let effects = self.core.handle(Intent::LoadCompleted {
                        shown,
                        now_ms: now_ms(ctx),
                    });
                    self.dispatch(ctx, effects);
                    // 核心决定起转场时（dispatch 已置 slide）目标图留在 incoming
                    // 里滑动；否则（首图或直接提交）立即成为当前图（工单 14），
                    // 当前槽记录随纹理一并转正（管线记账）。
                    if self.slide.is_none() && self.incoming.is_some() {
                        self.texture = self.incoming.take();
                        self.pipeline.promote_incoming();
                    }
                    // 导航全失败时核心保留当前图，纹理不动；只有核心回到无图状态才清纹理。
                    if self.core.view() == View::Empty {
                        self.texture = None;
                        self.pipeline.clear_current();
                    }
                }
                Reply::Refreshed { image } => {
                    // 当前图刷新（工单 18/20）：只换纹理，不经核心、不算
                    // 一次显示。是否替换由管线裁决（画面切走、转场中或
                    // 刷新失败都丢弃，失败保旧图且同口径不再重试）。
                    let showing = match self.core.view() {
                        View::Showing(path) => Some(path.clone()),
                        _ => None,
                    };
                    let decision = self.pipeline.refresh_reply(
                        image.as_ref().map(|image| image.path.as_path()),
                        showing.as_deref(),
                        self.slide.is_some(),
                    );
                    if decision == RefreshDecision::Swap {
                        if let Some(image) = image {
                            self.upload(ctx, image);
                            self.texture = self.incoming.take();
                        }
                    }
                }
            }
        }
        self.refresh_display_data(ctx);
    }

    fn upload(&mut self, ctx: &egui::Context, image: moe_frame::decode_validation::DisplayImage) {
        let size = [
            image.display_pixels[0] as usize,
            image.display_pixels[1] as usize,
        ];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba);
        // 先放 incoming：是否滑动由核心在 LoadCompleted 的效果里决定（工单 14）。
        self.incoming =
            Some(ctx.load_texture("moe-frame-incoming", color, egui::TextureOptions::LINEAR));
    }

    fn place_bottom_right(&mut self, ctx: &egui::Context) {
        if self.placed {
            return;
        }
        let Some(monitor) = ctx.input(|input| input.viewport().monitor_size) else {
            return;
        };
        let rect = ctx.viewport_rect();
        let placed = bottom_right_position([rect.width(), rect.height()], [monitor.x, monitor.y]);
        let pos = egui::pos2(placed[0], placed[1]);
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        self.placed = true;
    }
}

#[cfg(windows)]
impl eframe::App for FrameApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.started {
            self.started = true;
            let effects = self.core.handle(Intent::Started);
            self.dispatch(&ctx, effects);
        }
        self.drain_replies(&ctx);
        self.update_window_geometry(&ctx);
        self.place_bottom_right(&ctx);
        let now = now_ms(&ctx);
        let tick_effects = self.core.handle(Intent::Tick { now_ms: now });
        self.dispatch(&ctx, tick_effects);
        // 托盘（工单 17）：首帧在事件循环内建托盘；之后每帧收托盘事件——
        // 左键单击切换显示，菜单项走与相框菜单完全相同的核心意图，隐藏时同样有效。
        if !self.tray_tried {
            self.tray_tried = true;
            self.tray = build_tray(self.core.frame_visible(), self.core.is_playing());
        }
        let mut tray_intents: Vec<Intent> = Vec::new();
        let mut tray_settings = false;
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = event
            {
                tray_intents.push(Intent::ToggleVisibilityRequested);
            }
        }
        if let Some(tray) = &self.tray {
            while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                let id = event.id();
                if id == tray.show_hide.id() {
                    tray_intents.push(Intent::ToggleVisibilityRequested);
                } else if id == tray.play_pause.id() {
                    tray_intents.push(Intent::PlayPauseToggled { now_ms: now });
                } else if id == tray.prev.id() {
                    tray_intents.push(Intent::PrevRequested);
                } else if id == tray.next.id() {
                    tray_intents.push(Intent::NextRequested);
                } else if id == tray.settings.id() {
                    tray_settings = true;
                } else if id == tray.exit.id() {
                    tray_intents.push(Intent::ExitRequested);
                }
            }
        }
        for intent in tray_intents {
            let effects = self.core.handle(intent);
            self.dispatch(&ctx, effects);
        }
        if tray_settings {
            self.request_settings(&ctx);
        }
        // 关闭只隐藏（工单 17）：拦截窗口关闭请求（含 Alt+F4）改走核心的
        // 关闭即隐藏；明确退出已置 exiting 时不拦截，关窗即结束进程。
        if !self.exiting && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            let effects = self.core.handle(Intent::CloseRequested);
            self.dispatch(&ctx, effects);
        }
        self.handle_keys_and_wheel(&ctx, now);
        // 滑动转场推进（工单 14）：到时先把目标图转正并通知核心提交，再绘制。
        if let Some(slide) = self.slide {
            let elapsed = ctx.input(|input| input.time) - slide.started_secs;
            if elapsed >= SLIDE_SECONDS {
                self.slide = None;
                // 切走图的纹理随旧 handle 一并释放；目标图成为当前图，
                // 当前槽记录随纹理一并转正（管线记账，与直接提交共用）。
                self.texture = self.incoming.take();
                self.pipeline.promote_incoming();
                let effects = self
                    .core
                    .handle(Intent::TransitionCompleted { now_ms: now });
                self.dispatch(&ctx, effects);
            } else {
                ctx.request_repaint();
            }
        }

        let mut menu_intent: Option<Intent> = None;
        let mut menu_settings = false;
        let mut menu_autostart: Option<bool> = None;
        let mut menu_fit: Option<DisplayMode> = None;
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter();
                // 绘制几何（工单 18）：适应完整放入留黑边、填充覆盖居中
                // 裁剪，与库共用同一份 display_size 计算。
                let fit_mode = self.fit_mode;
                let fitted = |texture: &egui::TextureHandle| {
                    let tex = [texture.size()[0] as f32, texture.size()[1] as f32];
                    let size = display_size(fit_mode, [rect.width(), rect.height()], tex);
                    egui::Rect::from_center_size(rect.center(), egui::vec2(size[0], size[1]))
                };
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                if let (Some(slide), Some(outgoing), Some(incoming)) =
                    (self.slide, &self.texture, &self.incoming)
                {
                    // 转场中：源图与目标图按 ease-out 进度同向滑动（工单 14）。
                    let t = ((ctx.input(|input| input.time) - slide.started_secs) / SLIDE_SECONDS)
                        .clamp(0.0, 1.0) as f32;
                    let eased = 1.0 - (1.0 - t).powi(3);
                    // 行程按各自矩形算（工单 18）：切入图从恰好整幅出框的
                    // 位置进、切出图走到恰好整幅出框——填充图比窗口宽，
                    // 整宽平移会在起始帧先露出一截切入图。
                    let out_rect = fitted(outgoing);
                    let in_rect = fitted(incoming);
                    let (out_dx, in_dx) = match slide.direction {
                        Direction::Next => (
                            -(out_rect.right() - rect.left()) * eased,
                            (rect.right() - in_rect.left()) * (1.0 - eased),
                        ),
                        Direction::Prev => (
                            (rect.right() - out_rect.left()) * eased,
                            -(in_rect.right() - rect.left()) * (1.0 - eased),
                        ),
                    };
                    let painter = ui.painter_at(rect);
                    painter.image(
                        outgoing.id(),
                        out_rect.translate(egui::vec2(out_dx, 0.0)),
                        uv,
                        egui::Color32::WHITE,
                    );
                    painter.image(
                        incoming.id(),
                        in_rect.translate(egui::vec2(in_dx, 0.0)),
                        uv,
                        egui::Color32::WHITE,
                    );
                } else if let Some(texture) = &self.texture {
                    painter.image(texture.id(), fitted(texture), uv, egui::Color32::WHITE);
                } else {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "右键选择文件夹",
                        egui::FontId::proportional(15.0),
                        egui::Color32::from_rgb(200, 200, 200),
                    );
                }
            });
        let playing = self.core.is_playing();
        let current_interval = self.core.interval_secs();
        let current_mode = self.core.playback_mode();
        let current_path = match self.core.view() {
            View::Showing(path) => Some(path),
            View::Empty => None,
        };
        panel.response.context_menu(|ui| {
            if ui.button("选择文件夹…").clicked() {
                menu_intent = Some(Intent::FolderPickRequested);
                ui.close();
            }
            if ui.button("重新扫描").clicked() {
                menu_intent = Some(Intent::RescanRequested);
                ui.close();
            }
            ui.separator();
            if ui.button("上一张").clicked() {
                menu_intent = Some(Intent::PrevRequested);
                ui.close();
            }
            if ui.button("下一张").clicked() {
                menu_intent = Some(Intent::NextRequested);
                ui.close();
            }
            if ui.button(if playing { "暂停" } else { "播放" }).clicked() {
                menu_intent = Some(Intent::PlayPauseToggled { now_ms: now });
                ui.close();
            }
            ui.menu_button("播放模式", |ui| {
                for (label, mode) in [
                    ("顺序", PlaybackMode::Sequential),
                    ("随机", PlaybackMode::Random),
                ] {
                    if ui.selectable_label(mode == current_mode, label).clicked() {
                        menu_intent = Some(Intent::PlaybackModeSelected(mode));
                        ui.close();
                    }
                }
            });
            ui.menu_button("切换间隔", |ui| {
                for (label, secs) in INTERVAL_PRESETS {
                    if ui
                        .selectable_label(secs == current_interval, label)
                        .clicked()
                    {
                        menu_intent = Some(Intent::IntervalSelected { secs, now_ms: now });
                        ui.close();
                    }
                }
            });
            if let Some(current) = &current_path {
                ui.separator();
                if ui.button("打开当前原图").clicked() {
                    open_image(current);
                    ui.close();
                }
                if ui.button("打开所在文件夹").clicked() {
                    if let Some(parent) = current.parent() {
                        let _ = open::that(parent);
                    }
                    ui.close();
                }
                if ui.button("复制图片路径").clicked() {
                    ui.ctx().copy_text(current.to_string_lossy().into_owned());
                    ui.close();
                }
            }
            ui.menu_button("显示方式", |ui| {
                for (label, mode) in [("适应", DisplayMode::Fit), ("填充", DisplayMode::Fill)] {
                    if ui.selectable_label(mode == self.fit_mode, label).clicked() {
                        menu_fit = Some(mode);
                        ui.close();
                    }
                }
            });
            ui.separator();
            if ui.button("设置").clicked() {
                menu_settings = true;
                ui.close();
            }
            let mut autostart_checked = self.autostart.enabled();
            if ui.checkbox(&mut autostart_checked, "开机自启").clicked() {
                menu_autostart = Some(autostart_checked);
                ui.close();
            }
            if ui.button("退出").clicked() {
                menu_intent = Some(Intent::ExitRequested);
                ui.close();
            }
        });
        self.handle_pointer(&ctx);
        if let Some(intent) = menu_intent {
            let effects = self.core.handle(intent);
            self.dispatch(&ctx, effects);
        }
        if let Some(mode) = menu_fit {
            self.apply_fit_mode(&ctx, mode);
        }
        if let Some(target) = menu_autostart {
            self.apply_autostart(target);
        }
        if menu_settings {
            self.request_settings(&ctx);
        }
        if self.settings_open {
            self.show_settings(&ctx, now);
        }
        // 托盘菜单文案跟随核心的真实状态（工单 17），只在变化时更新。
        if let Some(tray) = &self.tray {
            let state = (self.core.frame_visible(), self.core.is_playing());
            if self.tray_state != Some(state) {
                self.tray_state = Some(state);
                tray.show_hide.set_text(if state.0 {
                    "关闭相框"
                } else {
                    "显示相框"
                });
                tray.play_pause
                    .set_text(if state.1 { "暂停" } else { "播放" });
            }
        }
        if !self.core.frame_visible() {
            // 隐藏期间保持低频心跳（工单 17）：托盘事件在 ui() 里轮询，暂停且
            // 隐藏时没有轮播截止时刻可唤醒，心跳保证托盘点击/菜单及时被处理；
            // 轮播 Tick 仍按各自截止时刻在心跳帧上判定，不受影响。
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if self.pipeline.load_in_flight() {
            ctx.request_repaint_after(Duration::from_millis(50));
        } else if let Some(deadline) = self.core.next_deadline_ms() {
            // 睡到下一次自动切换时刻再唤醒，避免空转重绘。
            ctx.request_repaint_after(Duration::from_millis(deadline.saturating_sub(now)));
        }
    }
}

#[cfg(windows)]
impl FrameApp {
    /// 聚焦时的左右方向键/空格与滚轮：与菜单共用同一组导航意图（工单 11）。
    fn handle_keys_and_wheel(&mut self, ctx: &egui::Context, now: u64) {
        let (prev, next, toggle) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowRight),
                input.key_pressed(egui::Key::Space),
            )
        });
        let mut intents = Vec::new();
        if prev {
            intents.push(Intent::PrevRequested);
        }
        if next {
            intents.push(Intent::NextRequested);
        }
        if toggle {
            intents.push(Intent::PlayPauseToggled { now_ms: now });
        }
        // 滚轮：平滑量攒够一格触发一次；向上（正）为上一张，向下为下一张。
        const WHEEL_NOTCH: f32 = 24.0;
        let scroll = ctx.input(|input| input.smooth_scroll_delta.y);
        if scroll != 0.0 {
            self.wheel_acc += scroll;
            if self.wheel_acc >= WHEEL_NOTCH {
                intents.push(Intent::PrevRequested);
                self.wheel_acc = 0.0;
            } else if self.wheel_acc <= -WHEEL_NOTCH {
                intents.push(Intent::NextRequested);
                self.wheel_acc = 0.0;
            }
        }
        for intent in intents {
            let effects = self.core.handle(intent);
            self.dispatch(ctx, effects);
        }
    }
}

#[cfg(windows)]
impl FrameApp {
    /// 指针事件（工单 15）：按发生顺序把内容区的按下/移动/松开与右键按下
    /// 转发给行为核心按系统阈值判定（单击/双击/拖动/取消），窗口拖动与边缘
    /// 缩放仍走原生命令；边缘按下只缩放，不进入手势判定。系统拖动期间移动
    /// 事件不送达应用，核心以松开时的总位移兜底判定拖动；拖动中右键的取消
    /// 与位置恢复由系统拖动环本身处理，核心侧的回滚效果是事件可达时的保障。
    fn handle_pointer(&mut self, ctx: &egui::Context) {
        if ctx.current_pass_index() != 0 {
            return;
        }
        let rect = ctx.viewport_rect();
        if let Some(pos) = ctx.input(|input| input.pointer.hover_pos()) {
            if rect.contains(pos) {
                if let Some(direction) = resize_direction(rect, pos) {
                    ctx.set_cursor_icon(resize_cursor(direction));
                }
            }
        }
        let maximized_or_fullscreen = ctx.input(|input| {
            input.viewport().maximized == Some(true) || input.viewport().fullscreen == Some(true)
        });
        let now = now_ms(ctx);
        let events: Vec<egui::Event> = ctx.input(|input| input.events.clone());
        for event in &events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } if rect.contains(*pos) => {
                    if let Some(direction) = resize_direction(rect, *pos) {
                        if !maximized_or_fullscreen {
                            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                        }
                        continue;
                    }
                    self.press_window_pos = ctx
                        .input(|input| input.viewport().outer_rect)
                        .map(|r| r.min);
                    let effects = self.core.handle(Intent::PointerDown {
                        pos: [pos.x - rect.left(), pos.y - rect.top()],
                        frame_width: rect.width(),
                        now_ms: now,
                    });
                    self.dispatch(ctx, effects);
                    if !maximized_or_fullscreen {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } => {
                    let effects = self.core.handle(Intent::PointerUp {
                        pos: [pos.x - rect.left(), pos.y - rect.top()],
                        now_ms: now,
                    });
                    self.dispatch(ctx, effects);
                }
                egui::Event::PointerButton {
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    ..
                } => {
                    // 右键只取消未完成的左键动作并照常开菜单（egui 上下文菜单），
                    // 从不启动拖动或缩放，也不取消已在播的转场。
                    let effects = self.core.handle(Intent::RightPressed);
                    self.dispatch(ctx, effects);
                }
                egui::Event::PointerMoved(pos) => {
                    let effects = self.core.handle(Intent::PointerMoved {
                        pos: [pos.x - rect.left(), pos.y - rect.top()],
                    });
                    self.dispatch(ctx, effects);
                }
                _ => {}
            }
        }
    }
}
