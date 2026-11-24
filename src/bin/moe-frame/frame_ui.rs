//! 每帧界面（eframe::App::ui）：滑动绘制、右键菜单、设置窗视口。

use super::*;

impl eframe::App for FrameApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.started {
            self.started = true;
            startup_marker("first-frame", "phase=egui-first-ui");
            let effects = self.core.handle(Intent::Started);
            self.dispatch(&ctx, effects);
        }
        // 几何会在松手帧接收最后一份用户尺寸；下一帧再执行到期的轮播。
        let resizing = self.native_gesture == Some(NativeGesture::Resize);
        self.core.set_autoplay_deferred(resizing);
        self.drain_replies(&ctx);
        self.update_geometry_and_autoplay(&ctx, resizing);
        let now = now_ms(&ctx);
        // 全局同步轮询（工单 04）：只在本帧（既有调度唤醒的帧）顺手
        // stat 一次配置修改时间，不新增唤醒——静止零重绘口径不变，
        // 多实例时由工单 03 的跟随心跳帧承载，延迟约一个心跳周期。
        self.poll_global_sync(&ctx, now);
        let tick_effects = self.core.handle(Intent::Tick { now_ms: now });
        self.dispatch(&ctx, tick_effects);
        // 转场推进（工单 02）：到期回送完成并在此刻转正纹理；
        // 在绘制之前跑，本帧画出的就是完成后的状态。
        self.advance_transition(&ctx);
        // 托盘（工单 17）：首帧在事件循环内建托盘；之后每帧收托盘事件——
        // 左键单击按焦点三分支召回（tray-only 工单 01），菜单项走与相框菜单相同的核心意图，隐藏时同样有效。
        if !self.tray_tried {
            self.tray_tried = true;
            self.tray = build_tray(
                self.core.frame_visible(),
                self.core.is_playing(),
                self.instance_number,
            );
        }
        let mut tray_intents: Vec<Intent> = Vec::new();
        let mut tray_settings = false;
        let mut tray_new_frame = false;
        let mut tray_exit_all = false;
        // 「退出全部相框」的接收侧（工单 04）：别的实例在给本窗置了
        // 退出属性并投递 WM_CLOSE 后，本帧读到属性即走自身正常退出
        // 路径（与托盘「退出」同一意图：几何冲刷、清在运行标记、
        // 摘托盘）。普通关闭没有属性，仍走关闭即隐藏，不受影响。
        if super::snap_enum::take_exit_all_request(self.hwnd) {
            tray_intents.push(Intent::ExitRequested);
        }
        while let Ok(event) = self.tray_event_rx.try_recv() {
            if let tray_icon::TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = event
            {
                if let Some(intent) = tray_click_intent(button, button_state, frame_focused(&ctx)) {
                    tray_intents.push(intent);
                }
            }
        }
        if let Some(tray) = &self.tray {
            while let Ok(event) = self.menu_event_rx.try_recv() {
                let id = event.id();
                if id == tray.show_hide.id() {
                    // 菜单项按标签语义给终态：菜单弹出时相框必失焦，走三分支会被
                    // 判成「只唤回」，「关闭相框」就点不动。
                    tray_intents.push(Intent::SetFrameVisibleRequested {
                        visible: !self.core.frame_visible(),
                    });
                } else if id == tray.play_pause.id() {
                    tray_intents.push(Intent::PlayPauseToggled { now_ms: now });
                } else if id == tray.prev.id() {
                    tray_intents.push(Intent::PrevRequested);
                } else if id == tray.next.id() {
                    tray_intents.push(Intent::NextRequested);
                } else if id == tray.settings.id() {
                    tray_settings = true;
                } else if id == tray.new_frame.id() {
                    tray_new_frame = true;
                } else if id == tray.exit_all.id() {
                    tray_exit_all = true;
                } else if id == tray.exit.id() {
                    tray_intents.push(Intent::ExitRequested);
                }
            }
        }
        for intent in tray_intents {
            let effects = self.core.handle(intent);
            self.dispatch(&ctx, effects);
        }
        if tray_settings {
            self.request_settings(&ctx);
        }
        if tray_new_frame {
            spawn_new_frame();
        }
        if tray_exit_all {
            // 退出全部（工单 04）：先给全部伙伴发退出请求，各实例走
            // 自身退出路径；本实例最后退出（同一意图收尾）。
            super::snap_enum::exit_all_partners(self.hwnd, ctx.pixels_per_point());
            let effects = self.core.handle(Intent::ExitRequested);
            self.dispatch(&ctx, effects);
        }
        // 关闭只隐藏（工单 17）：拦截窗口关闭请求（含 Alt+F4）改走核心的
        // 关闭即隐藏；明确退出已置 exiting 时不拦截，关窗即结束进程。
        if !self.exiting && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            let effects = self.core.handle(Intent::CloseRequested);
            self.dispatch(&ctx, effects);
        }
        self.handle_keys(&ctx, now);
        let mut menu_window_level = None;
        let requested_window_level = self.window_level;
        let mut menu_intent: Option<Intent> = None;
        let mut menu_settings = false;
        let mut menu_detach = false;
        // 窗口永远透明创建（V2 相框外观专题工单 01）：画哪一层、
        // 面板铺不铺底、空状态提示怎么落，由布局接缝的绘制计划
        // 一次裁决（frame_layout::paint_plan，工单 04）——极简靠
        // 面板黑底保持 V1 视觉；内置三层与阴影由 frame_paint 照
        // 计划绘制（工单 02）；自定义相框图生效时（工单 03）面板
        // 不铺底，相框图的透明像素（洞与异形外轮廓）直接透出
        // 桌面，宿主只画画面区域的插画与九宫格八区。
        self.ensure_frame_art_texture(&ctx);
        let effective = self.effective_appearance();
        let plan = moe_frame::frame_layout::paint_plan(effective, self.texture.is_some());
        let panel_fill = if plan.panel_opaque {
            egui::Color32::BLACK
        } else {
            egui::Color32::TRANSPARENT
        };
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(panel_fill))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter();
                // 内置画框（含自定义未生效时的回退）由 frame_paint
                // 整窗绘制：三层、阴影与空状态提示都在其内部；绘制
                // 参数取生效外观携带的那份（回退时是默认参数）。
                if let EffectiveAppearance::Builtin(frame) = effective {
                    self.paint_builtin_frame(painter, rect, frame);
                    return;
                }
                // 画面区域 = 外框减去生效内距：插画只画在这里，
                // 画框（内置或相框图）占其余部分；内距全 0 时即整窗。
                let insets = self.effective_insets(rect.width());
                let image_rect = egui::Rect::from_min_max(
                    rect.min + egui::vec2(insets.left, insets.top),
                    rect.max - egui::vec2(insets.right, insets.bottom),
                );
                if self.paint_stage(painter, image_rect) {
                    // 舞台已画（当前图或转场中的新旧两图）。
                } else if let moe_frame::frame_layout::HintPaint::Floating { outline } = plan.hint {
                    // 空状态提示浮在画面区域中央：自定义生效时这里
                    // 是相框图的洞，背景是透出的桌面——先衬一层深色
                    // 描影再落浅字，明暗桌面上都可读（工单 04）。
                    let hint_text = "右键打开设置，选择图库文件夹";
                    let hint_font = egui::FontId::proportional(15.0);
                    if outline {
                        painter.text(
                            image_rect.center() + egui::vec2(1.0, 1.0),
                            egui::Align2::CENTER_CENTER,
                            hint_text,
                            hint_font.clone(),
                            egui::Color32::from_black_alpha(160),
                        );
                    }
                    painter.text(
                        image_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        hint_text,
                        hint_font,
                        egui::Color32::from_rgb(200, 200, 200),
                    );
                }
                // 自定义相框图九宫格（工单 03）：八区盖在插画之上，
                // 洞区不绘制；相框图的透明像素不被覆画——异形外
                // 轮廓的透明处因此直接透出桌面（工单 04）。
                if plan.frame_paint == moe_frame::frame_layout::FramePaint::CustomArt {
                    if let FrameArtHost::Loaded {
                        analysis,
                        texture: Some(art_texture),
                        ..
                    } = &self.frame_art
                    {
                        let layout = nine_slice_layout(analysis, to_layout_rect(rect));
                        let art_size = [analysis.art_size[0] as f32, analysis.art_size[1] as f32];
                        for region in layout.regions {
                            let src_uv = egui::Rect::from_min_max(
                                egui::pos2(
                                    region.src.left / art_size[0],
                                    region.src.top / art_size[1],
                                ),
                                egui::pos2(
                                    region.src.right / art_size[0],
                                    region.src.bottom / art_size[1],
                                ),
                            );
                            painter.image(
                                art_texture.id(),
                                to_egui_rect(region.dst),
                                src_uv,
                                egui::Color32::WHITE,
                            );
                        }
                    }
                }
            });
        let playing = self.core.is_playing();
        // 相框右键菜单的最终形态（工单 04 定死，守菜单瘦身口径——
        // 本专题只加「脱离吸附组」一项）：上一张 / 下一张 / 播放·暂
        // 停 / 脱离吸附组（仅当前有直接接触时出现）/ 置于顶层 / 置
        // 于底层 / 设置 / 退出，共 7 项、无接触时 6 项（脱离项不
        // 出现）。「退出」只退本实例，与托盘「退出全部相框」分工。
        panel
            .response
            .interact(egui::Sense::click())
            .context_menu(|ui| {
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
                // 脱离吸附组（工单 06）：出现条件定死为「本窗有
                // 基座属性或有直接接触伙伴」（守菜单瘦身口径）；
                // 点击清本窗基座属性（经宿主薄层 RemoveProp），
                // 位置不变。
                if self.snap_detach_available && ui.button("脱离吸附组").clicked() {
                    menu_detach = true;
                    ui.close();
                }
                if ui
                    .selectable_label(
                        requested_window_level == egui::WindowLevel::AlwaysOnTop,
                        if requested_window_level == egui::WindowLevel::AlwaysOnTop {
                            "✓ 置于顶层"
                        } else {
                            "置于顶层"
                        },
                    )
                    .clicked()
                {
                    menu_window_level = Some(
                        if requested_window_level == egui::WindowLevel::AlwaysOnTop {
                            egui::WindowLevel::Normal
                        } else {
                            egui::WindowLevel::AlwaysOnTop
                        },
                    );
                    ui.close();
                }
                if ui
                    .selectable_label(
                        requested_window_level == egui::WindowLevel::AlwaysOnBottom,
                        if requested_window_level == egui::WindowLevel::AlwaysOnBottom {
                            "✓ 置于底层"
                        } else {
                            "置于底层"
                        },
                    )
                    .clicked()
                {
                    menu_window_level = Some(
                        if requested_window_level == egui::WindowLevel::AlwaysOnBottom {
                            egui::WindowLevel::Normal
                        } else {
                            egui::WindowLevel::AlwaysOnBottom
                        },
                    );
                    ui.close();
                }
                if ui.button("设置").clicked() {
                    menu_settings = true;
                    ui.close();
                }
                if ui.button("退出").clicked() {
                    menu_intent = Some(Intent::ExitRequested);
                    ui.close();
                }
            });
        if let Some(level) = menu_window_level {
            self.window_level = level;
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        }
        self.handle_pointer(&ctx);
        if let Some(intent) = menu_intent {
            let effects = self.core.handle(intent);
            self.dispatch(&ctx, effects);
        }
        if menu_settings {
            self.request_settings(&ctx);
        }
        if menu_detach {
            // 清本窗基座属性即脱离（工单 06）；锚边下一帧按新关
            // 系 + 直接接触重新派生，不在此清。
            super::snap_enum::clear_base(self.hwnd as u64);
            self.snap_detach_available = false;
        }
        if self.settings_open {
            self.show_settings(&ctx, now);
        }
        // 托盘菜单文案跟随核心的真实状态（工单 17），只在变化时更新。
        if let Some(tray) = &self.tray {
            let state = (self.core.frame_visible(), self.core.is_playing());
            if self.tray_state != Some(state) {
                self.tray_state = Some(state);
                tray.show_hide.set_text(if state.0 {
                    "关闭相框"
                } else {
                    "显示相框"
                });
                tray.play_pause
                    .set_text(if state.1 { "暂停" } else { "播放" });
            }
        }
        // 原生缩放期间 egui 不一定收到松手事件，短间隔低频确认手势结束。
        if resizing || self.native_gesture == Some(NativeGesture::Resize) {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if let Some(delay) = next_frame_delay(
            self.core.frame_visible(),
            self.pipeline.load_in_flight(),
            self.heal_repaint_until_ms,
            self.transition.as_ref().map(|t| t.deadline_ms()),
            self.geometry
                .settle_deadline_secs()
                .map(|secs| (secs.max(0.0) * 1000.0) as u64),
            self.core.next_deadline_ms(),
            now,
        ) {
            ctx.request_repaint_after(delay);
        }
    }
}

/// 帧尾的下一帧唤醒裁决（纯函数，回归测试钉住），优先级从高到低：
/// 隐藏低频心跳 → 导航加载轮询 → 转场动画帧（≤16ms，截止已过为
/// 0）→ 治愈保活 → 几何落盘结算与轮播截止取更早者；都不命中则
/// 不唤醒（None），避免空转重绘。工单 06 起本裁决不再含任何
/// 吸附轮询间隔——轮询跟随已整段拆除，静止零重绘回到基线。
// 参数即裁决的全部输入事实，逐项平铺便于回归测试逐项钉住。
#[allow(clippy::too_many_arguments)]
pub(crate) fn next_frame_delay(
    frame_visible: bool,
    load_in_flight: bool,
    heal_repaint_until_ms: u64,
    transition_deadline_ms: Option<u64>,
    geom_settle_ms: Option<u64>,
    next_deadline_ms: Option<u64>,
    now_ms: u64,
) -> Option<Duration> {
    if !frame_visible {
        // 隐藏期间保持低频心跳（工单 17）：托盘事件在 ui() 里轮询，暂停且
        // 隐藏时没有轮播截止时刻可唤醒，心跳保证托盘点击/菜单及时被处理；
        // 轮播 Tick 仍按各自截止时刻在心跳帧上判定，不受影响。
        Some(Duration::from_millis(250))
    } else if load_in_flight {
        Some(Duration::from_millis(50))
    } else if let Some(deadline) = transition_deadline_ms {
        // 转场动画（工单 02）：按墙钟连续出帧，单次等待不超过一
        // 帧（16ms）；截止已过回 0，下一帧立即回送完成。
        Some(Duration::from_millis(
            deadline.saturating_sub(now_ms).min(16),
        ))
    } else if now_ms < heal_repaint_until_ms {
        // 治愈保活（.scratch/blur-heal 工单 01）：当前图口径已过期、刷新
        // 在等停稳计时走完；保活出帧让它不依赖聚焦等外部唤醒。
        Some(Duration::from_millis(50))
    } else {
        let rotation =
            next_deadline_ms.map(|deadline| Duration::from_millis(deadline.saturating_sub(now_ms)));
        let settle =
            geom_settle_ms.map(|deadline| Duration::from_millis(deadline.saturating_sub(now_ms)));
        // 几何还没停稳 0.6 秒：结算时刻前必须再出一帧，否则要等下一次
        // 轮播唤醒才写盘，"移动后随手退出"就会丢位置。
        [rotation, settle].into_iter().flatten().min()
    }
}

#[cfg(test)]
mod tests {
    use super::next_frame_delay;
    use std::time::Duration;

    #[test]
    fn hidden_frame_keeps_the_slow_heartbeat_above_all() {
        assert_eq!(
            next_frame_delay(false, true, u64::MAX, None, None, Some(0), 1_000),
            Some(Duration::from_millis(250))
        );
    }

    #[test]
    fn load_in_flight_polls_fast() {
        assert_eq!(
            next_frame_delay(true, true, 0, None, None, Some(9_000), 1_000),
            Some(Duration::from_millis(50))
        );
    }

    #[test]
    fn transition_wakes_per_frame_and_immediately_once_due() {
        // 转场进行中：距截止还远也只睡一帧（16ms），保证连续绘制。
        assert_eq!(
            next_frame_delay(true, false, 0, Some(9_000), None, None, 1_000),
            Some(Duration::from_millis(16))
        );
        // 距截止 5ms：睡 5ms，到期帧不迟到。
        assert_eq!(
            next_frame_delay(true, false, 0, Some(1_005), None, None, 1_000),
            Some(Duration::from_millis(5))
        );
        // 截止已过：0ms，下一帧立即回送完成（不漏发、不拖延）。
        assert_eq!(
            next_frame_delay(true, false, 0, Some(900), None, None, 1_000),
            Some(Duration::from_millis(0))
        );
        // 转场优先于治愈保活与轮播截止，但低于隐藏心跳与加载轮询。
        assert_eq!(
            next_frame_delay(
                true,
                false,
                u64::MAX,
                Some(9_000),
                None,
                Some(60_000),
                1_000
            ),
            Some(Duration::from_millis(16))
        );
    }

    #[test]
    fn heal_keepalive_beats_the_slide_deadline() {
        // 保活窗口内（now < 截止）：50ms 出帧，不睡到轮播截止。
        assert_eq!(
            next_frame_delay(true, false, 3_000, None, None, Some(60_000), 1_000),
            Some(Duration::from_millis(50))
        );
    }

    #[test]
    fn expired_heal_window_falls_back_to_the_slide_deadline() {
        assert_eq!(
            next_frame_delay(true, false, 3_000, None, None, Some(60_000), 3_000),
            Some(Duration::from_millis(57_000))
        );
        // 截止已过按 0 处理，不欠账。
        assert_eq!(
            next_frame_delay(true, false, 0, None, None, Some(500), 1_000),
            Some(Duration::from_millis(0))
        );
    }

    #[test]
    fn idle_without_deadline_requests_nothing() {
        assert_eq!(
            next_frame_delay(true, false, 0, None, None, None, 1_000),
            None
        );
    }
    #[test]
    fn geometry_settle_wakes_earlier_than_the_slide_deadline() {
        // 轮播还剩 10 秒，但几何 0.4 秒后结算：先出帧把位置写盘。
        assert_eq!(
            next_frame_delay(true, false, 0, None, Some(1_400), Some(11_000), 1_000),
            Some(Duration::from_millis(400))
        );
        // 轮播比结算更早时按轮播走。
        assert_eq!(
            next_frame_delay(true, false, 0, None, Some(11_000), Some(1_500), 1_000),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn no_snap_poll_wake_remains_after_follow_removal() {
        // 工单 06：轮询跟随拆除后，唤醒裁决不再含吸附轮询间隔——
        // 无截止时不唤醒（静止零重绘基线）；有轮播截止时只按它走。
        assert_eq!(
            next_frame_delay(true, false, 0, None, None, None, 1_000),
            None
        );
        assert_eq!(
            next_frame_delay(true, false, 0, None, None, Some(60_000), 1_000),
            Some(Duration::from_millis(59_000))
        );
    }
}
