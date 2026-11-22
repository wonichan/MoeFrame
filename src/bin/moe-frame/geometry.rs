//! 窗口几何：拖动改尺寸、吸附与恢复默认。

use super::*;

impl FrameApp {
    pub(crate) fn update_window_geometry(&mut self, ctx: &egui::Context) {
        // 转场帧先推进淡化；到点会提交切换并可能顺带起下一段。
        self.advance_crossfade(ctx);
        if self.crossfade.is_some() {
            ctx.request_repaint();
        }
        let pixels_per_point = ctx.pixels_per_point();
        let native_gesture = self.native_gesture_active;
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
                    pointer_down: input.pointer.any_down() || native_gesture,
                    work_area: monitor_work_area(self.hwnd, pixels_per_point),
                    frame_visible: self.core.frame_visible(),
                    now_secs: input.time,
                    // 生效内距（工单 03：自定义相框图按当前内宽缩放，
                    // 回退时按内置）；内宽取不到时按系数 1 处理。
                    insets: self.effective_insets(
                        viewport.inner_rect.map(|rect| rect.width()).unwrap_or(0.0),
                    ),
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
        if self.native_gesture_active && !primary_button_down() {
            self.native_gesture_active = false;
        }
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
        if let Some(deadline) = self.geometry.transition_deadline_secs() {
            let remaining = (deadline - observation.now_secs).max(0.0);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining));
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
                GeomCommand::TransitionInterrupted => {
                    // 用户接管：立即提交切换，不让核心等动画回执。
                    self.finish_crossfade(ctx);
                }
            }
        }
    }
    pub(crate) fn restore_default_geometry(&mut self, ctx: &egui::Context) {
        let monitor = ctx
            .input(|input| input.viewport().monitor_size)
            .map(|size| [size.x, size.y]);
        let commands = self.geometry.restore_default(
            monitor,
            self.core.frame_visible(),
            self.effective_insets(ctx.viewport_rect().width()),
        );
        self.execute_geometry_commands(ctx, commands);
    }
}

/// 相框所在屏的工作区（逻辑坐标）：Win32 物理像素除以窗口 DPI 比例。
/// egui 只给屏尺寸，原点与任务栏扣除必须走 Win32。
#[cfg(windows)]
fn monitor_work_area(hwnd: isize, pixels_per_point: f32) -> Option<[f32; 4]> {
    use windows_sys::Win32::{
        Foundation::HWND,
        Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        },
    };
    if hwnd == 0 {
        return None;
    }
    let monitor = unsafe { MonitorFromWindow(hwnd as HWND, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let scale = if pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    };
    let work = info.rcWork;
    Some([
        work.left as f32 / scale,
        work.top as f32 / scale,
        work.right as f32 / scale,
        work.bottom as f32 / scale,
    ])
}

/// 物理左键是否仍按着：原生拖动/缩放期间 egui 可能收不到指针状态，
/// 用它判断模态循环是否结束。
fn primary_button_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe { GetAsyncKeyState(0x01) as u16 & 0x8000 != 0 }
}
