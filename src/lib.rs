//! 工单 08 / 09 的独立验证入口；不包含正式相框实现。

use eframe::egui;

pub mod app_core;
pub mod autostart;
pub mod config;
pub mod config_sync;
pub mod decode_validation;
pub mod diagnostics;
pub mod display;
pub mod display_data;
pub mod frame_art;
pub mod frame_layout;
pub mod gesture;
pub mod global_sync;
pub mod instance;
pub mod library;
pub mod loading;
pub mod playlist;
pub mod settings_session;
pub mod snap;
pub mod transition_ambient;

/// 当前 winit 窗口的观测；每帧重新查询，不保存窗口句柄。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeWindowState {
    pub maximized: bool,
    pub fullscreen: bool,
}

/// 这些是请求或拒绝记录，不是 Windows 能力通过的结论。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidationEvent {
    MoveRequested,
    ResizeRequested(egui::ResizeDirection),
    ResizeBlocked(egui::ResizeDirection),
    MoveBlocked,
    MaximizeRequested(bool),
    MaximizeBlocked,
    SnapshotRequested,
    CloseRequested,
}

/// At roughly 4 passes per second this bounds an unapplied maximize request to ~30 seconds.
const MAXIMIZATION_WAIT_PASSES: u8 = 120;

#[derive(Default)]
pub struct WindowValidation {
    resize_requests: u64,
    move_requests: u64,
    maximization_pending: Option<bool>,
    maximization_wait: u8,
    events: Vec<ValidationEvent>,
}

impl WindowValidation {
    pub fn request_counts(&self) -> (u64, u64) {
        (self.move_requests, self.resize_requests)
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = ValidationEvent> + '_ {
        self.events.drain(..)
    }

    /// 输入到真实视口命令的入口；测试与验证程序共用此处。
    pub fn show(&mut self, ui: &mut egui::Ui, native: Option<NativeWindowState>) {
        let ctx = ui.ctx();
        // An egui sizing pass may repeat the UI with the same press. Never repeat native actions.
        if ctx.current_pass_index() != 0 {
            return;
        }
        self.events.clear();
        if self.maximization_pending.is_some() {
            if native.is_some_and(|state| Some(state.maximized) == self.maximization_pending) {
                self.maximization_pending = None;
            } else if self.maximization_wait >= MAXIMIZATION_WAIT_PASSES {
                // A backend that never applies the request must not disable resizing forever.
                self.maximization_pending = None;
                self.events.push(ValidationEvent::MaximizeBlocked);
            } else {
                self.maximization_wait += 1;
            }
        }
        if key_pressed(ctx, egui::Key::Escape) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.events.push(ValidationEvent::CloseRequested);
            return;
        }
        if key_pressed(ctx, egui::Key::F10) {
            if let Some(state) = native.filter(|state| !state.fullscreen) {
                if self.maximization_pending.is_none() {
                    let target = !state.maximized;
                    self.maximization_pending = Some(target);
                    self.maximization_wait = 0;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(target));
                    self.events.push(ValidationEvent::MaximizeRequested(target));
                } else {
                    self.events.push(ValidationEvent::MaximizeBlocked);
                }
            } else {
                self.events.push(ValidationEvent::MaximizeBlocked);
            }
            // The backend applies this after the pass. Do not resize against the old state.
            return;
        }
        if key_pressed(ctx, egui::Key::F9) {
            self.events.push(ValidationEvent::SnapshotRequested);
        }
        let viewport_blocks_resize = ctx.input(|input| {
            input.viewport().maximized == Some(true) || input.viewport().fullscreen == Some(true)
        });
        let can_resize = native.is_some_and(|state| !state.maximized && !state.fullscreen)
            && !viewport_blocks_resize
            && self.maximization_pending.is_none();
        let rect = ctx.viewport_rect();
        if can_resize {
            if let Some(pos) = ctx.input(|input| input.pointer.hover_pos()) {
                if rect.contains(pos) {
                    if let Some(direction) = resize_direction(rect, pos) {
                        ctx.set_cursor_icon(resize_cursor(direction));
                    }
                }
            }
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
        if let Some(direction) = resize_direction(rect, pos) {
            if can_resize {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                self.resize_requests += 1;
                self.events
                    .push(ValidationEvent::ResizeRequested(direction));
            } else {
                // Maximized edges must not fall through to StartDrag (winit #4821).
                self.events.push(ValidationEvent::ResizeBlocked(direction));
            }
        } else if native.is_some_and(|state| !state.fullscreen)
            && !ctx.input(|input| input.viewport().fullscreen == Some(true))
            && self.maximization_pending.is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            self.move_requests += 1;
            self.events.push(ValidationEvent::MoveRequested);
        } else {
            self.events.push(ValidationEvent::MoveBlocked);
        }
    }
}

fn key_pressed(ctx: &egui::Context, key: egui::Key) -> bool {
    ctx.input(|input| input.events.iter().any(|event| {
        matches!(event, egui::Event::Key { key: pressed, pressed: true, repeat: false, .. } if *pressed == key)
    }))
}

pub fn resize_cursor(direction: egui::ResizeDirection) -> egui::CursorIcon {
    use egui::ResizeDirection::*;
    match direction {
        North | South => egui::CursorIcon::ResizeVertical,
        East | West => egui::CursorIcon::ResizeHorizontal,
        NorthEast | SouthWest => egui::CursorIcon::ResizeNeSw,
        NorthWest | SouthEast => egui::CursorIcon::ResizeNwSe,
    }
}

// Coordinates and the eight-point hit band are logical egui points, not physical pixels.
// Shared with the product frame (ticket 10), which reuses the validated hit geometry.
pub fn resize_direction(rect: egui::Rect, pos: egui::Pos2) -> Option<egui::ResizeDirection> {
    use egui::ResizeDirection::*;
    let left = pos.x - rect.left() < 8.0;
    let right = rect.right() - pos.x < 8.0;
    let top = pos.y - rect.top() < 8.0;
    let bottom = rect.bottom() - pos.y < 8.0;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(NorthWest),
        (_, true, true, _) => Some(NorthEast),
        (true, _, _, true) => Some(SouthWest),
        (_, true, _, true) => Some(SouthEast),
        (true, _, _, _) => Some(West),
        (_, true, _, _) => Some(East),
        (_, _, true, _) => Some(North),
        (_, _, _, true) => Some(South),
        _ => None,
    }
}

pub mod window_geom;
