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
        let mut menu_window_level = None;
        let requested_window_level = self.window_level;
        let mut menu_intent: Option<Intent> = None;
        let mut menu_settings = false;
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ui, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter();
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                if let Some(texture) = &self.texture {
                    painter.image(texture.id(), rect, uv, egui::Color32::WHITE);
                } else {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "右键打开设置，选择图库文件夹",
                        egui::FontId::proportional(15.0),
                        egui::Color32::from_rgb(200, 200, 200),
                    );
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
/// 隐藏低频心跳 → 导航加载轮询 → 治愈保活 → 几何落盘结算与轮播截止
/// 取更早者；都不命中则不唤醒（None），避免空转重绘。
pub(crate) fn next_frame_delay(
    frame_visible: bool,
    load_in_flight: bool,
    heal_repaint_until_ms: u64,
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
        match (rotation, settle) {
            (Some(rotation), Some(settle)) => Some(rotation.min(settle)),
            (rotation, settle) => rotation.or(settle),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::next_frame_delay;
    use std::time::Duration;

    #[test]
    fn hidden_frame_keeps_the_slow_heartbeat_above_all() {
        assert_eq!(
            next_frame_delay(false, true, u64::MAX, None, Some(0), 1_000),
            Some(Duration::from_millis(250))
        );
    }

    #[test]
    fn load_in_flight_polls_fast() {
        assert_eq!(
            next_frame_delay(true, true, 0, None, Some(9_000), 1_000),
            Some(Duration::from_millis(50))
        );
    }

    #[test]
    fn heal_keepalive_beats_the_slide_deadline() {
        // 保活窗口内（now < 截止）：50ms 出帧，不睡到轮播截止。
        assert_eq!(
            next_frame_delay(true, false, 3_000, None, Some(60_000), 1_000),
            Some(Duration::from_millis(50))
        );
    }

    #[test]
    fn expired_heal_window_falls_back_to_the_slide_deadline() {
        assert_eq!(
            next_frame_delay(true, false, 3_000, None, Some(60_000), 3_000),
            Some(Duration::from_millis(57_000))
        );
        // 截止已过按 0 处理，不欠账。
        assert_eq!(
            next_frame_delay(true, false, 0, None, Some(500), 1_000),
            Some(Duration::from_millis(0))
        );
    }

    #[test]
    fn idle_without_deadline_requests_nothing() {
        assert_eq!(next_frame_delay(true, false, 0, None, None, 1_000), None);
    }
    #[test]
    fn geometry_settle_wakes_earlier_than_the_slide_deadline() {
        // 轮播还剩 10 秒，但几何 0.4 秒后结算：先出帧把位置写盘。
        assert_eq!(
            next_frame_delay(true, false, 0, Some(1_400), Some(11_000), 1_000),
            Some(Duration::from_millis(400))
        );
        // 轮播比结算更早时按轮播走。
        assert_eq!(
            next_frame_delay(true, false, 0, Some(11_000), Some(1_500), 1_000),
            Some(Duration::from_millis(500))
        );
    }
}
