//! 指针与键盘输入：手势裁决、方向键与托盘召回。

use super::*;

/// 手势阈值取 Windows 系统设置（工单 15）：拖动阈值、双击时间与距离，
/// 不写死另一套值；系统接口异常时退到与系统默认同量级的值。系统值是物理
/// 像素，按当前缩放换算成 egui 逻辑点，与指针事件坐标同口径。
pub(crate) fn system_gesture_thresholds(pixels_per_point: f32) -> GestureThresholds {
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
impl FrameApp {
    pub(crate) fn handle_keys(&mut self, ctx: &egui::Context, now: u64) {
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
    pub(crate) fn handle_pointer(&mut self, ctx: &egui::Context) {
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
                    modifiers,
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
                            self.native_gesture = Some(NativeGesture::Resize);
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
                        // Shift 拖动拆组（工单 03）：手势开始时采样
                        // Shift——被拖窗 solo（松手不吸附回组，见
                        // geometry 的松手组装）；拖动开始同时是轮询
                        // 调度的重新武装事件。
                        self.solo_drag = modifiers.shift;
                        self.poll_schedule.wake();
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        self.native_gesture = Some(NativeGesture::Move);
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
                    self.native_gesture = None;
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
