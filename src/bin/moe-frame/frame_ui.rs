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
        self.drain_replies(&ctx);
        self.update_window_geometry(&ctx);
        let now = now_ms(&ctx);
        let tick_effects = self.core.handle(Intent::Tick { now_ms: now });
        self.dispatch(&ctx, tick_effects);
        // 托盘（工单 17）：首帧在事件循环内建托盘；之后每帧收托盘事件——
        // 左键单击按焦点三分支召回（tray-only 工单 01），菜单项走与相框菜单相同的核心意图，隐藏时同样有效。
        if !self.tray_tried {
            self.tray_tried = true;
            self.tray = build_tray(self.core.frame_visible(), self.core.is_playing());
        }
        let mut tray_intents: Vec<Intent> = Vec::new();
        let mut tray_settings = false;
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
                    tray_intents.push(Intent::ToggleVisibilityRequested {
                        focused: frame_focused(&ctx),
                    });
                } else if id == tray.play_pause.id() {
                    tray_intents.push(Intent::PlayPauseToggled { now_ms: now });
                } else if id == tray.prev.id() {
                    tray_intents.push(Intent::PrevRequested);
                } else if id == tray.next.id() {
                    tray_intents.push(Intent::NextRequested);
                } else if id == tray.settings.id() {
                    tray_settings = true;
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
        // 关闭只隐藏（工单 17）：拦截窗口关闭请求（含 Alt+F4）改走核心的
        // 关闭即隐藏；明确退出已置 exiting 时不拦截，关窗即结束进程。
        if !self.exiting && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            let effects = self.core.handle(Intent::CloseRequested);
            self.dispatch(&ctx, effects);
        }
        self.handle_keys(&ctx, now);
        let mut menu_intent: Option<Intent> = None;
        let mut menu_settings = false;
        // 窗口永远透明创建（V2 相框外观专题工单 01）：极简/内置模式
        // 靠这层铺满视口的不透明填充保持 V1 视觉；自定义相框图
        // （工单 03）不铺底——相框图的透明像素直接透出桌面，宿主
        // 只画画面区域的插画与九宫格八区，异形的完整行为在工单 04。
        self.ensure_frame_art_texture(&ctx);
        let custom_active = self.custom_art_active();
        let panel_frame = if custom_active {
            egui::Frame::NONE.fill(egui::Color32::TRANSPARENT)
        } else {
            egui::Frame::NONE.fill(egui::Color32::BLACK)
        };
        let panel = egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter();
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                // 画面区域 = 外框减去生效内距：插画只画在这里，
                // 画框（内置或相框图）占其余部分；内距全 0 时即整窗。
                let insets = self.effective_insets(rect.width());
                let image_rect = egui::Rect::from_min_max(
                    rect.min + egui::vec2(insets.left, insets.top),
                    rect.max - egui::vec2(insets.right, insets.bottom),
                );
                if let Some(texture) = &self.texture {
                    painter.image(texture.id(), image_rect, uv, egui::Color32::WHITE);
                } else {
                    painter.text(
                        image_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "右键打开设置，选择图库文件夹",
                        egui::FontId::proportional(15.0),
                        egui::Color32::from_rgb(200, 200, 200),
                    );
                }
                // 自定义相框图九宫格（工单 03）：八区盖在插画之上，
                // 洞区不绘制；相框图的透明像素不被覆画。
                if custom_active {
                    if let FrameArtHost::Loaded {
                        analysis,
                        texture: Some(art_texture),
                        ..
                    } = &self.frame_art
                    {
                        let layout = moe_frame::frame_art::nine_slice_layout(
                            analysis,
                            [rect.width(), rect.height()],
                        );
                        let art_size = [analysis.art_size[0] as f32, analysis.art_size[1] as f32];
                        for region in layout.regions {
                            let dst = egui::Rect::from_min_size(
                                rect.min + egui::vec2(region.dst[0], region.dst[1]),
                                egui::vec2(region.dst[2], region.dst[3]),
                            );
                            let src_uv = egui::Rect::from_min_size(
                                egui::pos2(
                                    region.src[0] / art_size[0],
                                    region.src[1] / art_size[1],
                                ),
                                egui::vec2(
                                    region.src[2] / art_size[0],
                                    region.src[3] / art_size[1],
                                ),
                            );
                            painter.image(art_texture.id(), dst, src_uv, egui::Color32::WHITE);
                        }
                    }
                }
            });
        let playing = self.core.is_playing();
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
                if ui.button("设置").clicked() {
                    menu_settings = true;
                    ui.close();
                }
                if ui.button("退出").clicked() {
                    menu_intent = Some(Intent::ExitRequested);
                    ui.close();
                }
            });
        self.handle_pointer(&ctx);
        if let Some(intent) = menu_intent {
            let effects = self.core.handle(intent);
            self.dispatch(&ctx, effects);
        }
        if menu_settings {
            self.request_settings(&ctx);
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
        if let Some(delay) = next_frame_delay(
            self.core.frame_visible(),
            self.pipeline.load_in_flight(),
            self.heal_repaint_until_ms,
            self.core.next_deadline_ms(),
            now,
        ) {
            ctx.request_repaint_after(delay);
        }
    }
}

/// 帧尾的下一帧唤醒裁决（纯函数，回归测试钉住），优先级从高到低：
/// 隐藏低频心跳 → 导航加载轮询 → 治愈保活 → 睡到轮播截止；都不
/// 命中则不唤醒（None），避免空转重绘。
pub(crate) fn next_frame_delay(
    frame_visible: bool,
    load_in_flight: bool,
    heal_repaint_until_ms: u64,
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
    } else if now_ms < heal_repaint_until_ms {
        // 治愈保活（.scratch/blur-heal 工单 01）：当前图口径已过期、刷新
        // 在等停稳计时走完；保活出帧让它不依赖聚焦等外部唤醒。
        Some(Duration::from_millis(50))
    } else {
        // 睡到下一次自动切换时刻再唤醒。
        next_deadline_ms.map(|deadline| Duration::from_millis(deadline.saturating_sub(now_ms)))
    }
}

#[cfg(test)]
mod tests {
    use super::next_frame_delay;
    use std::time::Duration;

    #[test]
    fn hidden_frame_keeps_the_slow_heartbeat_above_all() {
        assert_eq!(
            next_frame_delay(false, true, u64::MAX, Some(0), 1_000),
            Some(Duration::from_millis(250))
        );
    }

    #[test]
    fn load_in_flight_polls_fast() {
        assert_eq!(
            next_frame_delay(true, true, 0, Some(9_000), 1_000),
            Some(Duration::from_millis(50))
        );
    }

    #[test]
    fn heal_keepalive_beats_the_slide_deadline() {
        // 保活窗口内（now < 截止）：50ms 出帧，不睡到轮播截止。
        assert_eq!(
            next_frame_delay(true, false, 3_000, Some(60_000), 1_000),
            Some(Duration::from_millis(50))
        );
    }

    #[test]
    fn expired_heal_window_falls_back_to_the_slide_deadline() {
        assert_eq!(
            next_frame_delay(true, false, 3_000, Some(60_000), 3_000),
            Some(Duration::from_millis(57_000))
        );
        // 截止已过按 0 处理，不欠账。
        assert_eq!(
            next_frame_delay(true, false, 0, Some(500), 1_000),
            Some(Duration::from_millis(0))
        );
    }

    #[test]
    fn idle_without_deadline_requests_nothing() {
        assert_eq!(next_frame_delay(true, false, 0, None, 1_000), None);
    }
}
