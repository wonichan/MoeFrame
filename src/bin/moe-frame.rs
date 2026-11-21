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
    display_data::{DisplayKey, DisplayPipeline, NavDecision, Observation, RefreshDecision},
    gesture::GestureThresholds,
    loading::LoadRequest,
    resize_cursor, resize_direction,
    settings_session::{
        EffectiveValues, Field, OpenDecision, Picks, SessionAction, SettingsSession,
    },
    window_geom::{GeomCommand, GeomObservation, GeometryMachine, MIN_WINDOW_SIDE},
};
#[cfg(windows)]
use std::{
    error::Error,
    fs::OpenOptions,
    io::{self, Write},
    path::PathBuf,
    sync::mpsc::{channel, Receiver, Sender},
    time::{Duration, SystemTime, UNIX_EPOCH},
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

#[cfg(windows)]
fn app_directory() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("MoeFrame"))
}

#[cfg(windows)]
fn resize_debug_record(event: &str, details: &str) {
    let Some(directory) = app_directory() else {
        return;
    };
    let path = directory.join("moe_frame.log");
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let _ = writeln!(
        file,
        "[DEBUG-resize-f317] unix_ms={unix_ms} pid={} event={event} {details}",
        std::process::id()
    );
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
            resize_debug_record("startup", "diagnostic=resize-f317");
            let (command_tx, reply_rx) = worker::spawn(log_path);
            let (tray_event_tx, tray_event_rx) = channel();
            let tray_ctx = cc.egui_ctx.clone();
            tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
                let _ = tray_event_tx.send(event);
                tray_ctx.request_repaint();
            }));
            let (menu_event_tx, menu_event_rx) = channel();
            let menu_ctx = cc.egui_ctx.clone();
            tray_icon::menu::MenuEvent::set_event_handler(Some(move |event| {
                let _ = menu_event_tx.send(event);
                menu_ctx.request_repaint();
            }));
            Ok(Box::new(FrameApp {
                core: FrameCore::new(
                    core_args,
                    system_gesture_thresholds(cc.egui_ctx.pixels_per_point()),
                ),
                sync,
                command_tx,
                reply_rx,
                tray_event_rx,
                menu_event_rx,
                started: false,
                texture: None,
                incoming: None,
                press_window_pos: None,
                tray: None,
                tray_tried: false,
                tray_state: None,
                exiting: false,
                settings_open: false,
                settings_positioned: false,
                pipeline: DisplayPipeline::new(),
                geometry: GeometryMachine::new(
                    host_init.saved_geom,
                    None,
                    host_init.saved_position.is_some(),
                ),
                autostart: autostart_sync,
                autostart_store: RegAutostartStore,
                session: SettingsSession::new(),
                debug_last_inner: None,
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
        .with_menu_on_left_click(false)
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

#[cfg(windows)]
fn tray_click_intent(
    button: tray_icon::MouseButton,
    button_state: tray_icon::MouseButtonState,
) -> Option<Intent> {
    match (button, button_state) {
        (tray_icon::MouseButton::Left, tray_icon::MouseButtonState::Up) => {
            Some(Intent::ToggleVisibilityRequested)
        }
        _ => None,
    }
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

#[cfg(windows)]
struct FrameApp {
    core: FrameCore,
    sync: ConfigSync,
    command_tx: Sender<Command>,
    reply_rx: Receiver<Reply>,
    tray_event_rx: Receiver<tray_icon::TrayIconEvent>,
    menu_event_rx: Receiver<tray_icon::menu::MenuEvent>,
    started: bool,
    texture: Option<egui::TextureHandle>,
    /// 加载完成、等待核心完成显示提交的目标纹理。
    incoming: Option<egui::TextureHandle>,
    /// 本次左键按下时窗口的外框位置：右键取消拖动时回滚到这里（工单 15）。
    press_window_pos: Option<egui::Pos2>,
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
    /// 窗口几何状态机（工单 20 的生命周期，architecture-deepening
    /// 工单 05 起归库 window_geom）：宿主每帧喂观察、执行命令，几何
    /// 落盘经 config_sync，退出结算经它的 exit_settle 交接。
    geometry: GeometryMachine,
    /// 开机自启的真实状态（工单 21）：菜单与设置窗都读它；注册表实现
    /// 经接缝注入，切换失败时状态不动（回滚）。
    autostart: AutostartSync,
    autostart_store: RegAutostartStore,
    /// 设置窗会话（工单 19 的时序，architecture-deepening 工单 02 起归
    /// 库 settings_session）：草稿、基线、待结算与焦点记忆都在其中，
    /// 宿主只采样事件、渲染、执行它返回的动作。
    session: SettingsSession,
    debug_last_inner: Option<[f32; 2]>,
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
                    self.pipeline.invalidate();
                    // 新一轮扫描开始：作废在飞加载，画面回到核心仍在显示的源图。
                    self.incoming = None;
                    let _ = self.command_tx.send(Command::Scan { folder });
                }
                Effect::StartTransition { .. } => {
                    let effects = self.core.handle(Intent::TransitionCompleted {
                        now_ms: now_ms(ctx),
                    });
                    self.dispatch(ctx, effects);
                }
                Effect::Load { candidates } => {
                    // 序号与在飞口径记账归显示数据管线：回送按序号裁决，
                    // 转正时当前槽记为这次加载的口径。
                    let target = self.physical_target(ctx);
                    let seq = self.pipeline.begin_load(DisplayMode::Fit, target);
                    let request = LoadRequest::product(target, DisplayMode::Fit);
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
                        mode: DisplayMode::Fit,
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
                    // 退出收尾归 config_sync：几何状态机交出（最近
                    // 观察值、落盘基准），shutdown 一次结清（工单 20
                    // 的退出冲刷、工单 04 的收尾职责）。
                    let (observed, saved) = self.geometry.exit_settle();
                    self.sync.shutdown(observed, saved);
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
    /// （工单 04 改道）：文件夹/间隔/模式经核心的 Save 系列效果，窗口几何、自启镜像经同步写回。
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
                if ui.button("恢复默认位置与大小").clicked() {
                    picks_ref.restore_default = true;
                }
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
    /// 不经核心提交。窗口尺寸改变与图片切换均经这里刷新当前显示数据。
    fn refresh_display_data(&mut self, ctx: &egui::Context) {
        let showing = match self.core.view() {
            View::Showing(path) => Some(path.clone()),
            _ => None,
        };
        let observation = Observation {
            showing,
            fit_mode: DisplayMode::Fit,
            target: self.physical_target(ctx),
            slide_active: false,
            now_secs: ctx.input(|input| input.time),
        };
        if let Some(key) = self.pipeline.tick(&observation) {
            let _ = self.command_tx.send(Command::Refresh { key });
        }
    }

    /// 窗口几何维护（工单 20 的生命周期，工单 05 起归库状态机）：
    /// 每帧采样观察值喂给几何状态机，执行它返回的抽象命令。
    fn update_window_geometry(&mut self, ctx: &egui::Context) {
        let (observation, focused) = ctx.input(|input| {
            let viewport = input.viewport();
            (
                GeomObservation {
                    outer_pos: viewport.outer_rect.map(|rect| [rect.min.x, rect.min.y]),
                    outer_size: viewport
                        .outer_rect
                        .map(|rect| [rect.width(), rect.height()]),
                    inner: viewport
                        .inner_rect
                        .map(|rect| [rect.width(), rect.height()]),
                    monitor: viewport.monitor_size.map(|size| [size.x, size.y]),
                    pointer_down: input.pointer.any_down(),
                    frame_visible: self.core.frame_visible(),
                    now_secs: input.time,
                },
                viewport.focused,
            )
        });
        if let Some(inner) = observation.inner {
            let changed = self.debug_last_inner.is_none_or(|last| {
                (last[0] - inner[0]).abs() > 0.5 || (last[1] - inner[1]).abs() > 0.5
            });
            if changed {
                resize_debug_record(
                    "inner_size_observed",
                    &format!(
                        "inner={inner:?} outer={:?} pointer_down={} focused={focused:?} ui_ms={:.0}",
                        observation.outer_size,
                        observation.pointer_down,
                        observation.now_secs * 1000.0
                    ),
                );
                self.debug_last_inner = Some(inner);
            }
        }
        let commands = self.geometry.observe(&observation);
        for command in &commands {
            if let GeomCommand::SetInnerSize(requested) = command {
                resize_debug_record(
                    "inner_size_command",
                    &format!(
                        "observed={:?} requested={requested:?} pointer_down={} focused={focused:?} ui_ms={:.0}",
                        observation.inner,
                        observation.pointer_down,
                        observation.now_secs * 1000.0
                    ),
                );
            }
        }
        self.execute_geometry_commands(ctx, commands);
    }

    /// 执行几何状态机的抽象命令：位置/尺寸翻译成 ViewportCommand，
    /// 保存类经 config_sync 落盘，ShowFrame 经行为核心显示相框。
    fn execute_geometry_commands(&mut self, ctx: &egui::Context, commands: Vec<GeomCommand>) {
        for command in commands {
            match command {
                GeomCommand::SetOuterPosition(pos) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                        pos[0], pos[1],
                    )));
                }
                GeomCommand::SetInnerSize(size) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        size[0], size[1],
                    )));
                }
                GeomCommand::SaveGeometry(geom) => {
                    self.sync.save_window_geometry(geom);
                }
                GeomCommand::ShowFrame => {
                    let effects = self.core.handle(Intent::ToggleVisibilityRequested);
                    self.dispatch(ctx, effects);
                }
            }
        }
    }

    /// 恢复默认位置与大小（工单 20）：裁决与命令组成归几何状态机，
    /// 这里只采样显示器与可见性并执行。
    fn restore_default_geometry(&mut self, ctx: &egui::Context) {
        let monitor = ctx
            .input(|input| input.viewport().monitor_size)
            .map(|size| [size.x, size.y]);
        let commands = self
            .geometry
            .restore_default(monitor, self.core.frame_visible());
        self.execute_geometry_commands(ctx, commands);
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
                        self.geometry.set_image_aspect(Some(
                            image.source_pixels[0] as f32 / image.source_pixels[1] as f32,
                        ));
                        self.upload(ctx, image);
                    }
                    let effects = self.core.handle(Intent::LoadCompleted {
                        shown,
                        now_ms: now_ms(ctx),
                    });
                    self.dispatch(ctx, effects);
                    if self.incoming.is_some() {
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
                    // 当前图刷新只替换纹理；失败或图像比例变更时保留旧图状态。
                    let showing = match self.core.view() {
                        View::Showing(path) => Some(path.clone()),
                        _ => None,
                    };
                    let decision = self.pipeline.refresh_reply(
                        image.as_ref().map(|image| image.path.as_path()),
                        showing.as_deref(),
                        false,
                    );
                    if decision == RefreshDecision::Swap {
                        if let Some(image) = image {
                            self.geometry.set_image_aspect(Some(
                                image.source_pixels[0] as f32 / image.source_pixels[1] as f32,
                            ));
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
        // 暂存新纹理，待核心提交显示后转正；当前宿主不播放转场动画。
        self.incoming =
            Some(ctx.load_texture("moe-frame-incoming", color, egui::TextureOptions::LINEAR));
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
        while let Ok(event) = self.tray_event_rx.try_recv() {
            if let tray_icon::TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = event
            {
                if let Some(intent) = tray_click_intent(button, button_state) {
                    tray_intents.push(intent);
                }
            }
        }
        if let Some(tray) = &self.tray {
            while let Ok(event) = self.menu_event_rx.try_recv() {
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
        self.handle_keys(&ctx, now);
        let mut menu_intent: Option<Intent> = None;
        let mut menu_settings = false;
        let mut menu_autostart: Option<bool> = None;
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter();
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                if let Some(texture) = &self.texture {
                    painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
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
        panel
            .response
            .interact(egui::Sense::click())
            .context_menu(|ui| {
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
    /// 聚焦时的左右方向键与空格：与菜单共用同一组导航意图。
    fn handle_keys(&mut self, ctx: &egui::Context, now: u64) {
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
            let popup_open = egui::Popup::is_any_open(ctx);
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } if rect.contains(*pos) => {
                    let edge_distances = [
                        pos.x - rect.left(),
                        rect.right() - pos.x,
                        pos.y - rect.top(),
                        rect.bottom() - pos.y,
                    ];
                    let direction = resize_direction(rect, *pos);
                    if edge_distances.iter().any(|distance| *distance <= 24.0) {
                        let (focused, pixels_per_point) =
                            ctx.input(|input| (input.viewport().focused, input.pixels_per_point()));
                        resize_debug_record(
                            "edge_press",
                            &format!(
                                "pos={pos:?} rect={rect:?} edge_distances={edge_distances:?} hit={direction:?} popup={popup_open} maximized_or_fullscreen={maximized_or_fullscreen} focused={focused:?} pixels_per_point={pixels_per_point}",
                            ),
                        );
                    }
                    if popup_open {
                        continue;
                    }
                    if let Some(direction) = direction {
                        if !maximized_or_fullscreen {
                            resize_debug_record(
                                "begin_resize",
                                &format!("direction={direction:?} pos={pos:?} rect={rect:?}"),
                            );
                            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                        } else {
                            resize_debug_record(
                                "resize_blocked",
                                &format!("direction={direction:?} maximized_or_fullscreen=true"),
                            );
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

#[cfg(test)]
mod ui_regressions {
    use eframe::egui::{
        self, CentralPanel, Color32, Context, Event, Frame, Modifiers, PointerButton, Pos2,
        RawInput, Rect, Vec2,
    };

    fn frame(ctx: &Context, events: Vec<Event>) -> bool {
        let mut opened = false;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 200.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let panel = CentralPanel::default()
                .frame(Frame::NONE.fill(Color32::BLACK))
                .show(ui, |ui| {
                    ui.painter().rect_filled(
                        ui.max_rect(),
                        egui::CornerRadius::ZERO,
                        Color32::BLACK,
                    );
                });
            panel
                .response
                .interact(egui::Sense::click())
                .context_menu(|ui| {
                    ui.label("frame menu");
                });
            opened = panel.response.context_menu_opened();
        });
        output.textures_delta.clear();
        opened
    }

    #[test]
    fn the_frame_context_menu_opens_on_secondary_click() {
        let ctx = Context::default();
        let pos = Pos2::new(100.0, 100.0);
        frame(&ctx, vec![Event::PointerMoved(pos)]);
        frame(
            &ctx,
            vec![Event::PointerButton {
                pos,
                button: PointerButton::Secondary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(frame(
            &ctx,
            vec![Event::PointerButton {
                pos,
                button: PointerButton::Secondary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        ));
    }
    #[test]
    fn tray_left_click_toggles_visibility_and_right_click_is_reserved_for_menu() {
        use super::tray_click_intent;

        assert!(matches!(
            tray_click_intent(
                tray_icon::MouseButton::Left,
                tray_icon::MouseButtonState::Up
            ),
            Some(super::Intent::ToggleVisibilityRequested)
        ));
        assert!(tray_click_intent(
            tray_icon::MouseButton::Left,
            tray_icon::MouseButtonState::Down
        )
        .is_none());
        assert!(tray_click_intent(
            tray_icon::MouseButton::Right,
            tray_icon::MouseButtonState::Up
        )
        .is_none());
    }
}
