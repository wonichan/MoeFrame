#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

//! 正式相框（工单 10）：首次启动选文件夹，看到第一张图。
//! 行为规则在 `moe_frame::app_core`，本文件只负责窗口、纹理与效果执行。

#[cfg(not(windows))]
compile_error!("MoeFrame supports Windows 10/11 only");

use raw_window_handle::HasWindowHandle;

#[cfg(windows)]
#[path = "moe-frame/worker.rs"]
mod worker;

#[cfg(windows)]
#[path = "moe-frame/ambient_paint.rs"]
mod ambient_paint;
#[cfg(windows)]
#[path = "moe-frame/app.rs"]
mod app;
#[cfg(windows)]
#[path = "moe-frame/autostart_store.rs"]
mod autostart_store;
#[cfg(windows)]
#[path = "moe-frame/dwm_shadow.rs"]
mod dwm_shadow;
#[cfg(windows)]
#[path = "moe-frame/frame_paint.rs"]
mod frame_paint;
#[cfg(windows)]
#[path = "moe-frame/frame_ui.rs"]
mod frame_ui;
#[cfg(windows)]
#[path = "moe-frame/geometry.rs"]
mod geometry;
#[cfg(windows)]
#[path = "moe-frame/input.rs"]
mod input;
#[cfg(windows)]
#[path = "moe-frame/instance_launch.rs"]
mod instance_launch;
#[cfg(windows)]
#[path = "moe-frame/settings_ui.rs"]
mod settings_ui;
#[cfg(windows)]
#[path = "moe-frame/snap_enum.rs"]
mod snap_enum;
#[cfg(windows)]
#[path = "moe-frame/snap_subclass.rs"]
mod snap_subclass;
#[cfg(windows)]
#[path = "moe-frame/texture.rs"]
mod texture;
#[cfg(windows)]
#[path = "moe-frame/transition.rs"]
mod transition;
#[cfg(windows)]
#[path = "moe-frame/tray.rs"]
mod tray;
#[cfg(test)]
#[path = "moe-frame/ui_regressions.rs"]
mod ui_regressions;

#[cfg(windows)]
use eframe::egui;
#[cfg(windows)]
use moe_frame::{
    app_core::{ConcreteAnimation, Effect, FrameCore, Intent, View},
    autostart::{autostart_command, AutostartStore, AutostartSync, RUN_KEY_PATH, RUN_VALUE_NAME},
    config::{Animation, AppearanceMode, BuiltinFrame, FramePreset, PlaybackMode, MAX_LAYER_WIDTH},
    config_sync::ConfigSync,
    display_data::{DisplayKey, DisplayPipeline, NavDecision, Observation, RefreshDecision},
    frame_art::FrameArtAnalysis,
    frame_layout::{
        builtin_shading, effective_appearance, nine_slice_layout, BuiltinShading, EdgeShade,
        EffectiveAppearance, FadeBand, FrameEdge, LayoutRect, ProfileLines,
    },
    gesture::GestureThresholds,
    loading::{DisplayMode, LoadRequest},
    playlist::Direction,
    resize_cursor, resize_direction,
    settings_session::{
        EffectiveValues, Field, OpenDecision, Picks, SessionAction, SettingsSession,
    },
    window_geom::{GeomCommand, GeomObservation, GeometryMachine, Insets, MIN_WINDOW_SIDE},
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
use ambient_paint::*;
#[cfg(windows)]
use app::*;
#[cfg(windows)]
use autostart_store::*;
#[cfg(windows)]
use input::*;
#[cfg(windows)]
use tray::*;

#[cfg(windows)]
fn main() {
    startup_marker("launch", "phase=process-entry");
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

/// 布局接缝的平面矩形与 egui 矩形互转：宿主各绘制点共用这一份，
/// 不在单个绘制文件里私藏第二套。
#[cfg(windows)]
fn to_layout_rect(rect: egui::Rect) -> LayoutRect {
    LayoutRect::new(rect.left(), rect.top(), rect.right(), rect.bottom())
}

#[cfg(windows)]
fn to_egui_rect(rect: LayoutRect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.left, rect.top),
        egui::pos2(rect.right, rect.bottom),
    )
}

#[cfg(windows)]
fn app_directory() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("MoeFrame"))
}

/// 当前进程工作集（MB）：启动标记自带的资源读数，供发布资源门免采样直读。
#[cfg(windows)]
fn working_set_mb() -> u64 {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCounters {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }
    extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut core::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }
    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        PageFaultCount: 0,
        PeakWorkingSetSize: 0,
        WorkingSetSize: 0,
        QuotaPeakPagedPoolUsage: 0,
        QuotaPagedPoolUsage: 0,
        QuotaPeakNonPagedPoolUsage: 0,
        QuotaNonPagedPoolUsage: 0,
        PagefileUsage: 0,
        PeakPagefileUsage: 0,
    };
    let requested = counters.cb;
    // SAFETY: 两个 Win32 调用都只写 `counters`，`requested` 是它自身的字节数。
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, requested) };
    if ok == 0 {
        0
    } else {
        (counters.WorkingSetSize / (1024 * 1024)) as u64
    }
}

/// 启动耗时标记（NFR-4）：与调试日志同文件、不同前缀，只写一行，不参与行为。
#[cfg(windows)]
fn startup_marker(event: &str, details: &str) {
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
        "[startup] unix_ms={unix_ms} pid={} ws_mb={} event={event} {details}",
        std::process::id(),
        working_set_mb()
    );
}

/// 置顶守擂日志（置顶守擂工单 01）：与启动标记同文件同格式、独立
/// 前缀；只在重申分类变化时由 frame_ui 写一行，不参与行为。
#[cfg(windows)]
fn level_guard_record(event: &str, details: &str) {
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
        "[level-guard] unix_ms={unix_ms} pid={} event={event} {details}",
        std::process::id()
    );
}

/// 窗口投影日志（窗口投影工单 01）：与启动标记同文件同格式、独
/// 立前缀；窗口创建时关 DWM 投影的调用结果只写一行，不参与
/// 行为。
#[cfg(windows)]
fn dwm_shadow_record(event: &str, details: &str) {
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
        "[dwm-shadow] unix_ms={unix_ms} pid={} event={event} {details}",
        std::process::id()
    );
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
    // 实例编号认领（多实例吸附专题工单 01）：认领守卫活到 run()
    // 返回（即进程退出）才释放；配置的图库与几何按这个编号读写
    // 自己的实例段。认领/级联基点/自启恢复的装配细节在
    // instance_launch 模块，run() 只分发（审查修复 J3）。
    // 指定编号已被存活进程持有时安静退出（恢复幂等）。
    let Some(_instance_claim) = instance_launch::claim_from_args(&directory)? else {
        return Ok(());
    };
    let instance_number = _instance_claim.number();
    startup_marker("instance-claimed", &format!("instance={instance_number}"));
    // 级联基点：趁本窗尚未创建，先枚举活伙伴的当前位置（S4）。
    let live_cascade_base = instance_launch::live_cascade_base(&directory);
    // 启动装配（工单 04 深挖）：配置加载/修复、自启对账与全部初值
    // 翻译归 config_sync，run() 只分发启动包。
    let (mut sync, package) = ConfigSync::startup_for_instance_with_cascade(
        &config_path,
        &log_path,
        &mut RegAutostartStore,
        instance_number,
        live_cascade_base,
    );
    // 「在运行」标记置位（工单 04）：走工单 01 写入通道只改本实例
    // 段；正常退出时在 Effect::Exit 落地处清除，崩溃残留不清。
    sync.set_running_marker(true);
    // 自启恢复（工单 04）：1 号实例启动时幂等拉起残留的在运行
    // 编号，裁决与拉起在 instance_launch。
    if instance_number == 1 {
        instance_launch::recover_marked_instances(&directory, &sync);
    }
    // 全局同步基线（工单 04）：启动已应用的全局值 + 此刻的文件修改
    // 时间，同步状态机据此只报基线之后的变化。
    let sync_baseline_mtime = std::fs::metadata(&config_path)
        .and_then(|meta| meta.modified())
        .ok();
    let sync_applied = sync.global_settings();
    let core_args = package.core;
    let host_init = package.host;
    let autostart_sync = package.autostart;
    let window = host_init.window.clone();
    let saved_position = host_init.saved_position.map(|p| egui::pos2(p[0], p[1]));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("MoeFrame")
        .with_decorations(false)
        // 永远透明创建（V2 相框外观专题工单 01）：极简/内置模式由
        // 绘制层自绘不透明背景，视觉与 V1 一致；透明为自定义
        // 相框图的异形外轮廓透出桌面所必需（工单 04），切换模式
        // 不重启。
        .with_transparent(true)
        .with_resizable(true)
        .with_inner_size([window.width as f32, window.height as f32])
        .with_min_inner_size([MIN_WINDOW_SIDE, MIN_WINDOW_SIDE])
        .with_maximized(false)
        .with_fullscreen(false)
        .with_taskbar(false)
        .with_icon(app_icon_data())
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
            startup_marker("app-created", "phase=eframe-init-done");
            resize_debug_record("startup", "diagnostic=resize-f317");
            let hwnd = cc
                .winit_window()
                .and_then(|window| window.window_handle().ok())
                .and_then(|handle| match handle.as_raw() {
                    raw_window_handle::RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
                    _ => None,
                })
                .unwrap_or(0);
            // 工单 05：只给相框窗口挂 WM_MOVING 子类化（拖动中实时
            // 吸附）；设置窗是另一个 HWND，不在此处、不挂。
            snap_subclass::install(hwnd, host_init.window_snap);
            // 窗口投影工单 01：关掉 DWM 给相框窗口加的系统投影
            // （只给本窗、只此一次）；设置窗是另一个 HWND，不
            // 在此处、不动。
            dwm_shadow::disable(hwnd);
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
            let mut app = FrameApp {
                window_level: egui::WindowLevel::Normal,
                level_guard: moe_frame::level_guard::LevelGuard::new(),
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
                first_image_marked: false,
                texture: None,
                incoming: None,
                transition: None,
                hwnd,
                press_window_pos: None,
                native_gesture: None,
                held_autoplay: None,
                tray: None,
                tray_tried: false,
                tray_state: None,
                exiting: false,
                settings_open: false,
                settings_positioned: false,
                pipeline: DisplayPipeline::new(),
                heal_repaint_until_ms: 0,
                stale_target_seen: None,
                geometry: GeometryMachine::new(
                    host_init.saved_geom,
                    None,
                    host_init.saved_position.is_some(),
                ),
                autostart: autostart_sync,
                autostart_store: RegAutostartStore,
                appearance: host_init.appearance,
                window_snap: host_init.window_snap,
                snap_detach_available: false,
                snap_anchor: Vec::new(),
                solo_drag: false,
                instance_number,
                global_sync: moe_frame::global_sync::GlobalSyncState::new(
                    sync_applied,
                    sync_baseline_mtime,
                ),
                frame: host_init.frame,
                frame_art_path: host_init.frame_art.clone(),
                frame_art: FrameArtHost::NotSelected,
                session: SettingsSession::new(),
                debug_last_inner: None,
            };
            // 相框图启动分析（V2 相框外观专题工单 03）：按已保存
            // 路径分析一次；失败置回退态，相框照常打开。
            app.reload_frame_art();
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}
