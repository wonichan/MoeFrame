//! 设置窗会话机（architecture-deepening 工单 02）：工单 19 的全部时序
//! 规则（种子、帧尾结算、重扫否决、关闭丢弃、快照作废、即时控件顺序）
//! 在会话 interface 上验证——测试按帧喂事件与点选，断言每帧的动作。
//! 原 tests/settings_draft.rs 的结算用例已改走本 interface 表达。

use moe_frame::{
    config::PlaybackMode,
    decode_validation::DisplayMode,
    settings_session::{
        EffectiveValues, Field, OpenDecision, Picks, SessionAction, SettingsSession,
    },
};

fn effective() -> EffectiveValues {
    EffectiveValues {
        folder_text: r"D:\图库".to_owned(),
        interval_secs: 30,
        mode: PlaybackMode::Random,
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
    picks.mode = Some(PlaybackMode::Random); // 与当前相同：不产生动作
    picks.interval = Some(60);
    picks.fit = Some(DisplayMode::Fill);
    picks.aspect_lock = Some(true);
    picks.restore_default = true;
    picks.autostart = Some(true);
    assert_eq!(
        frame(&mut session, picks),
        vec![
            SessionAction::ApplyIntervalPreset(60),
            SessionAction::ApplyFitMode(DisplayMode::Fill),
            SessionAction::ApplyAspectLock(true),
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
