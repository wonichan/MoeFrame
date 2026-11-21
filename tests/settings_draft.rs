//! 工单 19：草稿结算语义——提交、作废、同值不动、无效保留，纯决策层验证。
use moe_frame::settings_draft::{settle_path_draft, settle_seconds_draft, DraftSettle};

#[test]
fn a_changed_path_commits_and_keeps_the_submitted_text() {
    let (outcome, display, base) = settle_path_draft("  E:\\新图库  ", "D:\\图库", "D:\\图库");
    assert_eq!(outcome, DraftSettle::CommitFolder("E:\\新图库".to_owned()));
    assert_eq!(display, "E:\\新图库");
    assert_eq!(base, "E:\\新图库");
}

#[test]
fn an_unchanged_path_is_a_noop() {
    let (outcome, display, _) = settle_path_draft(" D:\\图库 ", "D:\\图库", "D:\\图库");
    assert_eq!(outcome, DraftSettle::Noop);
    assert_eq!(display, "D:\\图库");
}

#[test]
fn a_path_draft_superseded_by_a_menu_pick_is_discarded() {
    // 基线 D:\图库、生效值已被菜单改成 E:\别的：草稿作废，显示生效值。
    let (outcome, display, base) = settle_path_draft("F:\\打字中", "D:\\图库", "E:\\别的");
    assert_eq!(outcome, DraftSettle::Discarded);
    assert_eq!(display, "E:\\别的");
    assert_eq!(base, "E:\\别的");
}

#[test]
fn an_invalid_or_blank_path_still_commits_verbatim() {
    // 无效路径保留提交文本、图库落空状态，是核心扫描的事，结算层照常提交。
    let (outcome, _, _) = settle_path_draft("C:\\不存在", "D:\\图库", "D:\\图库");
    assert_eq!(outcome, DraftSettle::CommitFolder("C:\\不存在".to_owned()));
    let (outcome, display, _) = settle_path_draft("   ", "D:\\图库", "D:\\图库");
    assert_eq!(outcome, DraftSettle::CommitFolder(String::new()));
    assert_eq!(display, "");
}

#[test]
fn a_changed_seconds_commits_with_the_clamped_result_displayed() {
    let (outcome, display, base) = settle_seconds_draft("45", 30, 30);
    assert_eq!(outcome, DraftSettle::CommitInterval(45));
    assert_eq!(display, "45");
    assert_eq!(base, 45);
    let (outcome, display, _) = settle_seconds_draft("100000000", 30, 30);
    assert_eq!(outcome, DraftSettle::CommitInterval(99_999_999));
    assert_eq!(display, "99999999");
    let (outcome, display, _) = settle_seconds_draft("2", 30, 30);
    assert_eq!(outcome, DraftSettle::CommitInterval(3));
    assert_eq!(display, "3");
    let (outcome, display, _) = settle_seconds_draft(" 42 ", 30, 30);
    assert_eq!(outcome, DraftSettle::CommitInterval(42));
    assert_eq!(display, "42");
}

#[test]
fn unparseable_seconds_keep_the_effective_value() {
    for bad in ["", "3.5", "30.0", "abc", "1e3"] {
        let (outcome, display, base) = settle_seconds_draft(bad, 30, 30);
        assert_eq!(outcome, DraftSettle::Noop, "输入 {bad:?}");
        assert_eq!(display, "30", "输入 {bad:?} 应恢复显示生效值");
        assert_eq!(base, 30);
    }
}

#[test]
fn same_value_seconds_and_a_superseded_draft_do_not_commit() {
    let (outcome, _, _) = settle_seconds_draft("30", 30, 30);
    assert_eq!(outcome, DraftSettle::Noop);
    // 预设已把生效值改成 60：草稿作废，不覆盖预设。
    let (outcome, display, base) = settle_seconds_draft("45", 30, 60);
    assert_eq!(outcome, DraftSettle::Discarded);
    assert_eq!(display, "60");
    assert_eq!(base, 60);
}
