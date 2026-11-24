//! 设置窗会话机（architecture-deepening 工单 02）：工单 19 的全部时序
//! 规则（种子、帧尾结算、重扫否决、关闭丢弃、快照作废、即时控件顺序）
//! 在会话 interface 上验证——测试按帧喂事件与点选，断言每帧的动作。
//! 原 tests/settings_draft.rs 的结算用例已改走本 interface 表达。

use moe_frame::{
    config::{Animation, AppearanceMode, BuiltinFrame, FramePreset, PlaybackMode},
    settings_session::{
        EffectiveValues, Field, OpenDecision, Picks, SessionAction, SettingsSession,
    },
};

fn effective() -> EffectiveValues {
    EffectiveValues {
        folder_text: r"D:\图库".to_owned(),
        interval_secs: 30,
        mode: PlaybackMode::Random,
        animation: Animation::Curl,
        appearance: AppearanceMode::Builtin,
        frame: BuiltinFrame::default(),
        window_snap: true,
    }
}

fn opened() -> SettingsSession {
    let mut session = SettingsSession::new();
    assert_eq!(session.open_requested(&effective()), OpenDecision::Opened);
    session
}

fn no_picks() -> Picks {
    Picks::default()
}

/// 跑一帧：帧首快照作废检查 → 帧尾结算/点选应用，返回本帧动作。
fn frame(session: &mut SettingsSession, picks: Picks) -> Vec<SessionAction> {
    session.begin_frame(&effective());
    session.end_frame(picks, true)
}

#[test]
fn open_seeds_drafts_and_second_request_only_focuses() {
    let mut session = opened();
    assert!(session.is_open());
    assert_eq!(session.path_text(), r"D:\图库");
    assert_eq!(session.seconds_text(), "30");
    assert_eq!(
        session.open_requested(&effective()),
        OpenDecision::FocusOnly
    );
}

#[test]
fn typing_alone_never_commits() {
    let mut session = opened();
    session.path_text_mut().push_str(r"\新图");
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    assert_eq!(session.path_text(), r"D:\图库\新图");
}

#[test]
fn blur_settles_at_the_end_of_the_next_frame() {
    let mut session = opened();
    *session.path_text_mut() = "  E:\\照片  ".to_owned();
    session.note_blur(Field::Path);
    // 失焦当帧不结算：延迟一帧，给同帧的否决事件留位置。
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    assert_eq!(
        frame(&mut session, no_picks()),
        vec![SessionAction::CommitFolder("E:\\照片".to_owned())]
    );
    assert_eq!(session.path_text(), "E:\\照片");
}

#[test]
fn enter_commits_in_the_same_frame() {
    let mut session = opened();
    *session.seconds_text_mut() = "45".to_owned();
    session.note_enter(Field::Seconds);
    assert_eq!(
        frame(&mut session, no_picks()),
        vec![SessionAction::CommitInterval(45)]
    );
    assert_eq!(session.seconds_text(), "45");
}

#[test]
fn rescan_vetoes_a_pending_commit_from_last_frame() {
    let mut session = opened();
    *session.path_text_mut() = "E:\\照片".to_owned();
    session.note_blur(Field::Path);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    let mut picks = no_picks();
    picks.rescan = true;
    assert_eq!(frame(&mut session, picks), vec![SessionAction::Rescan]);
    // 草稿文字保留、未提交：后续帧也不会补交。
    assert_eq!(session.path_text(), "E:\\照片");
    assert_eq!(frame(&mut session, no_picks()), vec![]);
}

#[test]
fn rescan_in_the_same_frame_as_blur_drops_the_blur() {
    let mut session = opened();
    *session.path_text_mut() = "E:\\照片".to_owned();
    session.note_blur(Field::Path);
    let mut picks = no_picks();
    picks.rescan = true;
    assert_eq!(frame(&mut session, picks), vec![SessionAction::Rescan]);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
}

#[test]
fn folder_pick_vetoes_a_pending_commit_and_keeps_the_draft_text() {
    let mut session = opened();
    *session.path_text_mut() = "E:\\照片".to_owned();
    session.note_blur(Field::Path);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    let mut picks = no_picks();
    picks.folder_pick = true;
    assert_eq!(frame(&mut session, picks), vec![SessionAction::PickFolder]);
    // 与重扫同规：文字保留、未提交，后续帧不补交。
    assert_eq!(session.path_text(), "E:\\照片");
    assert_eq!(frame(&mut session, no_picks()), vec![]);
}

#[test]
fn folder_pick_in_the_same_frame_as_blur_drops_the_blur() {
    let mut session = opened();
    *session.path_text_mut() = "E:\\照片".to_owned();
    session.note_blur(Field::Path);
    let mut picks = no_picks();
    picks.folder_pick = true;
    assert_eq!(frame(&mut session, picks), vec![SessionAction::PickFolder]);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
}

#[test]
fn close_discards_pending_and_reopen_reseeds() {
    let mut session = opened();
    *session.path_text_mut() = "E:\\照片".to_owned();
    session.note_blur(Field::Path);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    let mut picks = no_picks();
    picks.close = true;
    assert_eq!(frame(&mut session, picks), vec![SessionAction::CloseWindow]);
    assert!(!session.is_open());
    // 重开从生效值播种，上次丢弃的文字不残留。
    assert_eq!(session.open_requested(&effective()), OpenDecision::Opened);
    assert_eq!(session.path_text(), r"D:\图库");
}

#[test]
fn external_change_discards_draft_and_pending() {
    let mut session = opened();
    *session.path_text_mut() = "E:\\照片".to_owned();
    session.note_blur(Field::Path);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    // 菜单选了新文件夹：下一帧帧首快照比对作废草稿与待结算。
    let changed = EffectiveValues {
        folder_text: r"F:\新库".to_owned(),
        ..effective()
    };
    session.begin_frame(&changed);
    assert_eq!(session.path_text(), r"F:\新库");
    assert_eq!(session.end_frame(no_picks(), true), vec![]);
}

#[test]
fn seconds_parse_failure_keeps_effective_and_restores_text() {
    let mut session = opened();
    *session.seconds_text_mut() = "abc".to_owned();
    session.note_blur(Field::Seconds);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    // 解析失败：不提交（绝不回退默认 30 一说，这里生效值本就是 30，
    // 断言的是文字恢复生效值且无动作）。
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    assert_eq!(session.seconds_text(), "30");
}

#[test]
fn seconds_out_of_range_clamps_and_displays_clamped() {
    let mut session = opened();
    *session.seconds_text_mut() = "2".to_owned();
    session.note_enter(Field::Seconds);
    assert_eq!(
        frame(&mut session, no_picks()),
        vec![SessionAction::CommitInterval(3)]
    );
    assert_eq!(session.seconds_text(), "3");
}

#[test]
fn seconds_huge_digit_string_clamps_to_max() {
    let mut session = opened();
    *session.seconds_text_mut() = "99999999999999999999".to_owned();
    session.note_enter(Field::Seconds);
    assert_eq!(
        frame(&mut session, no_picks()),
        vec![SessionAction::CommitInterval(99_999_999)]
    );
}

#[test]
fn preset_pick_discards_pending_seconds_commit() {
    let mut session = opened();
    *session.seconds_text_mut() = "45".to_owned();
    session.note_blur(Field::Seconds);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    // 预设在本帧生效：待结算的秒数草稿按「基线≠生效」在同一帧尾的
    // 结算里作废（不提交 45），框里文字当场改显新的生效值 60。
    let mut picks = no_picks();
    picks.interval = Some(60);
    session.begin_frame(&effective());
    let actions = session.end_frame(picks, true);
    assert_eq!(actions, vec![SessionAction::ApplyIntervalPreset(60)]);
    assert_eq!(session.seconds_text(), "60");
    assert_eq!(frame(&mut session, no_picks()), vec![]);
}

#[test]
fn unchanged_path_blur_is_a_noop() {
    let mut session = opened();
    session.note_blur(Field::Path);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    assert_eq!(session.path_text(), r"D:\图库");
}

#[test]
fn picks_apply_in_order_and_same_mode_pick_is_dropped() {
    let mut session = opened();
    let mut picks = no_picks();
    picks.mode = Some(PlaybackMode::Random);
    picks.interval = Some(60);
    picks.restore_default = true;
    picks.autostart = Some(true);
    assert_eq!(
        frame(&mut session, picks),
        vec![
            SessionAction::ApplyIntervalPreset(60),
            SessionAction::RestoreDefaultGeometry,
            SessionAction::ApplyAutostart(true),
        ]
    );
    let mut picks = no_picks();
    picks.mode = Some(PlaybackMode::Sequential);
    assert_eq!(
        frame(&mut session, picks),
        vec![SessionAction::ApplyMode(PlaybackMode::Sequential)]
    );
}

#[test]
fn viewport_focus_loss_synthesizes_blur_for_the_focused_field() {
    let mut session = opened();
    *session.path_text_mut() = "E:\\照片".to_owned();
    *session.path_focus_mut() = true;
    session.begin_frame(&effective());
    assert_eq!(session.end_frame(no_picks(), true), vec![]);
    // 点相框：设置窗视口失去系统焦点，持有焦点记忆的字段按失焦处理——
    // 本帧只记待结算，下一帧帧尾才提交（与普通失焦同一条延迟路径）。
    session.begin_frame(&effective());
    assert_eq!(session.end_frame(no_picks(), false), vec![]);
    assert_eq!(
        frame(&mut session, no_picks()),
        vec![SessionAction::CommitFolder("E:\\照片".to_owned())]
    );
}

#[test]
fn invalid_and_blank_paths_commit_verbatim() {
    // 无效路径照样提交（由图库扫描落到空状态，文字保留）——原
    // settings_draft 用例改走会话 interface 的对应项。
    let mut session = opened();
    *session.path_text_mut() = r"Z:\不存在的目录".to_owned();
    session.note_enter(Field::Path);
    assert_eq!(
        frame(&mut session, no_picks()),
        vec![SessionAction::CommitFolder(r"Z:\不存在的目录".to_owned())]
    );
    let mut session = opened();
    *session.path_text_mut() = "   ".to_owned();
    session.note_enter(Field::Path);
    assert_eq!(
        frame(&mut session, no_picks()),
        vec![SessionAction::CommitFolder(String::new())]
    );
}

#[test]
fn seconds_same_value_enter_is_a_noop() {
    let mut session = opened();
    session.note_enter(Field::Seconds);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    assert_eq!(session.seconds_text(), "30");
}

#[test]
fn current_image_picks_emit_actions_in_order() {
    let mut session = opened();
    let mut picks = no_picks();
    picks.open_image = true;
    picks.open_folder = true;
    picks.copy_path = true;
    assert_eq!(
        frame(&mut session, picks),
        vec![
            SessionAction::OpenCurrentImage,
            SessionAction::OpenCurrentFolder,
            SessionAction::CopyCurrentPath,
        ]
    );
}

// ---------- 外观模式点选（V2 相框外观专题工单 01） ----------

#[test]
fn an_appearance_pick_applies_and_the_same_value_is_guarded() {
    let mut session = opened();
    // 点当前生效值（内置画框）：同值守卫，不产生动作。
    let mut picks = no_picks();
    picks.appearance = Some(AppearanceMode::Builtin);
    assert_eq!(frame(&mut session, picks), vec![]);
    // 点极简：产生一次应用动作；本帧内再点同值已被内部推进守卫。
    let mut picks = no_picks();
    picks.appearance = Some(AppearanceMode::Minimal);
    assert_eq!(
        frame(&mut session, picks),
        vec![SessionAction::ApplyAppearance(AppearanceMode::Minimal)]
    );
}

#[test]
fn appearance_pick_orders_after_mode_and_before_interval_preset() {
    let mut session = opened();
    let mut picks = no_picks();
    picks.mode = Some(PlaybackMode::Sequential);
    picks.appearance = Some(AppearanceMode::Custom);
    picks.interval = Some(60);
    assert_eq!(
        frame(&mut session, picks),
        vec![
            SessionAction::ApplyMode(PlaybackMode::Sequential),
            SessionAction::ApplyAppearance(AppearanceMode::Custom),
            SessionAction::ApplyIntervalPreset(60),
        ]
    );
}

// ---------- 内置画框参数点选（V2 相框外观专题工单 02） ----------

#[test]
fn builtin_frame_picks_apply_and_same_values_are_guarded() {
    let mut session = opened();
    // 点当前生效值（默认胡桃木 / 框宽 14 / 卡纸宽 18）：同值守卫，
    // 不产生动作。
    let mut picks = no_picks();
    picks.frame_preset = Some(FramePreset::Walnut);
    picks.frame_width = Some(14.0);
    picks.mat_width = Some(18.0);
    assert_eq!(frame(&mut session, picks), vec![]);
    // 改预设与两宽：各产出一个动作，顺序为预设 → 框宽 → 卡纸宽。
    let mut picks = no_picks();
    picks.frame_preset = Some(FramePreset::Oak);
    picks.frame_width = Some(22.0);
    picks.mat_width = Some(0.0);
    assert_eq!(
        frame(&mut session, picks),
        vec![
            SessionAction::ApplyFramePreset(FramePreset::Oak),
            SessionAction::ApplyFrameWidth(22.0),
            SessionAction::ApplyMatWidth(0.0),
        ]
    );
}

#[test]
fn builtin_frame_picks_order_after_appearance_and_before_interval_preset() {
    let mut session = opened();
    let mut picks = no_picks();
    picks.appearance = Some(AppearanceMode::Minimal);
    picks.frame_width = Some(20.0);
    picks.interval = Some(60);
    assert_eq!(
        frame(&mut session, picks),
        vec![
            SessionAction::ApplyAppearance(AppearanceMode::Minimal),
            SessionAction::ApplyFrameWidth(20.0),
            SessionAction::ApplyIntervalPreset(60),
        ]
    );
}

// ---------- 选相框图（V2 相框外观专题工单 03） ----------

#[test]
fn frame_art_pick_vetoes_a_pending_commit_and_keeps_the_draft_text() {
    let mut session = opened();
    *session.path_text_mut() = "E:\\照片".to_owned();
    session.note_blur(Field::Path);
    assert_eq!(frame(&mut session, no_picks()), vec![]);
    let mut picks = no_picks();
    picks.frame_art_pick = true;
    assert_eq!(
        frame(&mut session, picks),
        vec![SessionAction::PickFrameArt]
    );
    // 与选文件夹同规：文字保留、未提交，后续帧不补交。
    assert_eq!(session.path_text(), "E:\\照片");
    assert_eq!(frame(&mut session, no_picks()), vec![]);
}

#[test]
fn frame_art_pick_in_the_same_frame_as_blur_drops_the_blur() {
    let mut session = opened();
    *session.path_text_mut() = "E:\\照片".to_owned();
    session.note_blur(Field::Path);
    let mut picks = no_picks();
    picks.frame_art_pick = true;
    assert_eq!(
        frame(&mut session, picks),
        vec![SessionAction::PickFrameArt]
    );
    assert_eq!(frame(&mut session, no_picks()), vec![]);
}

// ---------- 切换动画点选（切换动画专题工单 04） ----------

#[test]
fn an_animation_pick_applies_and_the_same_value_is_guarded() {
    let mut session = opened();
    // 点当前生效值（卷页，默认档）：同值守卫，不产生动作。
    let mut picks = no_picks();
    picks.animation = Some(Animation::Curl);
    assert_eq!(frame(&mut session, picks), vec![]);
    // 点墨晕：产生一次应用动作。
    let mut picks = no_picks();
    picks.animation = Some(Animation::Ink);
    assert_eq!(
        frame(&mut session, picks),
        vec![SessionAction::ApplyAnimation(Animation::Ink)]
    );
}

#[test]
fn animation_pick_orders_after_mode_and_before_appearance() {
    let mut session = opened();
    let mut picks = no_picks();
    picks.mode = Some(PlaybackMode::Sequential);
    picks.animation = Some(Animation::Random);
    picks.appearance = Some(AppearanceMode::Custom);
    picks.interval = Some(60);
    assert_eq!(
        frame(&mut session, picks),
        vec![
            SessionAction::ApplyMode(PlaybackMode::Sequential),
            SessionAction::ApplyAnimation(Animation::Random),
            SessionAction::ApplyAppearance(AppearanceMode::Custom),
            SessionAction::ApplyIntervalPreset(60),
        ]
    );
}

#[test]
fn window_snap_pick_applies_once_with_same_value_guard() {
    // 窗口吸附开关（多实例吸附专题工单 02）：即时控件，与其他即时
    // 控件同规——点选产出一次动作；宿主应用后快照变为新值，同值
    // 点选被守卫吞掉。
    let mut session = opened();
    let picks = Picks {
        window_snap: Some(false),
        ..Default::default()
    };
    assert_eq!(
        frame(&mut session, picks),
        vec![SessionAction::ApplyWindowSnap(false)]
    );
    // 宿主已应用 false：下一帧快照为 false，同值点选无动作。
    let applied = EffectiveValues {
        window_snap: false,
        ..effective()
    };
    session.begin_frame(&applied);
    let picks = Picks {
        window_snap: Some(false),
        ..Default::default()
    };
    assert_eq!(session.end_frame(picks, true), vec![]);
    let picks = Picks {
        window_snap: Some(true),
        ..Default::default()
    };
    assert_eq!(
        session.end_frame(picks, true),
        vec![SessionAction::ApplyWindowSnap(true)]
    );
}
