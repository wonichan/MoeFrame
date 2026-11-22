//! 纹理与显示数据：上传、转正与重载。

use super::*;

impl FrameApp {
    /// 解码口径 = 画面区域的物理尺寸（V2 相框外观专题工单 01）：
    /// 窗口物理尺寸减去四边内距的物理量，不含画框；内距全 0 时
    /// 与 V1 的整窗尺寸逐项等值。显示数据管线的接口不变，变的
    /// 只是宿主喂入的目标值。内距取生效内距（工单 03：自定义
    /// 相框图按检测结果缩放，回退时按内置）。
    pub(crate) fn physical_target(&self, ctx: &egui::Context) -> [u32; 2] {
        let rect = ctx.viewport_rect();
        let ppp = ctx.pixels_per_point();
        let insets = self.effective_insets(rect.width());
        [
            (((rect.width() - insets.horizontal()) * ppp).round() as u32).max(1),
            (((rect.height() - insets.vertical()) * ppp).round() as u32).max(1),
        ]
    }
}
/// 治愈保活时长（毫秒）：口径过期后保活出帧的上限，覆盖窗口尺寸
/// 落地、0.25s 停稳与一次刷新解码的正常耗时；超时未愈合维持原行为
/// （下一次外部唤醒时治愈），不无限保活。
const HEAL_KEEPALIVE_MS: u64 = 2_000;

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
        let issued = self.pipeline.tick(&observation);
        if let Some(key) = &issued {
            let _ = self.command_tx.send(Command::Refresh { key: key.clone() });
        }
        // 治愈保活（.scratch/blur-heal 工单 01）：当前口径仍过期且本帧
        // 没发出刷新时，若过期目标是新出现的变化，给帧尾设保活截止——
        // 停稳计时走的是帧时间，没有后续帧刷新就永远不发（失焦切换后
        // 模糊持续到聚焦才清晰的根源）。同一目标只设一次：刷新失败被
        // 管线按期望键抑制后，目标不变就不再续，保活自然到期。
        let stale = self.pipeline.current_key().is_some_and(|key| {
            key.target != observation.target || key.mode != observation.fit_mode
        });
        if stale && issued.is_none() {
            if self.stale_target_seen != Some(observation.target) {
                self.stale_target_seen = Some(observation.target);
                self.heal_repaint_until_ms = now_ms(ctx) + HEAL_KEEPALIVE_MS;
            }
        } else if !stale {
            self.stale_target_seen = None;
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

    /// 相框图纹理懒上传（V2 相框外观专题工单 03）：分析在启动/
    /// 切换时已完成（当时没有 egui 上下文），像素在首帧上传为
    /// 纹理后释放；已上传或回退/未选时什么都不做。
    pub(crate) fn ensure_frame_art_texture(&mut self, ctx: &egui::Context) {
        if let FrameArtHost::Loaded {
            analysis,
            pixels,
            texture,
        } = &mut self.frame_art
        {
            if texture.is_none() {
                if let Some(rgba) = pixels.take() {
                    let size = [analysis.art_size[0] as usize, analysis.art_size[1] as usize];
                    let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                    *texture = Some(ctx.load_texture(
                        "moe-frame-frame-art",
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
        }
    }
}
