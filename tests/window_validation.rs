use eframe::egui::{self, Event, PointerButton, ViewportCommand, ViewportId};
use moe_frame::WindowValidation;

fn frame(
    app: &mut WindowValidation,
    ctx: &egui::Context,
    events: Vec<Event>,
    maximized: Option<bool>,
) -> egui::FullOutput {
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(480.0, 320.0),
        )),
        events,
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&ViewportId::ROOT)
        .unwrap()
        .maximized = maximized;
    run_input(app, ctx, input, maximized)
}

fn run_input(
    app: &mut WindowValidation,
    ctx: &egui::Context,
    input: egui::RawInput,
    native_maximized: Option<bool>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(input, |ui| {
        app.show(
            ui,
            native_maximized.map(|maximized| moe_frame::NativeWindowState {
                maximized,
                fullscreen: false,
            }),
        )
    });
    // Headless tests consume commands, not textures. Explicitly discard the font delta.
    output.textures_delta.clear();
    output
        .viewport_output
        .get_mut(&ViewportId::ROOT)
        .unwrap()
        .commands
        .retain(|command| !matches!(command, ViewportCommand::SetTheme(_)));
    output
}

fn press(x: f32, y: f32) -> Event {
    Event::PointerButton {
        pos: egui::pos2(x, y),
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    }
}

#[test]
fn pressing_the_left_edge_requests_resize_instead_of_moving() {
    let output = frame(
        &mut WindowValidation::default(),
        &egui::Context::default(),
        vec![press(2.0, 160.0)],
        Some(false),
    );
    assert_eq!(
        output.viewport_output[&ViewportId::ROOT].commands,
        vec![ViewportCommand::BeginResize(egui::ResizeDirection::West)],
    );
}

#[test]
fn all_four_edges_and_four_corners_take_priority_over_content_drag() {
    use egui::ResizeDirection::*;
    let cases = [
        (2.0, 160.0, West),
        (478.0, 160.0, East),
        (240.0, 2.0, North),
        (240.0, 318.0, South),
        (2.0, 2.0, NorthWest),
        (478.0, 2.0, NorthEast),
        (2.0, 318.0, SouthWest),
        (478.0, 318.0, SouthEast),
    ];
    for (x, y, direction) in cases {
        let output = frame(
            &mut WindowValidation::default(),
            &egui::Context::default(),
            vec![press(x, y)],
            Some(false),
        );
        assert_eq!(
            output.viewport_output[&ViewportId::ROOT].commands,
            vec![ViewportCommand::BeginResize(direction)],
            "pressed {direction:?}",
        );
    }
}

#[test]
fn content_press_starts_moving_without_waiting_for_pointer_motion() {
    let output = frame(
        &mut WindowValidation::default(),
        &egui::Context::default(),
        vec![press(240.0, 160.0)],
        Some(false),
    );
    assert_eq!(
        output.viewport_output[&ViewportId::ROOT].commands,
        vec![ViewportCommand::StartDrag],
    );
}

fn release(x: f32, y: f32) -> Event {
    Event::PointerButton {
        pos: egui::pos2(x, y),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }
}

fn key(key: egui::Key, repeat: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat,
        modifiers: Default::default(),
    }
}

#[test]
fn fullscreen_requests_no_resize_or_move() {
    for shortcut in [None, Some(egui::Key::F10)] {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(480.0, 320.0),
            )),
            events: shortcut
                .map(|pressed| vec![key(pressed, false)])
                .unwrap_or_default(),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&ViewportId::ROOT)
            .unwrap()
            .fullscreen = Some(true);
        let output = run_input(
            &mut WindowValidation::default(),
            &egui::Context::default(),
            input,
            Some(false),
        );
        assert!(
            output.viewport_output[&ViewportId::ROOT]
                .commands
                .iter()
                .all(|command| !matches!(
                    command,
                    ViewportCommand::BeginResize(_) | ViewportCommand::StartDrag
                )),
            "fullscreen shortcut {shortcut:?}"
        );
    }
}
#[test]
fn maximized_or_unknown_edges_do_not_fall_back_to_moving() {
    for maximized in [Some(true), None] {
        for (x, y) in [
            (2.0, 160.0),
            (478.0, 160.0),
            (240.0, 2.0),
            (240.0, 318.0),
            (2.0, 2.0),
            (478.0, 2.0),
            (2.0, 318.0),
            (478.0, 318.0),
        ] {
            let output = frame(
                &mut WindowValidation::default(),
                &egui::Context::default(),
                vec![press(x, y)],
                maximized,
            );
            assert!(output.viewport_output[&ViewportId::ROOT]
                .commands
                .is_empty());
        }
    }
}

#[test]
fn stale_egui_maximized_state_also_blocks_resize() {
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(480.0, 320.0),
        )),
        events: vec![press(2.0, 160.0)],
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&ViewportId::ROOT)
        .unwrap()
        .maximized = Some(true);
    let output = run_input(
        &mut WindowValidation::default(),
        &egui::Context::default(),
        input,
        Some(false),
    );
    assert!(output.viewport_output[&ViewportId::ROOT]
        .commands
        .is_empty());
}

#[test]
fn restoring_after_a_blocked_edge_allows_new_resize_and_content_move() {
    let mut app = WindowValidation::default();
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![press(2.0, 160.0)], Some(true));
    frame(&mut app, &ctx, vec![release(2.0, 160.0)], Some(false));
    let resized = frame(&mut app, &ctx, vec![press(2.0, 160.0)], Some(false));
    assert_eq!(
        resized.viewport_output[&ViewportId::ROOT].commands,
        vec![ViewportCommand::BeginResize(egui::ResizeDirection::West)]
    );
    frame(&mut app, &ctx, vec![release(2.0, 160.0)], Some(false));
    let moved = frame(&mut app, &ctx, vec![press(240.0, 160.0)], Some(false));
    assert_eq!(
        moved.viewport_output[&ViewportId::ROOT].commands,
        vec![ViewportCommand::StartDrag]
    );
}

#[test]
fn moving_a_held_pointer_and_releasing_do_not_repeat_the_request() {
    let mut app = WindowValidation::default();
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![press(2.0, 160.0)], Some(false));
    for event in [
        Event::PointerMoved(egui::pos2(20.0, 190.0)),
        release(20.0, 190.0),
    ] {
        let output = frame(&mut app, &ctx, vec![event], Some(false));
        assert!(output.viewport_output[&ViewportId::ROOT]
            .commands
            .is_empty());
    }
}

#[test]
fn an_already_released_or_non_primary_press_does_not_begin_a_modal_operation() {
    let right = Event::PointerButton {
        pos: egui::pos2(2.0, 160.0),
        button: PointerButton::Secondary,
        pressed: true,
        modifiers: Default::default(),
    };
    for events in [vec![right], vec![press(2.0, 160.0), release(2.0, 160.0)]] {
        let output = frame(
            &mut WindowValidation::default(),
            &egui::Context::default(),
            events,
            Some(false),
        );
        assert!(output.viewport_output[&ViewportId::ROOT]
            .commands
            .is_empty());
    }
}

#[test]
fn f10_toggles_maximize_and_restore_but_never_repeats_or_resizes_in_the_same_frame() {
    for (maximized, target) in [(false, true), (true, false)] {
        let output = frame(
            &mut WindowValidation::default(),
            &egui::Context::default(),
            vec![key(egui::Key::F10, false), press(2.0, 160.0)],
            Some(maximized),
        );
        assert_eq!(
            output.viewport_output[&ViewportId::ROOT].commands,
            vec![ViewportCommand::Maximized(target)]
        );
    }
    let mut app = WindowValidation::default();
    let ctx = egui::Context::default();
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::F10, false)],
        Some(false),
    );
    let output = frame(&mut app, &ctx, vec![key(egui::Key::F10, true)], Some(true));
    assert!(output.viewport_output[&ViewportId::ROOT]
        .commands
        .is_empty());
}

#[test]
fn escape_closes_the_validation_window() {
    let output = frame(
        &mut WindowValidation::default(),
        &egui::Context::default(),
        vec![key(egui::Key::Escape, false)],
        Some(false),
    );
    assert_eq!(
        output.viewport_output[&ViewportId::ROOT].commands,
        vec![ViewportCommand::Close]
    );
}

#[test]
fn a_new_press_after_a_release_in_the_same_frame_uses_the_latest_press() {
    let output = frame(
        &mut WindowValidation::default(),
        &egui::Context::default(),
        vec![press(2.0, 160.0), release(2.0, 160.0), press(240.0, 160.0)],
        Some(false),
    );
    assert_eq!(
        output.viewport_output[&ViewportId::ROOT].commands,
        vec![ViewportCommand::StartDrag]
    );
}

#[test]
fn an_unapplied_maximize_request_does_not_block_resizing_forever() {
    let mut app = WindowValidation::default();
    let ctx = egui::Context::default();
    frame(
        &mut app,
        &ctx,
        vec![key(egui::Key::F10, false)],
        Some(false),
    );
    for _ in 0..121 {
        frame(&mut app, &ctx, vec![], Some(false));
    }
    let output = frame(&mut app, &ctx, vec![press(2.0, 160.0)], Some(false));
    assert_eq!(
        output.viewport_output[&ViewportId::ROOT].commands,
        vec![ViewportCommand::BeginResize(egui::ResizeDirection::West)]
    );
}
