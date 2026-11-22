//! 窗口几何：拖动改尺寸、吸附与恢复默认。

use super::*;

impl FrameApp {
    pub(crate) fn update_window_geometry(&mut self, ctx: &egui::Context) {
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
    pub(crate) fn execute_geometry_commands(
        &mut self,
        ctx: &egui::Context,
        commands: Vec<GeomCommand>,
    ) {
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
                    // 几何机只在相框隐藏时发本命令：核心按 !visible
                    // 分支显示，焦点标志不影响结果，照常采样递交。
                    let effects = self.core.handle(Intent::ToggleVisibilityRequested {
                        focused: frame_focused(ctx),
                    });
                    self.dispatch(ctx, effects);
                }
            }
        }
    }
    pub(crate) fn restore_default_geometry(&mut self, ctx: &egui::Context) {
        let monitor = ctx
            .input(|input| input.viewport().monitor_size)
            .map(|size| [size.x, size.y]);
        let commands = self
            .geometry
            .restore_default(monitor, self.core.frame_visible());
        self.execute_geometry_commands(ctx, commands);
    }
}
