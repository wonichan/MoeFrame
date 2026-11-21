//! 工单 11/12/13/16 的播放序列测试，工单 03（深挖）起全部改走导航决策
//! interface：计划（plan_navigation / plan_first_load / plan_resume_load /
//! rescan）、落位（land / initial_landing）与续播快照（resume_snapshot）。
//! 候选与落位结果只在这个测试面断言；随机性由构造注入的确定性实现控制。

mod common;

use common::{IdentityRng, ZeroRng};
use moe_frame::config::PlaybackMode;
use moe_frame::playlist::{Direction, Landing, LandingKind, Playlist, RescanPlan};
use std::path::PathBuf;

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

fn p(name: &str) -> PathBuf {
    PathBuf::from(name)
}

fn seq_list(names: &[&str]) -> Playlist {
    Playlist::new(
        paths(names),
        PlaybackMode::Sequential,
        Box::new(IdentityRng),
    )
}

fn random_list(names: &[&str]) -> Playlist {
    Playlist::new(paths(names), PlaybackMode::Random, Box::new(IdentityRng))
}

/// 一次新鲜选择的成功显示（候选由测试直接指定胜出者）。
fn land_fresh(list: &mut Playlist, name: &str) {
    assert!(list.land(Landing::fresh(), &p(name)));
}

#[test]
fn empty_playlist_has_no_current_and_no_candidates() {
    let mut list = seq_list(&[]);
    assert_eq!(list.current_path(), None);
    assert_eq!(list.plan_navigation(Direction::Next), None);
    assert_eq!(list.plan_navigation(Direction::Prev), None);
    assert_eq!(list.preload_target(), None);
}

#[test]
fn single_image_never_rotates() {
    let mut list = seq_list(&["a.jpg"]);
    land_fresh(&mut list, "a.jpg");
    // 单图：没有「下一张」，导航与预加载都无目标，避免自转忙循环。
    assert_eq!(list.plan_navigation(Direction::Next), None);
    assert_eq!(list.plan_navigation(Direction::Prev), None);
    assert_eq!(list.preload_target(), None);
}

#[test]
fn next_wraps_from_tail_to_head() {
    let mut list = seq_list(&["a.jpg", "b.jpg", "c.jpg"]);
    land_fresh(&mut list, "c.jpg");
    assert_eq!(list.preload_target(), Some(p("a.jpg")));
    let plan = list.plan_navigation(Direction::Next).expect("plan");
    assert_eq!(plan.candidates, paths(&["a.jpg", "b.jpg"]));
    assert_eq!(plan.landing.kind(), LandingKind::Fresh);
}

#[test]
fn navigation_rotation_follows_direction_and_excludes_current() {
    let mut list = seq_list(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    land_fresh(&mut list, "b.jpg");
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["c.jpg", "d.jpg", "a.jpg"])
    );
    assert_eq!(
        list.plan_navigation(Direction::Prev)
            .expect("plan")
            .candidates,
        paths(&["a.jpg", "d.jpg", "c.jpg"])
    );
}

#[test]
fn landing_positions_at_the_shown_path_and_rejects_unknown() {
    let mut list = seq_list(&["a.jpg", "b.jpg", "c.jpg"]);
    land_fresh(&mut list, "a.jpg");
    // 加载时 b 坏了跳到 c：落位按实际显示路径走。
    land_fresh(&mut list, "c.jpg");
    assert_eq!(list.current_path(), Some(&p("c.jpg")));
    assert!(!list.land(Landing::fresh(), &p("zzz.jpg")));
    assert_eq!(list.current_path(), Some(&p("c.jpg")));
}

#[test]
fn no_current_means_no_rotation() {
    let mut list = seq_list(&["a.jpg", "b.jpg"]);
    assert_eq!(list.plan_navigation(Direction::Next), None);
    assert_eq!(list.plan_navigation(Direction::Prev), None);
    assert_eq!(list.preload_target(), None);
}

// ---------- 工单 12：随机洗牌袋、本轮进度与历史 ----------

#[test]
fn picks_come_only_from_unseen_until_the_round_is_complete() {
    let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    land_fresh(&mut list, "a.jpg");
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["b.jpg", "c.jpg", "d.jpg"])
    );
    land_fresh(&mut list, "c.jpg");
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["b.jpg", "d.jpg"])
    );
    land_fresh(&mut list, "b.jpg");
    land_fresh(&mut list, "d.jpg");
    // 全看完才开新一轮；新一轮候选不含刚显示完的 d（轮间避让），其余回归。
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["a.jpg", "b.jpg", "c.jpg"])
    );
}

#[test]
fn two_image_rounds_alternate_by_avoidance() {
    let mut list = random_list(&["a.jpg", "b.jpg"]);
    land_fresh(&mut list, "a.jpg");
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["b.jpg"])
    );
    land_fresh(&mut list, "b.jpg");
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["a.jpg"])
    );
}

#[test]
fn single_image_random_has_no_candidates() {
    let mut list = random_list(&["a.jpg"]);
    land_fresh(&mut list, "a.jpg");
    assert_eq!(list.plan_navigation(Direction::Next), None);
    assert_eq!(list.preload_target(), None);
}

#[test]
fn candidates_and_peeks_record_nothing() {
    let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg"]);
    land_fresh(&mut list, "a.jpg");
    let before = list.resume_snapshot();
    let _ = list.plan_navigation(Direction::Next);
    let _ = list.preload_target();
    let _ = list.plan_navigation(Direction::Prev);
    // 预加载与候选计算不得提前记账（父规格 §3）。
    assert_eq!(list.resume_snapshot(), before);
    assert!(!list.resume_snapshot().seen.contains(&p("b.jpg")));
}

#[test]
fn history_back_and_forward_walk_the_actual_display_order() {
    let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg"]);
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        land_fresh(&mut list, name);
    }
    let back = list.plan_navigation(Direction::Prev).expect("plan");
    assert_eq!(back.candidates, paths(&["b.jpg", "a.jpg"]));
    assert_eq!(back.landing.kind(), LandingKind::Back);
    assert!(list.land(back.landing, &p("b.jpg")));
    assert_eq!(list.current_path(), Some(&p("b.jpg")));
    // 回看不消耗本轮进度：b 仍算已看。
    assert!(list.resume_snapshot().seen.contains(&p("b.jpg")));
    let forward = list.plan_navigation(Direction::Next).expect("plan");
    assert_eq!(forward.candidates, paths(&["c.jpg"]));
    assert_eq!(forward.landing.kind(), LandingKind::Forward);
    assert!(list.land(forward.landing, &p("c.jpg")));
    // 回到最新端后，下一张是新一轮的新鲜选择（备袋不含刚显示的 c）。
    let next = list.plan_navigation(Direction::Next).expect("plan");
    assert_eq!(next.candidates, paths(&["a.jpg", "b.jpg"]));
    assert_eq!(next.landing.kind(), LandingKind::Fresh);
}

#[test]
fn history_walk_skips_paths_no_longer_in_the_library() {
    // 恢复场景（工单 13 的构造路径）：历史里混有已不在图库的路径；
    // 当前图正是历史尾部在图库内的 c。
    let mut list = Playlist::restored(
        paths(&["a.jpg", "c.jpg"]),
        PlaybackMode::Random,
        Box::new(IdentityRng),
        Some(p("c.jpg")),
        paths(&["a.jpg", "c.jpg"]),
        paths(&["a.jpg", "gone.jpg", "c.jpg"]),
    );
    assert_eq!(list.current_path(), Some(&p("c.jpg")));
    assert_eq!(
        list.plan_navigation(Direction::Prev)
            .expect("plan")
            .candidates,
        paths(&["a.jpg"])
    );
}

#[test]
fn history_is_capped_at_512_entries() {
    let names: Vec<String> = (0..600).map(|i| format!("img{i}.jpg")).collect();
    let items: Vec<PathBuf> = names.iter().map(PathBuf::from).collect();
    let mut list = Playlist::new(items, PlaybackMode::Random, Box::new(ZeroRng));
    for i in 0..513 {
        land_fresh(&mut list, &format!("img{i}.jpg"));
    }
    let history = list.resume_snapshot().history;
    assert_eq!(history.len(), 512);
    assert_eq!(history[0], p("img1.jpg"));
    assert_eq!(history[511], p("img512.jpg"));
}

#[test]
fn a_prepared_new_round_is_not_committed_until_a_successful_display() {
    let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg"]);
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        land_fresh(&mut list, name);
    }
    // 备袋（计划与预加载走的路径）不得提前清本轮进度（父规格 §3）。
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["a.jpg", "b.jpg"])
    );
    assert_eq!(
        list.resume_snapshot().seen,
        paths(&["a.jpg", "b.jpg", "c.jpg"])
    );
    assert_eq!(list.preload_target(), Some(p("a.jpg")));
    assert!(list.resume_snapshot().seen.contains(&p("b.jpg")));
    // 本轮首张成功显示后才提交新一轮：只记新图，上一轮最后的 c 放回袋中。
    land_fresh(&mut list, "a.jpg");
    assert_eq!(list.resume_snapshot().seen, paths(&["a.jpg"]));
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["b.jpg", "c.jpg"])
    );
}

// ---------- 导航计划全程（自核心测试搬家，工单 03 深挖） ----------

#[test]
fn random_next_draws_only_unseen_and_a_new_round_avoids_the_last_image() {
    let mut list = random_list(&["a.png", "b.png", "c.png"]);
    assert_eq!(list.plan_first_load(), paths(&["a.png", "b.png", "c.png"]));
    land_fresh(&mut list, "a.png");
    // 首图显示后预加载的是袋首（未看路径），快照记本次成功显示。
    assert_eq!(list.preload_target(), Some(p("b.png")));
    let snapshot = list.resume_snapshot();
    assert_eq!(snapshot.current, Some(p("a.png")));
    assert_eq!(snapshot.seen, paths(&["a.png"]));
    assert_eq!(snapshot.history, paths(&["a.png"]));
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["b.png", "c.png"])
    );
    land_fresh(&mut list, "b.png");
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["c.png"])
    );
    land_fresh(&mut list, "c.png");
    // 全看完：新一轮候选避开刚显示的 c。
    assert_eq!(
        list.plan_navigation(Direction::Next)
            .expect("plan")
            .candidates,
        paths(&["a.png", "b.png"])
    );
}

#[test]
fn a_random_first_image_is_drawn_from_the_shuffle_bag() {
    let mut list = Playlist::new(
        paths(&["a.png", "b.png", "c.png"]),
        PlaybackMode::Random,
        Box::new(ZeroRng),
    );
    // ZeroRng 洗牌 [a,b,c] → [b,c,a]：首图候选是袋序，不是自然序。
    assert_eq!(list.plan_first_load(), paths(&["b.png", "c.png", "a.png"]));
}

// ---------- 工单 13：跨启动恢复的播放序列构造与首图计划 ----------

fn restored_list(
    items: &[&str],
    mode: PlaybackMode,
    current: Option<&str>,
    seen: &[&str],
    history: &[&str],
) -> Playlist {
    Playlist::restored(
        paths(items),
        mode,
        Box::new(IdentityRng),
        current.map(p),
        paths(seen),
        paths(history),
    )
}

#[test]
fn restored_positions_at_the_saved_current_inside_history() {
    let mut list = restored_list(
        &["a.jpg", "b.jpg", "c.jpg"],
        PlaybackMode::Random,
        Some("b.jpg"),
        &["a.jpg", "b.jpg"],
        &["a.jpg", "b.jpg", "c.jpg"],
    );
    assert_eq!(list.current_path(), Some(&p("b.jpg")));
    // 游标停在 b 的历史位置：向后只剩 a，向前还有 c。
    assert_eq!(
        list.plan_navigation(Direction::Prev)
            .expect("plan")
            .candidates,
        paths(&["a.jpg"])
    );
    let forward = list.plan_navigation(Direction::Next).expect("plan");
    assert_eq!(forward.candidates, paths(&["c.jpg"]));
    assert_eq!(forward.landing.kind(), LandingKind::Forward);
    // 本轮进度按当前图库过滤后保留。
    assert_eq!(list.resume_snapshot().seen, paths(&["a.jpg", "b.jpg"]));
}

#[test]
fn restored_current_outside_history_walks_the_whole_history_backwards() {
    let mut list = restored_list(
        &["a.jpg", "b.jpg", "c.jpg"],
        PlaybackMode::Random,
        Some("c.jpg"),
        &["c.jpg"],
        &["a.jpg", "b.jpg"],
    );
    assert_eq!(list.current_path(), Some(&p("c.jpg")));
    assert_eq!(
        list.plan_navigation(Direction::Prev)
            .expect("plan")
            .candidates,
        paths(&["b.jpg", "a.jpg"])
    );
    // 向前无历史可返：下一张是新鲜选择，不是历史返回。
    let next = list.plan_navigation(Direction::Next).expect("plan");
    assert_eq!(next.landing.kind(), LandingKind::Fresh);
    assert_eq!(next.candidates, paths(&["a.jpg", "b.jpg"]));
}

#[test]
fn a_missing_saved_current_suppresses_the_history_tail_fallback() {
    // 当前图字段在、但路径已不在图库：不拿历史尾部冒充当前图；
    // 随机模式下旧本轮进度整轮作废（工单 13 准则 4），对当前图库开新一轮。
    let list = restored_list(
        &["a.jpg", "b.jpg"],
        PlaybackMode::Random,
        Some("gone.jpg"),
        &["a.jpg", "gone.jpg"],
        &["a.jpg", "b.jpg"],
    );
    assert_eq!(list.current_path(), None);
    assert!(list.resume_snapshot().seen.is_empty());
}

#[test]
fn a_missing_sequential_current_keeps_the_filtered_progress() {
    // 顺序模式当前图缺失：进度只按图库过滤，不整轮作废（准则 5）。
    let list = restored_list(
        &["a.jpg", "b.jpg"],
        PlaybackMode::Sequential,
        Some("gone.jpg"),
        &["a.jpg", "gone.jpg"],
        &["a.jpg", "b.jpg"],
    );
    assert_eq!(list.current_path(), None);
    assert_eq!(list.resume_snapshot().seen, paths(&["a.jpg"]));
}

#[test]
fn an_absent_current_positions_nothing() {
    // 当前图字段缺失/损坏被丢弃时按缺失处理（工单 02 决定 10）：
    // 不拿历史尾部冒充当前图；本轮进度与历史本身仍按各自字段恢复。
    let list = restored_list(
        &["a.jpg", "b.jpg"],
        PlaybackMode::Random,
        None,
        &["a.jpg"],
        &["a.jpg", "gone.jpg", "b.jpg"],
    );
    assert_eq!(list.current_path(), None);
    assert_eq!(list.resume_snapshot().seen, paths(&["a.jpg"]));
    assert_eq!(list.resume_snapshot().history.len(), 3);
}

#[test]
fn landing_the_restored_image_records_nothing() {
    let mut list = restored_list(
        &["a.jpg", "b.jpg", "c.jpg"],
        PlaybackMode::Random,
        Some("b.jpg"),
        &["a.jpg", "b.jpg"],
        &["a.jpg", "b.jpg", "c.jpg"],
    );
    let landing = list.initial_landing(&p("b.jpg"));
    assert_eq!(landing.kind(), LandingKind::Restored);
    assert!(list.land(landing, &p("b.jpg")));
    // 恢复显示不重复记账：历史不追加、本轮进度不变，游标仍停在 b 处。
    let snapshot = list.resume_snapshot();
    assert_eq!(snapshot.history, paths(&["a.jpg", "b.jpg", "c.jpg"]));
    assert_eq!(snapshot.seen, paths(&["a.jpg", "b.jpg"]));
    assert_eq!(
        list.plan_navigation(Direction::Prev)
            .expect("plan")
            .candidates,
        paths(&["a.jpg"])
    );
}

#[test]
fn resume_load_restores_the_current_first_then_continues_by_mode() {
    // 随机：先恢复 b；b 加载失败时才轮到未看的 c。
    let mut list = restored_list(
        &["a.jpg", "b.jpg", "c.jpg"],
        PlaybackMode::Random,
        Some("b.jpg"),
        &["a.jpg", "b.jpg"],
        &["a.jpg", "b.jpg"],
    );
    assert_eq!(list.plan_resume_load(), paths(&["b.jpg", "c.jpg"]));
    // 顺序：先恢复 b，其后沿轮转（c、a）。
    let mut list = restored_list(
        &["a.jpg", "b.jpg", "c.jpg"],
        PlaybackMode::Sequential,
        Some("b.jpg"),
        &["a.jpg", "b.jpg"],
        &["a.jpg", "b.jpg"],
    );
    assert_eq!(list.plan_resume_load(), paths(&["b.jpg", "c.jpg", "a.jpg"]));
}

#[test]
fn resume_load_with_a_missing_current_follows_the_mode_rule() {
    // 随机当前图缺失：全图库洗牌袋（恒等洗牌下即自然序）。
    let mut list = restored_list(
        &["a.jpg", "b.jpg"],
        PlaybackMode::Random,
        Some("gone.jpg"),
        &["a.jpg", "gone.jpg"],
        &["a.jpg", "gone.jpg"],
    );
    assert_eq!(list.plan_resume_load(), paths(&["a.jpg", "b.jpg"]));
    // 顺序当前图缺失：从自然序中它的下一张起循环寻找。
    let mut list = restored_list(
        &["a.jpg", "c.jpg", "d.jpg"],
        PlaybackMode::Sequential,
        Some("b.jpg"),
        &[],
        &[],
    );
    assert_eq!(list.plan_resume_load(), paths(&["c.jpg", "d.jpg", "a.jpg"]));
    // 顺序且无当前图字段：全量自然序。
    let mut list = restored_list(
        &["a.jpg", "b.jpg"],
        PlaybackMode::Sequential,
        None,
        &[],
        &[],
    );
    assert_eq!(list.plan_resume_load(), paths(&["a.jpg", "b.jpg"]));
}

#[test]
fn initial_landing_marks_only_the_restored_current() {
    let mut list = restored_list(
        &["a.jpg", "b.jpg", "c.jpg"],
        PlaybackMode::Random,
        Some("b.jpg"),
        &["a.jpg", "b.jpg"],
        &["a.jpg", "b.jpg"],
    );
    // 恢复图加载失败、袋中 c 胜出：新鲜记账，不是恢复落位。
    assert_eq!(list.initial_landing(&p("c.jpg")).kind(), LandingKind::Fresh);
    // 恢复判定只消费一次：之后再问 b 也是新鲜。
    assert_eq!(list.initial_landing(&p("b.jpg")).kind(), LandingKind::Fresh);
}

// ---------- 工单 16：重扫合并 ----------

#[test]
fn rescan_purges_deleted_progress_and_joins_additions_to_the_round() {
    let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    land_fresh(&mut list, "a.jpg");
    land_fresh(&mut list, "b.jpg");
    // c 被删、e 新增：当前图 b 保留，本轮不重洗。
    assert_eq!(
        list.rescan(paths(&["a.jpg", "b.jpg", "d.jpg", "e.jpg"])),
        RescanPlan::Survived
    );
    assert_eq!(list.current_path(), Some(&p("b.jpg")));
    // 剩余未看恰为 {d, e}：c 已剔除，e 作为本轮未看并入。
    let mut candidates = list
        .plan_navigation(Direction::Next)
        .expect("plan")
        .candidates;
    candidates.sort();
    assert_eq!(candidates, paths(&["d.jpg", "e.jpg"]));
    let seen = list.resume_snapshot().seen;
    assert!(seen.contains(&p("a.jpg")));
    assert!(!seen.contains(&p("e.jpg")));
}

#[test]
fn rescan_with_the_current_image_deleted_clears_random_progress() {
    let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg"]);
    land_fresh(&mut list, "a.jpg");
    land_fresh(&mut list, "b.jpg");
    let RescanPlan::Missing { mut candidates } = list.rescan(paths(&["a.jpg", "c.jpg"])) else {
        panic!("当前图缺失应给出替代候选");
    };
    candidates.sort();
    assert_eq!(candidates, paths(&["a.jpg", "c.jpg"]));
    assert_eq!(list.current_path(), None);
    // 随机模式当前图缺失：旧本轮进度整轮作废，新一轮候选是全图库。
    assert!(list.resume_snapshot().seen.is_empty());
}

#[test]
fn rescan_with_the_current_image_deleted_keeps_sequential_progress() {
    let mut list = seq_list(&["a.jpg", "b.jpg", "c.jpg"]);
    land_fresh(&mut list, "a.jpg");
    land_fresh(&mut list, "b.jpg");
    // 顺序模式替代候选从缺失图 b 的自然序后继 c 起循环。
    assert_eq!(
        list.rescan(paths(&["a.jpg", "c.jpg"])),
        RescanPlan::Missing {
            candidates: paths(&["c.jpg", "a.jpg"]),
        }
    );
    assert_eq!(list.current_path(), None);
    // 顺序模式只剔除已删路径的进度，仍在图库的进度保留。
    assert_eq!(list.resume_snapshot().seen, paths(&["a.jpg"]));
}

#[test]
fn rescan_keeps_history_and_back_walking_skips_deleted_paths() {
    let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg"]);
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        land_fresh(&mut list, name);
    }
    assert_eq!(
        list.rescan(paths(&["a.jpg", "c.jpg"])),
        RescanPlan::Survived
    );
    // 历史原样保留；回看候选跳过已删的 b，不把它写回本轮进度。
    assert_eq!(
        list.resume_snapshot().history,
        paths(&["a.jpg", "b.jpg", "c.jpg"])
    );
    assert_eq!(
        list.plan_navigation(Direction::Prev)
            .expect("plan")
            .candidates,
        paths(&["a.jpg"])
    );
    assert!(!list.resume_snapshot().seen.contains(&p("b.jpg")));
}

#[test]
fn rescan_to_an_empty_library_leaves_nothing() {
    let mut list = random_list(&["a.jpg"]);
    land_fresh(&mut list, "a.jpg");
    assert_eq!(list.rescan(paths(&[])), RescanPlan::EmptyLibrary);
    assert_eq!(list.len(), 0);
    assert_eq!(list.current_path(), None);
    assert_eq!(list.plan_navigation(Direction::Next), None);
}

#[test]
fn rescan_preserves_the_relative_order_of_surviving_unseen_paths() {
    let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg", "d.jpg", "e.jpg"]);
    land_fresh(&mut list, "a.jpg");
    // 先把洗牌袋备出来（顺序已定），再删 b、加 f 重扫。
    let bag_before = list
        .plan_navigation(Direction::Next)
        .expect("plan")
        .candidates;
    assert_eq!(
        list.rescan(paths(&["a.jpg", "c.jpg", "d.jpg", "e.jpg", "f.jpg"])),
        RescanPlan::Survived
    );
    let bag_after = list
        .plan_navigation(Direction::Next)
        .expect("plan")
        .candidates;
    // 存活未看路径的相对顺序与重扫前一致（不整轮重洗），f 已并入。
    let survivors_before: Vec<&PathBuf> = bag_before
        .iter()
        .filter(|path| *path != &p("b.jpg"))
        .collect();
    let survivors_after: Vec<&PathBuf> = bag_after
        .iter()
        .filter(|path| *path != &p("f.jpg"))
        .collect();
    assert_eq!(survivors_before, survivors_after);
    assert!(bag_after.contains(&p("f.jpg")));
}
