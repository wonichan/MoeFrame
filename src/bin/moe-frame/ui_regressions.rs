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

fn test_app(test_id: &str) -> (super::FrameApp, std::path::PathBuf) {
    use std::sync::mpsc::channel;

    struct NoAutostart;
    impl moe_frame::autostart::AutostartStore for NoAutostart {
        fn read_enabled(&mut self) -> std::io::Result<bool> {
            Ok(false)
        }
        fn set_enabled(&mut self, _enabled: bool, _command: &str) -> std::io::Result<()> {
            Ok(())
        }
    }

    assert!(
        !test_id.is_empty()
            && test_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "test ID must be a filename-safe ASCII slug"
    );
    let temp = std::env::temp_dir().join(format!(
        "moe-frame-ui-regression-{}-{test_id}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp).expect("test temp directory can be created");
    let config_path = temp.join("config.toml");
    let log_path = temp.join("test.log");
    let (sync, package) =
        moe_frame::config_sync::ConfigSync::startup(&config_path, &log_path, &mut NoAutostart);
    let (command_tx, _command_rx) = channel();
    let (_reply_tx, reply_rx) = channel();
    let (tray_event_tx, tray_event_rx) = channel();
    drop(tray_event_tx);
    let (menu_event_tx, menu_event_rx) = channel();
    drop(menu_event_tx);
    let host = package.host;
    let global_sync = moe_frame::global_sync::GlobalSyncState::new(sync.global_settings(), None);
    let app = super::FrameApp {
        core: moe_frame::app_core::FrameCore::new(
            package.core,
            moe_frame::gesture::GestureThresholds {
                drag_threshold: [6.0, 6.0],
                double_click_dist: [6.0, 6.0],
                double_click_ms: 500,
            },
        ),
        sync,
        command_tx,
        reply_rx,
        tray_event_rx,
        menu_event_rx,
        started: true,
        first_image_marked: true,
        texture: None,
        incoming: None,
        transition: None,
        hwnd: 0,
        press_window_pos: None,
        native_gesture: None,
        held_autoplay: None,
        tray: None,
        tray_tried: true,
        tray_state: None,
        exiting: false,
        settings_open: false,
        settings_positioned: false,
        pipeline: moe_frame::display_data::DisplayPipeline::new(),
        heal_repaint_until_ms: 0,
        stale_target_seen: None,
        geometry: moe_frame::window_geom::GeometryMachine::new(
            host.saved_geom,
            None,
            host.saved_position.is_some(),
        ),
        autostart: package.autostart,
        autostart_store: super::RegAutostartStore,
        appearance: host.appearance,
        window_snap: host.window_snap,
        follow: moe_frame::snap::FollowEngine::new(),
        poll_schedule: moe_frame::snap::PollSchedule::new(),
        snap_contacts: Vec::new(),
        snap_anchor: Vec::new(),
        solo_drag: false,
        instance_number: host.instance_number,
        global_sync,
        frame: host.frame,
        frame_art_path: host.frame_art.clone(),
        frame_art: super::FrameArtHost::NotSelected,
        window_level: egui::WindowLevel::Normal,
        session: moe_frame::settings_session::SettingsSession::new(),
        debug_last_inner: None,
    };
    (app, temp)
}

fn run_app_frame(app: &mut super::FrameApp, ctx: &Context, events: Vec<Event>) -> egui::FullOutput {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0))),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        eframe::App::ui(app, ui, &mut eframe::Frame::_new_kittest());
    });
    output.textures_delta.clear();
    output
}

fn pointer_button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn painted_text_center(output: &egui::FullOutput, text: &str) -> Option<Pos2> {
    output
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::epaint::Shape::Text(shape) if shape.galley.text() == text => {
                Some(shape.pos + shape.galley.size() * 0.5)
            }
            _ => None,
        })
}

fn open_context_menu(app: &mut super::FrameApp, ctx: &Context) -> egui::FullOutput {
    let pos = Pos2::new(200.0, 150.0);
    run_app_frame(app, ctx, vec![Event::PointerMoved(pos)]);
    run_app_frame(
        app,
        ctx,
        vec![pointer_button(pos, PointerButton::Secondary, true)],
    );
    run_app_frame(
        app,
        ctx,
        vec![pointer_button(pos, PointerButton::Secondary, false)],
    );
    run_app_frame(app, ctx, Vec::new())
}

fn choose_menu_item(app: &mut super::FrameApp, ctx: &Context, label: &str) -> egui::FullOutput {
    let item = painted_text_center(&run_app_frame(app, ctx, Vec::new()), label)
        .expect("requested context-menu item is visible");
    run_app_frame(
        app,
        ctx,
        vec![
            Event::PointerMoved(item),
            pointer_button(item, PointerButton::Primary, true),
        ],
    );
    run_app_frame(
        app,
        ctx,
        vec![pointer_button(item, PointerButton::Primary, false)],
    )
}

fn has_window_level_command(output: &egui::FullOutput, level: egui::WindowLevel) -> bool {
    output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::WindowLevel(level))
}

fn has_geometry_command(output: &egui::FullOutput) -> bool {
    output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .iter()
        .any(|command| {
            matches!(
                command,
                egui::ViewportCommand::InnerSize(_) | egui::ViewportCommand::OuterPosition(_)
            )
        })
}

#[test]
fn real_frame_menu_controls_session_window_level_without_changing_playback_or_visibility() {
    let (mut app, temp) = test_app("window-layer-menu");
    let ctx = Context::default();
    let current = temp.join("images/current.png");
    let other = temp.join("images/other.png");

    app.core.handle(moe_frame::app_core::Intent::FolderPicked(
        temp.join("images"),
    ));
    app.core
        .handle(moe_frame::app_core::Intent::ScanCompleted(vec![
            current.clone(),
            other,
        ]));
    app.core.handle(moe_frame::app_core::Intent::LoadCompleted {
        shown: Some(current.clone()),
        now_ms: 100,
    });
    let seq = app
        .pipeline
        .begin_load(moe_frame::loading::DisplayMode::Fit, [400, 300]);
    assert_eq!(
        app.pipeline.nav_reply(seq, Some(&current)),
        moe_frame::display_data::NavDecision::Accept
    );
    app.pipeline.promote_incoming();

    let before_view = app.core.view();
    let before_deadline = app.core.next_deadline_ms();
    let before_playing = app.core.is_playing();
    let before_visible = app.core.frame_visible();
    assert_eq!(
        before_view,
        moe_frame::app_core::View::Showing(current.clone())
    );
    assert!(before_deadline.is_some(), "seeded playback has a deadline");
    assert_eq!(
        app.pipeline.current_key().map(|key| &key.path),
        Some(&current)
    );

    let opened = open_context_menu(&mut app, &ctx);
    assert!(painted_text_center(&opened, "置于顶层").is_some());
    assert!(painted_text_center(&opened, "置于底层").is_some());
    assert!(painted_text_center(&opened, "普通层级").is_none());
    let top = choose_menu_item(&mut app, &ctx, "置于顶层");
    assert!(has_window_level_command(
        &top,
        egui::WindowLevel::AlwaysOnTop
    ));
    assert!(!has_geometry_command(&top));

    let opened = open_context_menu(&mut app, &ctx);
    assert!(painted_text_center(&opened, "✓ 置于顶层").is_some());
    assert!(painted_text_center(&opened, "置于顶层").is_none());
    assert!(painted_text_center(&opened, "置于底层").is_some());
    assert!(painted_text_center(&opened, "✓ 置于底层").is_none());
    let bottom = choose_menu_item(&mut app, &ctx, "置于底层");
    assert!(has_window_level_command(
        &bottom,
        egui::WindowLevel::AlwaysOnBottom
    ));
    assert!(!has_geometry_command(&bottom));

    let opened = open_context_menu(&mut app, &ctx);
    assert!(painted_text_center(&opened, "置于顶层").is_some());
    assert!(painted_text_center(&opened, "✓ 置于顶层").is_none());
    assert!(painted_text_center(&opened, "置于底层").is_none());
    assert!(painted_text_center(&opened, "✓ 置于底层").is_some());
    let normal = choose_menu_item(&mut app, &ctx, "✓ 置于底层");
    assert!(has_window_level_command(&normal, egui::WindowLevel::Normal));
    assert!(!has_geometry_command(&normal));

    let opened = open_context_menu(&mut app, &ctx);
    assert!(painted_text_center(&opened, "置于顶层").is_some());
    assert!(painted_text_center(&opened, "置于底层").is_some());
    assert!(painted_text_center(&opened, "✓ 置于顶层").is_none());
    assert!(painted_text_center(&opened, "✓ 置于底层").is_none());
    let top = choose_menu_item(&mut app, &ctx, "置于顶层");
    assert!(has_window_level_command(
        &top,
        egui::WindowLevel::AlwaysOnTop
    ));
    assert!(!has_geometry_command(&top));

    let opened = open_context_menu(&mut app, &ctx);
    assert!(painted_text_center(&opened, "✓ 置于顶层").is_some());
    assert!(painted_text_center(&opened, "置于顶层").is_none());
    let normal = choose_menu_item(&mut app, &ctx, "✓ 置于顶层");
    assert!(has_window_level_command(&normal, egui::WindowLevel::Normal));
    assert!(!has_geometry_command(&normal));

    open_context_menu(&mut app, &ctx);
    let bottom = choose_menu_item(&mut app, &ctx, "置于底层");
    assert!(has_window_level_command(
        &bottom,
        egui::WindowLevel::AlwaysOnBottom
    ));
    assert!(!has_geometry_command(&bottom));
    let opened = open_context_menu(&mut app, &ctx);
    assert!(painted_text_center(&opened, "✓ 置于底层").is_some());
    assert!(painted_text_center(&opened, "置于底层").is_none());

    let top = choose_menu_item(&mut app, &ctx, "置于顶层");
    assert!(has_window_level_command(
        &top,
        egui::WindowLevel::AlwaysOnTop
    ));
    assert!(!has_geometry_command(&top));
    let opened = open_context_menu(&mut app, &ctx);
    assert!(painted_text_center(&opened, "✓ 置于顶层").is_some());
    assert!(painted_text_center(&opened, "置于顶层").is_none());
    let normal = choose_menu_item(&mut app, &ctx, "✓ 置于顶层");
    assert!(has_window_level_command(&normal, egui::WindowLevel::Normal));
    assert!(!has_geometry_command(&normal));

    assert_eq!(app.core.view(), before_view);
    assert_eq!(app.core.next_deadline_ms(), before_deadline);
    assert_eq!(
        app.pipeline.current_key().map(|key| &key.path),
        Some(&current)
    );
    assert_eq!(app.core.is_playing(), before_playing);
    assert_eq!(app.core.frame_visible(), before_visible);
    let _ = std::fs::remove_dir_all(temp);
}
