#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("window-validation supports Windows 10/11 only");

#[cfg(windows)]
#[path = "window-validation/native.rs"]
mod native;

#[cfg(windows)]
use eframe::egui;
#[cfg(windows)]
use moe_frame::{
    diagnostics::{SessionLog, WindowObservation},
    NativeWindowState, ValidationEvent, WindowValidation,
};
#[cfg(windows)]
use std::{error::Error, io, path::PathBuf, time::Duration};
#[cfg(windows)]
use winit::platform::windows::WindowExtWindows;

#[cfg(windows)]
fn main() {
    if let Err(error) = run() {
        // A GUI subsystem process has no console; a startup failure must still be visible.
        native::report_startup_error(&error.to_string());
    }
}

#[cfg(windows)]
fn run() -> Result<(), Box<dyn Error>> {
    let appdata =
        std::env::var_os("APPDATA").ok_or_else(|| io::Error::other("APPDATA is unavailable"))?;
    let directory = PathBuf::from(appdata)
        .join("MoeFrame")
        .join("window-validation");
    let os = native::system_version().unwrap_or_else(|error| format!("unknown: {error}"));
    let metadata = format!(
        "application=window-validation {}; os={os}; rustc={}; cargo={}; target={}; profile={}; revision={}; dirty={}; build_unix_ms={}; dependencies={}; renderer=Glow; eframe_features=glow/default_fonts/system_fonts; requested_settings=decorations:false,resizable:true,min_inner_size:120x120,normal_level,active:false,file_drop:false,undecorated_shadow:false,zoom:1; foreground_before_creation={:?}; physical_client=winit PhysicalSize; win32_client=raw GetClientRect; system_logical_client=physical_client/native_scale_factor (derived)",
        env!("CARGO_PKG_VERSION"), env!("WV_RUSTC"), env!("WV_CARGO"), env!("WV_TARGET"),
        env!("WV_PROFILE"), env!("WV_REVISION"), env!("WV_DIRTY"), env!("WV_BUILD_UNIX_MS"),
        env!("WV_DEPENDENCIES"), native::foreground(),
    );
    let mut log = SessionLog::create(&directory, &metadata)?;
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("MoeFrame - Window validation #08")
            .with_decorations(false)
            .with_resizable(true)
            .with_inner_size([480.0, 320.0])
            .with_min_inner_size([120.0, 120.0])
            .with_maximized(false)
            .with_fullscreen(false)
            .with_window_level(egui::WindowLevel::Normal),
        window_builder: Some(Box::new(|builder| {
            builder.with_active(false).with_drag_and_drop(false)
        })),
        ..Default::default()
    };
    let app_log = &mut log;
    let result = eframe::run_native(
        "Window validation #08",
        options,
        Box::new(move |cc| {
            let window = cc
                .winit_window()
                .ok_or_else(|| io::Error::other("native winit window unavailable"))?;
            // egui-winit enables this Windows shadow even without decorations. Disable before first show.
            window.set_undecorated_shadow(false);
            cc.egui_ctx
                .options_mut(|options| options.zoom_with_keyboard = false);
            cc.egui_ctx.set_zoom_factor(1.0);
            app_log.record(
                "created",
                "undecorated shadow disabled before first show; user acceptance remains unverified",
            )?;
            Ok(Box::new(ValidationApp {
                validation: WindowValidation::default(),
                log: app_log,
                last_observation: None,
                log_error: None,
            }))
        }),
    );
    if let Err(error) = result {
        if let Err(log_error) = log.record("backend_error", &error.to_string()) {
            eprintln!("could not write backend diagnostic: {log_error}");
        }
        return Err(error.into());
    }
    Ok(())
}

#[cfg(windows)]
struct ValidationApp<'a> {
    validation: WindowValidation,
    log: &'a mut SessionLog,
    last_observation: Option<WindowObservation>,
    log_error: Option<String>,
}

#[cfg(windows)]
impl eframe::App for ValidationApp<'_> {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let observation = native::observe(&ctx, frame);
        let native =
            observation
                .maximized
                .zip(observation.fullscreen)
                .map(|(maximized, fullscreen)| NativeWindowState {
                    maximized,
                    fullscreen,
                });
        self.validation.show(ui, native);
        let mut snapshot = false;
        for event in self.validation.drain_events() {
            snapshot |= event == ValidationEvent::SnapshotRequested;
            let input = ctx.input(|input| {
                let pointer_events: Vec<_> = input
                    .events
                    .iter()
                    .filter(|event| matches!(event, egui::Event::PointerButton { .. }))
                    .collect();
                format!(
                    "time={:.6}; primary_down={}; pointer_events={pointer_events:?}",
                    input.time,
                    input.pointer.primary_down()
                )
            });
            if let Err(error) = self.log.record("input_action", &format!("{event:?}; native={native:?}; {input}; viewport commands are queued, not verified")) {
                self.log_error = Some(error.to_string());
            }
        }
        if snapshot || self.last_observation.as_ref() != Some(&observation) {
            let reason = if snapshot {
                "manual_snapshot"
            } else {
                "observed_state"
            };
            if let Err(error) = self.log.observe(reason, &observation) {
                self.log_error = Some(error.to_string());
            }
            self.last_observation = Some(observation.clone());
        }
        self.paint(ui, &observation);
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // The window is already closing; a failed last write has no visible place to report to.
        let _ = self.log.record(
            "exit",
            "validation window exited; no human acceptance recorded",
        );
    }
}

#[cfg(windows)]
impl ValidationApp<'_> {
    fn paint(&self, ui: &egui::Ui, observation: &WindowObservation) {
        let rect = ui.ctx().viewport_rect();
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(27, 31, 39));
        let painter = painter.with_clip_rect(rect.shrink(10.0));
        if rect.width() < 360.0 || rect.height() < 260.0 {
            let labels = [
                format!("{:.0} x {:.0}", rect.width(), rect.height()),
                format!("DPI {:?}", observation.dpi),
                "min 120 x 120".to_owned(),
                "F9 / F10 / Esc".to_owned(),
            ];
            for (index, label) in labels.iter().enumerate() {
                painter.text(
                    rect.center() + egui::vec2(0.0, (index as f32 - 1.5) * 19.0),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(11.0),
                    egui::Color32::WHITE,
                );
            }
            return;
        }
        let (moves, resizes) = self.validation.request_counts();
        let log_name = self
            .log
            .path()
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let mut lines = vec![
            "无边框窗口能力验证 #08（不是正式相框）".to_owned(),
            "内容区按下拖动移动；8 逻辑点边带 / 四角缩放".to_owned(),
            "F9 记录快照 · F10 最大化/还原 · Esc 退出".to_owned(),
            format!(
                "egui 逻辑客户区: {:.1} x {:.1}",
                rect.width(),
                rect.height()
            ),
            format!(
                "原生物理客户区: {:?} · Win32: {:?}",
                observation.physical_client, observation.win32_client
            ),
            format!(
                "DPI: {:?} · 系统缩放: {:?} · egui ppp: {:?}",
                observation.dpi, observation.native_scale_factor, observation.pixels_per_point
            ),
            format!(
                "最大化: {:?} · 普通层 / 置顶: {:?}",
                observation.maximized, observation.topmost
            ),
            format!(
                "前台: {:?} · winit 焦点: {:?}",
                observation.foreground, observation.winit_focus
            ),
            format!("移动 / 缩放请求: {moves} / {resizes}（不代表通过）"),
            format!("日志: {log_name}"),
            "人工验收: 未验证；None = 未取得测量值".to_owned(),
        ];
        if let Some(error) = &self.log_error {
            lines.push(format!("日志写入失败: {error}"));
        }
        if let Some(error) = &observation.error {
            lines.push(format!("原生测量未知: {error}"));
        }
        // The last entries are the human-verification note plus any write or measurement errors.
        const MEASURED_LINES: usize = 11;
        for (index, line) in lines.iter().enumerate() {
            let color = if index >= MEASURED_LINES {
                egui::Color32::LIGHT_RED
            } else {
                egui::Color32::WHITE
            };
            painter.text(
                rect.left_top() + egui::vec2(12.0, 16.0 + index as f32 * 23.0),
                egui::Align2::LEFT_TOP,
                line,
                egui::FontId::proportional(13.0),
                color,
            );
        }
    }
}
