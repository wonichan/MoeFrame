//! 窗口几何：拖动改尺寸与恢复默认。

use super::*;

impl FrameApp {
    /// 返回本帧发出的内尺寸请求；调用者不能把请求视为已经落地。
    pub(crate) fn update_window_geometry(&mut self, ctx: &egui::Context) -> Option<[f32; 2]> {
        let pixels_per_point = ctx.pixels_per_point();
        // 原生移动手势结束（模态循环已退出、主键已抬起）的这一帧：
        // 清手势旗标，让本帧观察的 pointer_down 如实为假，几何机据
        // 此在松手帧做既有重校正。拖动中的窗口吸附已由 WM_MOVING
        // 子类化层在拖动过程中实时完成（工单 05），几何机不再有
        // 松手吸附路径；松手位置以实际观察为准、防抖落盘照旧。
        if self.native_gesture == Some(NativeGesture::Move) && !primary_button_down() {
            self.native_gesture = None;
            if self.solo_drag {
                // Shift 拖动（工单 03 手势开始采样、工单 06 清
                // 属性语义）：独走已由子类化层在拖动开始执行，
                // 这里只收尾旗标。
                self.solo_drag = false;
            }
        }
        // 锚边邻居刷新（工单 06）：只在已唤醒的帧上枚举一次伙伴
        // （含各伙伴与本窗的基座属性），按「属性关系 + 直接接
        // 触」派生锚边与菜单脱离的出现条件；不新增任何唤醒——
        // 静止零重绘口径回到无吸附专题轮询的基线（工单 03 的轮
        // 询跟随已整段拆除，整组联动由子类化层领队搬运）。
        if self.window_snap && self.core.frame_visible() {
            let own_rect = ctx.input(|input| input.viewport().outer_rect).map(|rect| {
                moe_frame::snap::WindowRect::from_pos_size(
                    [rect.min.x, rect.min.y],
                    [rect.width(), rect.height()],
                )
            });
            if let (Some(rect), Some((monitor, work_area))) = (
                own_rect,
                super::snap_enum::own_monitor_work_area(self.hwnd, pixels_per_point),
            ) {
                let own_frame = moe_frame::snap::PartnerFrame {
                    rect,
                    monitor,
                    work_area,
                };
                let partners = super::snap_enum::relation_partners(self.hwnd, pixels_per_point);
                let own_id = self.hwnd as u64;
                let mut relations: Vec<moe_frame::snap_group::SnapRelation> = partners
                    .iter()
                    .filter_map(|partner| {
                        partner
                            .base
                            .map(|base| moe_frame::snap_group::SnapRelation {
                                subordinate: partner.id,
                                base,
                            })
                    })
                    .collect();
                let own_base = super::snap_enum::read_base(own_id);
                if let Some(base) = own_base {
                    relations.push(moe_frame::snap_group::SnapRelation {
                        subordinate: own_id,
                        base,
                    });
                }
                let contacts = moe_frame::snap_group::direct_contact_ids(own_frame, &partners);
                let neighbours =
                    moe_frame::snap_group::anchor_neighbour_ids(own_id, &relations, &contacts);
                self.snap_anchor =
                    moe_frame::snap_group::anchor_edges(own_frame, &partners, &neighbours);
                // 菜单脱离出现条件（审查修复 F5 改定）：本窗有基
                // 座属性，或有直接从属指向本窗——存量接触无属性
                // 关系不再触发该项。
                self.snap_detach_available = moe_frame::snap_group::snap_detach_available(
                    own_base.is_some(),
                    partners.iter().any(|partner| partner.base == Some(own_id)),
                );
            }
        }
        let native_gesture = self.native_gesture.is_some();
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
                    snap_anchor: self.snap_anchor.clone(),
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
        if native_gesture && !primary_button_down() {
            self.native_gesture = None;
        }
        let mut requested_size = None;
        for command in &commands {
            if let GeomCommand::SetInnerSize(requested) = command {
                requested_size = Some(*requested);
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
        requested_size
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
