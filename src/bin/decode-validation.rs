//! 独立解码/内存验证器。所有日志为观测；不自动验收产品 Limits 或峰值线。
#[path = "decode-validation/config.rs"]
mod config;
#[cfg(windows)]
#[path = "decode-validation/observation.rs"]
mod observation;
#[cfg(windows)]
#[path = "decode-validation/worker.rs"]
mod worker;

#[cfg(windows)]
use {
    eframe::egui,
    moe_frame::{
        decode_validation::{DecodeRequest, DisplayImage, DisplayMode},
        diagnostics::SessionLog,
    },
    observation::{Observation, Sampler},
    std::{
        path::PathBuf,
        sync::{atomic::Ordering, Arc},
        time::{Duration, Instant},
    },
};

fn main() {
    let config = match config::Config::parse(std::env::args_os().skip(1)) {
        Ok(Some(config)) => config,
        Ok(None) => {
            print!("{}", config::HELP);
            return;
        }
        Err(error) => {
            eprintln!("{error}\n{}", config::HELP);
            std::process::exit(2);
        }
    };
    #[cfg(windows)]
    if let Err(error) = run(config) {
        eprintln!("{error}");
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        let _ = config;
        eprintln!(
            "This validator requires Windows GetProcessMemoryInfo; no substitute RSS is reported."
        );
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn run(config: config::Config) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = format!("rustc={}; cargo={}; target={}; profile={}; revision={}; dirty={}; build_unix_ms={}; dependencies={}; system={}; config={config:?}; requested_logical_client=[480,320]; renderer=glow; units=bytes; sampled_process_working_set_excludes_GPU_memory; quiescent=visible_UI_after_at_least_1s_since_preparation; no_working_set_trimming",
        env!("WV_RUSTC"), env!("WV_CARGO"), env!("WV_TARGET"), env!("WV_PROFILE"), env!("WV_REVISION"), env!("WV_DIRTY"),
        env!("WV_BUILD_UNIX_MS"), env!("WV_DEPENDENCIES"), observation::system_version()?);
    let log =
        SessionLog::create_with_schema(&config.log_dir, "decode-memory-validation-v1", &metadata)?;
    println!("Validation log: {}", log.path().display());
    let observation = Arc::new(Observation::new(log));
    observation.record(
        "decode_policy",
        if cfg!(panic = "unwind") {
            "panic=unwind; catch only decoder operations; allocator abort/OOM remains unrecoverable"
        } else {
            "panic=abort; loader refuses image input because decoder panic cannot be isolated"
        },
    );
    observation.sample();
    let mut sampler = Sampler::start(observation.clone(), config.sample_period);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([480.0, 320.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_icon(egui::IconData::default()),
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        system_font_fallback: config.system_font_fallback,
        ..Default::default()
    };
    let font_insert = match &config.font_file {
        Some(path) => Some((
            path.clone(),
            std::fs::read(path)
                .map_err(|e| format!("cannot read --font-file {}: {e}", path.display()))?,
        )),
        None => None,
    };
    let app_observation = observation.clone();
    let result = eframe::run_native(
        "MoeFrame decode memory validation",
        options,
        Box::new(move |cc| {
            if let Some((path, bytes)) = font_insert {
                let byte_count = bytes.len();
                cc.egui_ctx.add_font(egui::FontInsert {
                    name: "single-file-fallback".to_owned(),
                    data: egui::FontData::from_owned(bytes),
                    families: vec![
                        egui::InsertFontFamily {
                            family: egui::FontFamily::Proportional,
                            priority: egui::FontPriority::Lowest,
                        },
                        egui::InsertFontFamily {
                            family: egui::FontFamily::Monospace,
                            priority: egui::FontPriority::Lowest,
                        },
                    ],
                });
                app_observation.record(
                    "font_file_registered",
                    &format!("path={path:?}; bytes={byte_count}"),
                );
            }
            if let Some(gl) = &cc.gl {
                use eframe::glow::HasContext;
                // Query GL metadata without reading pixels or creating diagnostic textures.
                let info = unsafe {
                    format!(
                        "vendor={:?}; renderer={:?}; version={:?}; max_texture_side={}",
                        gl.get_parameter_string(eframe::glow::VENDOR),
                        gl.get_parameter_string(eframe::glow::RENDERER),
                        gl.get_parameter_string(eframe::glow::VERSION),
                        gl.get_parameter_i32(eframe::glow::MAX_TEXTURE_SIZE)
                    )
                };
                app_observation.record("gl_info", &info);
                app_observation.sample();
            }
            Ok(Box::new(Validator::new(
                config,
                app_observation,
                cc.egui_ctx.clone(),
            )))
        }),
    );
    sampler.stop(&observation);
    observation
        .phase
        .store(observation::CLEARING, Ordering::Relaxed);
    observation.sample();
    observation.record("run_end", &format!("prepared={}; rejected={}; draw_submissions={}; samples={}; failure={:?}; native_result={result:?}",
        observation.prepared.load(Ordering::Relaxed), observation.rejected.load(Ordering::Relaxed),
        observation.shown.load(Ordering::Relaxed), observation.samples.load(Ordering::Relaxed), observation.failure()));
    result?;
    if let Some(error) = observation.failure() {
        return Err(std::io::Error::other(error).into());
    }
    Ok(())
}

#[cfg(windows)]
struct Texture {
    path: PathBuf,
    source: [u32; 2],
    display: [u32; 2],
    bytes: usize,
    handle: egui::TextureHandle,
}
#[cfg(windows)]
struct Validator {
    config: config::Config,
    observation: Arc<Observation>,
    worker: worker::Worker,
    request: Option<DecodeRequest>,
    pending: bool,
    preload_after_draw: bool,
    current: Option<Texture>,
    next: Option<Texture>,
    last_ready: Option<Instant>,
    run_started: Option<Instant>,
    draw_pending: bool,
    last_window: Option<String>,
    last_heartbeat: Instant,
    stopped: bool,
    last_font_observation: Option<Instant>,
}
#[cfg(windows)]
impl Validator {
    fn new(config: config::Config, observation: Arc<Observation>, ctx: egui::Context) -> Self {
        let worker = worker::Worker::start(config.paths.clone(), observation.clone(), ctx);
        Self {
            config,
            observation,
            worker,
            request: None,
            pending: false,
            preload_after_draw: false,
            current: None,
            next: None,
            last_ready: None,
            run_started: None,
            draw_pending: false,
            last_window: None,
            last_font_observation: None,
            last_heartbeat: Instant::now(),
            stopped: false,
        }
    }
    fn upload(&self, ctx: &egui::Context, image: &DisplayImage, role: &str) -> Texture {
        self.observation
            .phase
            .store(observation::UPLOADING, Ordering::Relaxed);
        self.observation.record(
            "texture_set_requested",
            &format!(
                "role={role}; path={:?}; display_pixels={:?}; rgba_bytes={}",
                image.path,
                image.display_pixels,
                image.rgba.len()
            ),
        );
        let handle = ctx.load_texture(
            role,
            egui::ColorImage::from_rgba_unmultiplied(
                [
                    image.display_pixels[0] as usize,
                    image.display_pixels[1] as usize,
                ],
                &image.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
        self.observation.sample();
        Texture {
            path: image.path.clone(),
            source: image.source_pixels,
            display: image.display_pixels,
            bytes: image.rgba.len(),
            handle,
        }
    }
    fn retire(&self, slot: Option<Texture>) {
        if let Some(texture) = slot {
            let path = texture.path.clone();
            let id = texture.handle.id();
            drop(texture);
            self.observation.record(
                "texture_free_requested",
                &format!(
                    "path={path:?}; id={id:?}; backend consumes the free delta after painting"
                ),
            );
        }
    }
    fn publish_texture_bytes(&self) {
        let bytes = self
            .current
            .iter()
            .chain(self.next.iter())
            .map(|t| t.bytes)
            .sum();
        self.observation
            .texture_metadata_bytes
            .store(bytes, Ordering::Relaxed);
    }
    fn clear_textures(&mut self) {
        let current = self.current.take();
        let next = self.next.take();
        self.retire(current);
        self.retire(next);
        self.publish_texture_bytes();
    }
    fn submit(&mut self, command: worker::Command) {
        match self.worker.submit(command) {
            Ok(()) => {
                self.pending = true;
                self.last_ready = None;
                self.observation
                    .phase
                    .store(observation::AWAITING_UPLOAD, Ordering::Relaxed);
            }
            Err(error) => self.observation.fail(error),
        }
    }
    fn advance(&mut self, ctx: &egui::Context) {
        if self.pending || self.next.is_none() {
            return;
        }
        let old = self.current.take();
        self.retire(old);
        self.current = self.next.take();
        self.draw_pending = true;
        self.publish_texture_bytes();
        self.observation.record(
            "preload_promoted",
            &format!(
                "path={:?}; no current texture re-upload",
                self.current.as_ref().map(|t| &t.path)
            ),
        );
        self.submit(worker::Command::Advance);
        ctx.request_repaint();
    }
    fn apply(&mut self, ready: worker::Ready, ctx: &egui::Context) {
        match ready.kind {
            worker::ReadyKind::Current | worker::ReadyKind::Advanced => {
                let Some(image) = &ready.snapshot.current else {
                    self.observation.record(
                        "empty_cycle",
                        "all paths rejected; no display; close without a passing verdict",
                    );
                    self.observation
                        .fail("all input images were rejected".into());
                    return;
                };
                if ready.kind == worker::ReadyKind::Advanced {
                    if !self
                        .current
                        .as_ref()
                        .is_some_and(|t| t.path == image.path && t.display == image.display_pixels)
                    {
                        self.observation
                            .fail("worker/texture promotion mismatch".into());
                        return;
                    }
                } else {
                    self.clear_textures();
                    self.current = Some(self.upload(ctx, image, "current"));
                    self.draw_pending = true;
                    self.publish_texture_bytes();
                }
                // 当前图先发布；预加载请求等本帧绘制提交后再发出（见 ui 的 draw_submitted 处）。
                self.pending = true;
                self.preload_after_draw = true;
                self.last_ready = Some(Instant::now());
            }
            worker::ReadyKind::Preload => {
                let old_next = self.next.take();
                self.retire(old_next);
                self.next = ready
                    .snapshot
                    .next
                    .as_ref()
                    .map(|image| self.upload(ctx, image, "next"));
                self.publish_texture_bytes();
                self.pending = false;
                // The snapshot (RGBA owners) drops here. Only the worker keeps current/next pixel buffers.
                self.observation
                    .phase
                    .store(observation::SETTLING, Ordering::Relaxed);
            }
        }
    }
    fn observe_fonts(&mut self, ctx: &egui::Context) {
        if self
            .last_font_observation
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        let info = ctx.fonts(|fonts| {
            let size = fonts.font_image_size();
            format!("atlas_pixels={size:?}; theoretical_atlas_rgba_bytes={}; discovered_fallback_fonts={:?}; cached_galleys={}; image_texture_metadata_excludes_font_atlas_and_actual_GPU_allocations",
                (size[0] as u64) * (size[1] as u64) * 4, fonts.discovered_fonts().iter().map(|font| &font.name).collect::<Vec<_>>(), fonts.num_galleys_in_cache())
        });
        self.observation.record("font_state", &info);
        self.last_font_observation = Some(Instant::now());
    }
    fn shutdown(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        self.observation
            .phase
            .store(observation::CLEARING, Ordering::Relaxed);
        self.clear_textures();
        self.worker.stop(&self.observation);
        self.observation.record("session_stopped", &format!("display_elapsed_ms={:?}; texture frees requested; renderer teardown is not a process-memory verdict", self.run_started.map(|t| t.elapsed().as_millis())));
    }
}

#[cfg(windows)]
impl eframe::App for Validator {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(50));
        if self.observation.failure().is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let Some(window) = frame.winit_window() else {
            self.observation
                .fail("live winit window unavailable; physical target is unknown".into());
            return;
        };
        let physical = window.inner_size();
        let logical = ctx.viewport_rect().size();
        let window_info = format!("logical_client=[{},{}]; physical_client=[{},{}]; native_scale_factor={}; pixels_per_point={}; visible={:?}; minimized={:?}; mode={:?}",
            logical.x, logical.y, physical.width, physical.height, window.scale_factor(), ctx.pixels_per_point(), window.is_visible(), window.is_minimized(), self.config.mode);
        if self.last_window.as_ref() != Some(&window_info) {
            self.observation.record("window", &window_info);
            self.last_window = Some(window_info);
        }
        if window.is_visible() != Some(true) || window.is_minimized() != Some(false) {
            self.observation
                .phase
                .store(observation::HIDDEN, Ordering::Relaxed);
        }
        if !self.config.baseline && !self.pending && physical.width != 0 && physical.height != 0 {
            let request = DecodeRequest {
                target_pixels: [physical.width, physical.height],
                mode: self.config.mode,
                limits: self
                    .config
                    .limits
                    .expect("non-baseline config has explicit Limits"),
            };
            if self.request != Some(request) {
                self.clear_textures();
                self.request = Some(request);
                self.submit(worker::Command::Configure(request));
            }
        }
        if self
            .run_started
            .is_some_and(|start| self.config.duration.is_some_and(|d| start.elapsed() >= d))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.last_heartbeat.elapsed() >= Duration::from_secs(1) {
            self.observation.record("heartbeat", &format!("display_elapsed_ms={:?}; prepared={}; rejected={}; draw_submissions={}; pending={}; visible={:?}; minimized={:?}",
                self.run_started.map(|t| t.elapsed().as_millis()), self.observation.prepared.load(Ordering::Relaxed),
                self.observation.rejected.load(Ordering::Relaxed), self.observation.shown.load(Ordering::Relaxed), self.pending, window.is_visible(), window.is_minimized()));
            self.last_heartbeat = Instant::now();
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        match self.worker.poll() {
            Ok(Some(ready)) => self.apply(ready, &ctx),
            Ok(None) => {}
            Err(error) => self.observation.fail(error),
        }
        if self
            .last_ready
            .is_some_and(|t| t.elapsed() >= self.config.interval)
        {
            self.advance(&ctx);
        }
        let rect = ui.max_rect();
        ui.painter()
            .rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 24));
        if let Some(texture) = &self.current {
            let size = egui::vec2(texture.display[0] as f32, texture.display[1] as f32)
                / ctx.pixels_per_point();
            let destination = egui::Rect::from_center_size(rect.center(), size);
            ui.painter().image(
                texture.handle.id(),
                destination,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            if self.draw_pending {
                self.observation.record("draw_submitted", &format!("path={:?}; source_pixels={:?}; display_pixels={:?}; mode={:?}; target_pixels={:?}", texture.path, texture.source, texture.display, self.config.mode, self.request.map(|r| r.target_pixels)));
                self.observation.shown.fetch_add(1, Ordering::Relaxed);
                self.draw_pending = false;
                if self.run_started.is_none() {
                    self.run_started = Some(Instant::now());
                    self.observation.record(
                        "display_started",
                        "first image draw submitted; duration starts here",
                    );
                }
            }
            if self.preload_after_draw {
                self.preload_after_draw = false;
                // 当前图绘制已提交，此刻才请求预加载下一张；submit 会清 last_ready，先保留再恢复。
                let ready_at = self.last_ready;
                self.submit(worker::Command::PreloadNext);
                self.last_ready = ready_at;
            }
        } else if self.config.baseline && self.run_started.is_none() {
            self.run_started = Some(Instant::now());
            self.last_ready = Some(Instant::now());
            self.observation.record(
                "baseline_started",
                "empty GUI; no input image or decoder allocations",
            );
        }
        if !self.pending
            && self
                .last_ready
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(1))
            && frame
                .winit_window()
                .is_some_and(|w| w.is_visible() == Some(true) && w.is_minimized() == Some(false))
        {
            self.observation
                .phase
                .store(observation::QUIESCENT, Ordering::Relaxed);
        }
        egui::Area::new(egui::Id::unique("validation_controls"))
            .fixed_pos(rect.min + egui::vec2(6.0, 6.0))
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(if self.config.baseline {
                        "Empty GUI baseline"
                    } else {
                        "Decode + preload validation"
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.pending && self.next.is_some(),
                                egui::Button::new("Next"),
                            )
                            .clicked()
                        {
                            self.advance(&ctx);
                        }
                        if ui
                            .add_enabled(
                                !self.pending && !self.config.baseline,
                                egui::Button::new("Fit / Fill"),
                            )
                            .clicked()
                        {
                            self.config.mode = if self.config.mode == DisplayMode::Fit {
                                DisplayMode::Fill
                            } else {
                                DisplayMode::Fit
                            };
                            self.request = None;
                            self.last_ready = None;
                            self.clear_textures();
                            ctx.request_repaint();
                        }
                        if ui.button("Close").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    if let Some(texture) = &self.current {
                        ui.label(
                            texture
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy(),
                        );
                        ui.label(format!(
                            "{}x{} -> {}x{}; {:?}",
                            texture.source[0],
                            texture.source[1],
                            texture.display[0],
                            texture.display[1],
                            self.config.mode
                        ));
                    }
                    ui.label(format!(
                        "WS {:.1} MB; process peak {:.1} MB",
                        self.observation.working_set_bytes.load(Ordering::Relaxed) as f64
                            / 1_000_000.0,
                        self.observation
                            .peak_working_set_bytes
                            .load(Ordering::Relaxed) as f64
                            / 1_000_000.0
                    ));
                    ui.label(format!(
                        "Prepared {}; rejected {}; {}",
                        self.observation.prepared.load(Ordering::Relaxed),
                        self.observation.rejected.load(Ordering::Relaxed),
                        if self.pending { "loading..." } else { "ready" }
                    ));
                    ui.label("Observations only; no acceptance verdict");
                });
            });
        self.observe_fonts(&ctx);
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.shutdown();
    }
}
#[cfg(windows)]
impl Drop for Validator {
    fn drop(&mut self) {
        self.shutdown();
    }
}
