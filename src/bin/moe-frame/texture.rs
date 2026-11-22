//! 纹理与显示数据：上传、转正与重载。

use super::*;

impl FrameApp {
    pub(crate) fn physical_target(&self, ctx: &egui::Context) -> [u32; 2] {
        let rect = ctx.viewport_rect();
        let ppp = ctx.pixels_per_point();
        [
            ((rect.width() * ppp).round() as u32).max(1),
            ((rect.height() * ppp).round() as u32).max(1),
        ]
    }
}
impl FrameApp {
    pub(crate) fn refresh_display_data(&mut self, ctx: &egui::Context) {
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
}
impl FrameApp {
    pub(crate) fn upload(
        &mut self,
        ctx: &egui::Context,
        image: moe_frame::decode_validation::DisplayImage,
    ) {
        let size = [
            image.display_pixels[0] as usize,
            image.display_pixels[1] as usize,
        ];
        // 首张图口径：纹理上传即视为「到首张图显示」，供 NFR-4 启动测量标记；只写日志。
        if !self.first_image_marked && self.texture.is_none() {
            self.first_image_marked = true;
            startup_marker("first-image", "phase=texture-upload");
        }
        let color = egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba);
        // 暂存新纹理，待核心提交显示后转正；当前宿主不播放转场动画。
        self.incoming =
            Some(ctx.load_texture("moe-frame-incoming", color, egui::TextureOptions::LINEAR));
    }
}
