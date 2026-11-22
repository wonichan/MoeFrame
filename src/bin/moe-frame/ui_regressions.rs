//! 宿主输入回归测试。

use eframe::egui::{
    self, CentralPanel, Color32, Context, Event, Frame, Modifiers, PointerButton, Pos2, RawInput,
    Rect, Vec2,
};

fn frame(ctx: &Context, events: Vec<Event>) -> bool {
    let mut opened = false;
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 200.0))),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        let panel = CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::BLACK))
            .show(ui, |ui| {
                ui.painter()
                    .rect_filled(ui.max_rect(), egui::CornerRadius::ZERO, Color32::BLACK);
            });
        panel
            .response
            .interact(egui::Sense::click())
            .context_menu(|ui| {
                ui.label("frame menu");
            });
        opened = panel.response.context_menu_opened();
    });
    output.textures_delta.clear();
    opened
}

#[test]
fn the_frame_context_menu_opens_on_secondary_click() {
    let ctx = Context::default();
    let pos = Pos2::new(100.0, 100.0);
    frame(&ctx, vec![Event::PointerMoved(pos)]);
    frame(
        &ctx,
        vec![Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(frame(
        &ctx,
        vec![Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    ));
}
#[test]
fn tray_left_click_toggles_visibility_and_right_click_is_reserved_for_menu() {
    use super::tray_click_intent;

    assert!(matches!(
        tray_click_intent(
            tray_icon::MouseButton::Left,
            tray_icon::MouseButtonState::Up,
            true
        ),
        Some(super::Intent::ToggleVisibilityRequested { focused: true })
    ));
    assert!(tray_click_intent(
        tray_icon::MouseButton::Left,
        tray_icon::MouseButtonState::Down,
        false
    )
    .is_none());
    assert!(tray_click_intent(
        tray_icon::MouseButton::Right,
        tray_icon::MouseButtonState::Up,
        false
    )
    .is_none());
}
