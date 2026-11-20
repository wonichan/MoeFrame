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
    config::ConfigStore,
    decode_validation::DisplayMode,
    loading::LoadRequest,
    resize_cursor, resize_direction,
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

#[cfg(windows)]
fn app_directory() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join("MoeFrame"))
}

#[cfg(windows)]
fn run() -> Result<(), Box<dyn Error>> {
    let directory = app_directory().ok_or_else(|| io::Error::other("APPDATA is unavailable"))?;
    let config_path = directory.join("config.toml");
    let log_path = directory.join("moe_frame.log");
    let store = ConfigStore::load(&config_path);
    let saved_folder = store.config().image_folder.as_ref().map(PathBuf::from);
    let fit_mode = store.config().display.fit_mode;
    let interval_secs = store.config().playback.interval_secs;
    let window = store.config().window.clone();
    // 配置里存有窗口位置时按存档开窗；没有（首次启动）才走右下角内缩放置。
    let saved_position = match (window.x, window.y) {
        (Some(x), Some(y)) => Some(egui::pos2(x as f32, y as f32)),
        _ => None,
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("MoeFrame")
        .with_decorations(false)
        .with_resizable(true)
        .with_inner_size([window.width as f32, window.height as f32])
        .with_min_inner_size([120.0, 120.0])
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
                core: FrameCore::new(saved_folder, interval_secs),
                store,
                fit_mode,
                command_tx,
                reply_rx,
                started: false,
                placed: saved_position.is_some(),
                texture: None,
                load_seq: 0,
                load_in_flight: false,
                wheel_acc: 0.0,
            }))
        }),
    )?;
    Ok(())
}

#[cfg(windows)]
struct FrameApp {
    core: FrameCore,
    store: ConfigStore,
    fit_mode: DisplayMode,
    command_tx: Sender<Command>,
    reply_rx: Receiver<Reply>,
    started: bool,
    placed: bool,
    texture: Option<egui::TextureHandle>,
    /// 当前一轮加载的序号；新一轮扫描开始即作废在飞的旧加载结果。
    load_seq: u64,
    load_in_flight: bool,
    /// 滚轮平滑滚动量的累积，攒够一格才触发一次切换。
    wheel_acc: f32,
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
                    let text = folder.to_string_lossy().into_owned();
                    // A failed config write must not crash the frame; startup will re-prompt.
                    let _ = self.store.set_image_folder(Some(&text));
                }
                Effect::Scan(folder) => {
                    // 新一轮扫描开始：在飞的旧加载结果从此作废。
                    self.load_seq += 1;
                    let _ = self.command_tx.send(Command::Scan { folder });
                }
                Effect::Load { candidates } => {
                    self.load_seq += 1;
                    self.load_in_flight = true;
                    let request = LoadRequest::product(self.physical_target(ctx), self.fit_mode);
                    let _ = self.command_tx.send(Command::Load {
                        seq: self.load_seq,
                        candidates,
                        request,
                    });
                }
                Effect::Preload { path } => {
                    let request = LoadRequest::product(self.physical_target(ctx), self.fit_mode);
                    let _ = self.command_tx.send(Command::Preload { path, request });
                }
                Effect::SaveInterval(secs) => {
                    // A failed config write must not crash the frame; the menu re-reads the core value.
                    let _ = self.store.set_interval_secs(secs);
                }
                Effect::Exit => {
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

    fn drain_replies(&mut self, ctx: &egui::Context) {
        while let Ok(reply) = self.reply_rx.try_recv() {
            match reply {
                Reply::ScanCompleted(paths) => {
                    let effects = self.core.handle(Intent::ScanCompleted(paths));
                    self.dispatch(ctx, effects);
                }
                Reply::Loaded { seq, image } => {
                    if seq != self.load_seq {
                        // 旧一轮（换文件夹之前）的结果：直接丢弃，不碰当前画面与状态。
                        continue;
                    }
                    self.load_in_flight = false;
                    let shown = image.as_ref().map(|image| image.path.clone());
                    if let Some(image) = image {
                        self.upload(ctx, image);
                    }
                    let effects = self.core.handle(Intent::LoadCompleted {
                        shown,
                        now_ms: now_ms(ctx),
                    });
                    self.dispatch(ctx, effects);
                    // 导航全失败时核心保留当前图，纹理不动；只有核心回到无图状态才清纹理。
                    if self.core.view() == View::Empty {
                        self.texture = None;
                    }
                }
            }
        }
    }

    fn upload(&mut self, ctx: &egui::Context, image: moe_frame::decode_validation::DisplayImage) {
        let size = [
            image.display_pixels[0] as usize,
            image.display_pixels[1] as usize,
        ];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba);
        self.texture =
            Some(ctx.load_texture("moe-frame-current", color, egui::TextureOptions::LINEAR));
    }

    fn place_bottom_right(&mut self, ctx: &egui::Context) {
        if self.placed {
            return;
        }
        let Some(monitor) = ctx.input(|input| input.viewport().monitor_size) else {
            return;
        };
        let rect = ctx.viewport_rect();
        let pos = egui::pos2(
            (monitor.x - rect.width() - 24.0).max(0.0),
            (monitor.y - rect.height() - 24.0).max(0.0),
        );
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
        self.place_bottom_right(&ctx);
        let now = now_ms(&ctx);
        let tick_effects = self.core.handle(Intent::Tick { now_ms: now });
        self.dispatch(&ctx, tick_effects);
        self.handle_keys_and_wheel(&ctx, now);

        let mut menu_intent: Option<Intent> = None;
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter();
                if let Some(texture) = &self.texture {
                    // 适应：无论窗口如何缩放，整图按比例完整放进窗口，余白即黑边。
                    let tex = egui::vec2(texture.size()[0] as f32, texture.size()[1] as f32);
                    let scale = (rect.width() / tex.x).min(rect.height() / tex.y);
                    let image_rect = egui::Rect::from_center_size(rect.center(), tex * scale);
                    painter.image(
                        texture.id(),
                        image_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
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
        panel.response.context_menu(|ui| {
            if ui.button("选择文件夹…").clicked() {
                menu_intent = Some(Intent::FolderPickRequested);
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
            ui.menu_button("切换间隔", |ui| {
                for (label, secs) in [
                    ("10 秒", 10),
                    ("30 秒", 30),
                    ("1 分钟", 60),
                    ("5 分钟", 300),
                    ("10 分钟", 600),
                    ("30 分钟", 1800),
                ] {
                    if ui
                        .selectable_label(secs == current_interval, label)
                        .clicked()
                    {
                        menu_intent = Some(Intent::IntervalSelected { secs, now_ms: now });
                        ui.close();
                    }
                }
            });
            ui.separator();
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
        if self.load_in_flight {
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
    /// Content drag and eight-direction resize, reusing the geometry validated in ticket 08.
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
        // A right-click opens the menu; it must never start a drag or resize.
        if ctx.input(|input| input.pointer.secondary_pressed()) {
            return;
        }
        let pressed = ctx.input(|input| {
            if !input.pointer.primary_down() {
                return None;
            }
            input.events.iter().rev().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
        });
        let Some(pos) = pressed.filter(|pos| rect.contains(*pos)) else {
            return;
        };
        let maximized_or_fullscreen = ctx.input(|input| {
            input.viewport().maximized == Some(true) || input.viewport().fullscreen == Some(true)
        });
        if maximized_or_fullscreen {
            return;
        }
        if let Some(direction) = resize_direction(rect, pos) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }
}
