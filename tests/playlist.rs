//! 工单 11：播放序列纯逻辑测试——顺序、循环、轮转候选、空与单图。
mod common;

use moe_frame::playlist::{Direction, Playlist};
use std::path::PathBuf;

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

#[test]
fn empty_playlist_has_no_current_and_no_candidates() {
    let list = Playlist::new(Vec::new(), moe_frame::config::PlaybackMode::Sequential);
    assert_eq!(list.current_path(), None);
    assert_eq!(list.next_path(), None);
    assert!(list.rotation_candidates(Direction::Next).is_empty());
    assert!(list.rotation_candidates(Direction::Prev).is_empty());
}

#[test]
fn single_image_never_rotates() {
    let mut list = Playlist::new(
        paths(&["a.jpg"]),
        moe_frame::config::PlaybackMode::Sequential,
    );
    assert!(list.set_current(&PathBuf::from("a.jpg")));
    // 单图：没有「下一张」，导航与预加载都无目标，避免自转忙循环。
    assert_eq!(list.next_path(), None);
    assert!(list.rotation_candidates(Direction::Next).is_empty());
    assert!(list.rotation_candidates(Direction::Prev).is_empty());
}

#[test]
fn next_wraps_from_tail_to_head() {
    let mut list = Playlist::new(
        paths(&["a.jpg", "b.jpg", "c.jpg"]),
        moe_frame::config::PlaybackMode::Sequential,
    );
    assert!(list.set_current(&PathBuf::from("c.jpg")));
    assert_eq!(list.next_path(), Some(PathBuf::from("a.jpg")));
    assert_eq!(
        list.rotation_candidates(Direction::Next),
        paths(&["a.jpg", "b.jpg"])
    );
}

#[test]
fn rotation_candidates_follow_direction_and_exclude_current() {
    let mut list = Playlist::new(
        paths(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]),
        moe_frame::config::PlaybackMode::Sequential,
    );
    assert!(list.set_current(&PathBuf::from("b.jpg")));
    assert_eq!(
        list.rotation_candidates(Direction::Next),
        paths(&["c.jpg", "d.jpg", "a.jpg"])
    );
    assert_eq!(
        list.rotation_candidates(Direction::Prev),
        paths(&["a.jpg", "d.jpg", "c.jpg"])
    );
}

#[test]
fn set_current_accepts_a_skipped_to_path_and_rejects_unknown() {
    let mut list = Playlist::new(
        paths(&["a.jpg", "b.jpg", "c.jpg"]),
        moe_frame::config::PlaybackMode::Sequential,
    );
    assert!(list.set_current(&PathBuf::from("a.jpg")));
    // 加载时 b 坏了跳到 c：当前位置按实际显示路径落位。
    assert!(list.set_current(&PathBuf::from("c.jpg")));
    assert_eq!(list.current_path(), Some(&PathBuf::from("c.jpg")));
    assert!(!list.set_current(&PathBuf::from("zzz.jpg")));
    assert_eq!(list.current_path(), Some(&PathBuf::from("c.jpg")));
}

#[test]
fn no_current_means_no_rotation() {
    let list = Playlist::new(
        paths(&["a.jpg", "b.jpg"]),
        moe_frame::config::PlaybackMode::Sequential,
    );
    assert_eq!(list.next_path(), None);
    assert!(list.rotation_candidates(Direction::Next).is_empty());
}

// ---------- 工单 12：随机洗牌袋、本轮进度与历史 ----------

mod random_tests {
    use super::common::{IdentityRng, ZeroRng};
    use moe_frame::{
        config::PlaybackMode,
        playlist::{Direction, Playlist},
    };
    use std::path::PathBuf;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    fn random_list(names: &[&str]) -> Playlist {
        Playlist::new(paths(names), PlaybackMode::Random)
    }

    #[test]
    fn picks_come_only_from_unseen_until_the_round_is_complete() {
        let mut rng = IdentityRng;
        let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
        assert!(list.record_fresh(&PathBuf::from("a.jpg"), &mut rng));
        assert_eq!(
            list.bag_candidates(&mut rng),
            paths(&["b.jpg", "c.jpg", "d.jpg"])
        );
        assert!(list.record_fresh(&PathBuf::from("c.jpg"), &mut rng));
        assert_eq!(list.bag_candidates(&mut rng), paths(&["b.jpg", "d.jpg"]));
        assert!(list.record_fresh(&PathBuf::from("b.jpg"), &mut rng));
        assert!(list.record_fresh(&PathBuf::from("d.jpg"), &mut rng));
        // 全看完才开新一轮；新一轮候选不含刚显示完的 d（轮间避让），其余回归。
        assert_eq!(
            list.bag_candidates(&mut rng),
            paths(&["a.jpg", "b.jpg", "c.jpg"])
        );
    }

    #[test]
    fn two_image_rounds_alternate_by_avoidance() {
        let mut rng = IdentityRng;
        let mut list = random_list(&["a.jpg", "b.jpg"]);
        assert!(list.record_fresh(&PathBuf::from("a.jpg"), &mut rng));
        assert_eq!(list.bag_candidates(&mut rng), paths(&["b.jpg"]));
        assert!(list.record_fresh(&PathBuf::from("b.jpg"), &mut rng));
        assert_eq!(list.bag_candidates(&mut rng), paths(&["a.jpg"]));
    }

    #[test]
    fn single_image_random_has_no_candidates() {
        let mut rng = IdentityRng;
        let mut list = random_list(&["a.jpg"]);
        assert!(list.record_fresh(&PathBuf::from("a.jpg"), &mut rng));
        assert!(list.bag_candidates(&mut rng).is_empty());
        assert_eq!(list.preload_target(&mut rng), None);
    }

    #[test]
    fn candidates_and_peeks_record_nothing() {
        let mut rng = IdentityRng;
        let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg"]);
        assert!(list.record_fresh(&PathBuf::from("a.jpg"), &mut rng));
        let history_before = list.history().to_vec();
        let _ = list.bag_candidates(&mut rng);
        let _ = list.preload_target(&mut rng);
        let _ = list.history_back_candidates();
        // 预加载与候选计算不得提前记账（父规格 §3）。
        assert_eq!(list.history(), history_before.as_slice());
        assert!(!list.is_seen(&PathBuf::from("b.jpg")));
    }

    #[test]
    fn history_back_and_forward_walk_the_actual_display_order() {
        let mut rng = IdentityRng;
        let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg"]);
        for name in ["a.jpg", "b.jpg", "c.jpg"] {
            assert!(list.record_fresh(&PathBuf::from(name), &mut rng));
        }
        assert_eq!(list.history_back_candidates(), paths(&["b.jpg", "a.jpg"]));
        assert!(list.land_history(&PathBuf::from("b.jpg"), Direction::Prev));
        assert_eq!(list.current_path(), Some(&PathBuf::from("b.jpg")));
        // 回看不消耗本轮进度：b 仍算已看，袋子不变。
        assert!(list.is_seen(&PathBuf::from("b.jpg")));
        assert_eq!(list.bag_candidates(&mut rng), paths(&["a.jpg", "c.jpg"]));
        assert_eq!(list.history_forward_candidates(), paths(&["c.jpg"]));
        assert!(list.land_history(&PathBuf::from("c.jpg"), Direction::Next));
        // 回到最新图后游标归零（live edge），向前候选耗尽。
        assert_eq!(list.history_cursor(), None);
        assert!(list.history_forward_candidates().is_empty());
    }

    #[test]
    fn history_walk_skips_paths_no_longer_in_the_library() {
        // 恢复场景（工单 13 的构造路径）：历史里混有已不在图库的路径；
        // 工单 13 起 restored 接收显式当前图；此处当前图正是历史尾部在图库内的 c。
        let list = Playlist::restored(
            paths(&["a.jpg", "c.jpg"]),
            PlaybackMode::Random,
            Some(PathBuf::from("c.jpg")),
            paths(&["a.jpg", "c.jpg"]),
            paths(&["a.jpg", "gone.jpg", "c.jpg"]),
        );
        assert_eq!(list.current_path(), Some(&PathBuf::from("c.jpg")));
        assert_eq!(list.history_back_candidates(), paths(&["a.jpg"]));
    }

    #[test]
    fn history_is_capped_at_512_entries() {
        let mut rng = ZeroRng;
        let names: Vec<String> = (0..600).map(|i| format!("img{i}.jpg")).collect();
        let items: Vec<PathBuf> = names.iter().map(PathBuf::from).collect();
        let mut list = Playlist::new(items, PlaybackMode::Random);
        for i in 0..513 {
            let p = PathBuf::from(format!("img{i}.jpg"));
            assert!(list.record_fresh(&p, &mut rng));
        }
        assert_eq!(list.history().len(), 512);
        assert_eq!(list.history()[0], PathBuf::from("img1.jpg"));
        assert_eq!(list.history()[511], PathBuf::from("img512.jpg"));
    }

    #[test]
    fn a_prepared_new_round_is_not_committed_until_a_successful_display() {
        let mut rng = IdentityRng;
        let mut list = random_list(&["a.jpg", "b.jpg", "c.jpg"]);
        for name in ["a.jpg", "b.jpg", "c.jpg"] {
            assert!(list.record_fresh(&PathBuf::from(name), &mut rng));
        }
        // 备袋（候选计算与预加载走的路径）不得提前清本轮进度（父规格 §3）。
        assert_eq!(list.bag_candidates(&mut rng), paths(&["a.jpg", "b.jpg"]));
        assert!(list.is_seen(&PathBuf::from("a.jpg")));
        assert!(list.is_seen(&PathBuf::from("c.jpg")));
        let _ = list.preload_target(&mut rng);
        assert!(list.is_seen(&PathBuf::from("b.jpg")));
        // 本轮首张成功显示后才提交新一轮：只记新图，上一轮最后的 c 放回袋中。
        assert!(list.record_fresh(&PathBuf::from("a.jpg"), &mut rng));
        assert!(list.is_seen(&PathBuf::from("a.jpg")));
        assert!(!list.is_seen(&PathBuf::from("b.jpg")));
        assert!(!list.is_seen(&PathBuf::from("c.jpg")));
        assert!(list
            .bag_candidates(&mut rng)
            .contains(&PathBuf::from("c.jpg")));
    }
}

// ---------- 工单 13：跨启动恢复的播放序列构造 ----------

mod resume_tests {
    use moe_frame::{config::PlaybackMode, playlist::Playlist};
    use std::path::PathBuf;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn restored_positions_at_the_saved_current_inside_history() {
        let list = Playlist::restored(
            paths(&["a.jpg", "b.jpg", "c.jpg"]),
            PlaybackMode::Random,
            Some(PathBuf::from("b.jpg")),
            paths(&["a.jpg", "b.jpg"]),
            paths(&["a.jpg", "b.jpg", "c.jpg"]),
        );
        assert_eq!(list.current_path(), Some(&PathBuf::from("b.jpg")));
        // 游标停在 b 的历史位置：向后只剩 a，向前还有 c。
        assert_eq!(list.history_back_candidates(), paths(&["a.jpg"]));
        assert_eq!(list.history_forward_candidates(), paths(&["c.jpg"]));
        // 本轮进度按当前图库过滤后保留。
        assert!(list.is_seen(&PathBuf::from("a.jpg")));
        assert!(!list.is_seen(&PathBuf::from("c.jpg")));
        assert_eq!(list.seen_paths(), paths(&["a.jpg", "b.jpg"]));
    }

    #[test]
    fn restored_current_outside_history_walks_the_whole_history_backwards() {
        let list = Playlist::restored(
            paths(&["a.jpg", "b.jpg", "c.jpg"]),
            PlaybackMode::Random,
            Some(PathBuf::from("c.jpg")),
            paths(&["c.jpg"]),
            paths(&["a.jpg", "b.jpg"]),
        );
        assert_eq!(list.current_path(), Some(&PathBuf::from("c.jpg")));
        assert_eq!(list.history_back_candidates(), paths(&["b.jpg", "a.jpg"]));
        assert!(list.history_forward_candidates().is_empty());
    }

    #[test]
    fn a_missing_saved_current_suppresses_the_history_tail_fallback() {
        // 当前图字段在、但路径已不在图库：不拿历史尾部冒充当前图（工单 13 准则 4/5
        // 由核心按模式另行决定首图），本轮进度仍按图库过滤。
        let list = Playlist::restored(
            paths(&["a.jpg", "b.jpg"]),
            PlaybackMode::Random,
            Some(PathBuf::from("gone.jpg")),
            paths(&["a.jpg", "gone.jpg"]),
            paths(&["a.jpg", "b.jpg"]),
        );
        assert_eq!(list.current_path(), None);
        assert!(list.is_seen(&PathBuf::from("a.jpg")));
        assert!(!list.is_seen(&PathBuf::from("gone.jpg")));
    }

    #[test]
    fn an_absent_current_positions_nothing() {
        // 当前图字段缺失/损坏被丢弃时按缺失处理（工单 02 决定 10）：
        // 不拿历史尾部冒充当前图，首图由核心按模式另行决定；
        // 本轮进度与历史本身仍按各自字段恢复。
        let list = Playlist::restored(
            paths(&["a.jpg", "b.jpg"]),
            PlaybackMode::Random,
            None,
            paths(&["a.jpg"]),
            paths(&["a.jpg", "gone.jpg", "b.jpg"]),
        );
        assert_eq!(list.current_path(), None);
        assert!(list.is_seen(&PathBuf::from("a.jpg")));
        assert_eq!(list.history().len(), 3);
    }

    #[test]
    fn landing_the_restored_image_records_nothing() {
        let mut list = Playlist::restored(
            paths(&["a.jpg", "b.jpg", "c.jpg"]),
            PlaybackMode::Random,
            Some(PathBuf::from("b.jpg")),
            paths(&["a.jpg", "b.jpg"]),
            paths(&["a.jpg", "b.jpg", "c.jpg"]),
        );
        assert!(list.land_restored(&PathBuf::from("b.jpg")));
        // 恢复显示不重复记账：历史不追加、本轮进度不变，游标仍停在 b 处。
        assert_eq!(list.history(), paths(&["a.jpg", "b.jpg", "c.jpg"]));
        assert_eq!(list.seen_paths(), paths(&["a.jpg", "b.jpg"]));
        assert_eq!(list.history_back_candidates(), paths(&["a.jpg"]));
    }
}

mod rescan_tests {
    use super::common::IdentityRng;
    use moe_frame::{config::PlaybackMode, playlist::Playlist};
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    fn candidate_set(list: &mut Playlist) -> HashSet<PathBuf> {
        list.bag_candidates(&mut IdentityRng).into_iter().collect()
    }

    #[test]
    fn rescan_purges_deleted_progress_and_joins_additions_to_the_round() {
        let mut list = Playlist::new(
            paths(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]),
            PlaybackMode::Random,
        );
        let mut rng = IdentityRng;
        list.record_fresh(&PathBuf::from("a.jpg"), &mut rng);
        list.record_fresh(&PathBuf::from("b.jpg"), &mut rng);
        // c 被删、e 新增：当前图 b 保留，本轮不重洗。
        assert!(list.rescan(paths(&["a.jpg", "b.jpg", "d.jpg", "e.jpg"]), &mut rng));
        assert_eq!(list.current_path(), Some(&PathBuf::from("b.jpg")));
        // 剩余未看恰为 {d, e}：c 已剔除，e 作为本轮未看并入。
        assert_eq!(
            candidate_set(&mut list),
            HashSet::from([PathBuf::from("d.jpg"), PathBuf::from("e.jpg")])
        );
        assert!(list.is_seen(&PathBuf::from("a.jpg")));
        assert!(!list.is_seen(&PathBuf::from("e.jpg")));
    }

    #[test]
    fn rescan_with_the_current_image_deleted_clears_random_progress() {
        let mut list = Playlist::new(paths(&["a.jpg", "b.jpg", "c.jpg"]), PlaybackMode::Random);
        let mut rng = IdentityRng;
        list.record_fresh(&PathBuf::from("a.jpg"), &mut rng);
        list.record_fresh(&PathBuf::from("b.jpg"), &mut rng);
        assert!(!list.rescan(paths(&["a.jpg", "c.jpg"]), &mut rng));
        assert_eq!(list.current_path(), None);
        // 随机模式当前图缺失：旧本轮进度整轮作废，新一轮候选是全图库。
        assert!(!list.is_seen(&PathBuf::from("a.jpg")));
        assert_eq!(
            candidate_set(&mut list),
            HashSet::from([PathBuf::from("a.jpg"), PathBuf::from("c.jpg")])
        );
    }

    #[test]
    fn rescan_with_the_current_image_deleted_keeps_sequential_progress() {
        let mut list = Playlist::new(
            paths(&["a.jpg", "b.jpg", "c.jpg"]),
            PlaybackMode::Sequential,
        );
        let mut rng = IdentityRng;
        list.record_fresh(&PathBuf::from("a.jpg"), &mut rng);
        list.record_fresh(&PathBuf::from("b.jpg"), &mut rng);
        assert!(!list.rescan(paths(&["a.jpg", "c.jpg"]), &mut rng));
        assert_eq!(list.current_path(), None);
        // 顺序模式只剔除已删路径的进度，仍在图库的进度保留。
        assert!(list.is_seen(&PathBuf::from("a.jpg")));
        assert!(!list.is_seen(&PathBuf::from("b.jpg")));
    }

    #[test]
    fn rescan_keeps_history_and_back_walking_skips_deleted_paths() {
        let mut list = Playlist::new(paths(&["a.jpg", "b.jpg", "c.jpg"]), PlaybackMode::Random);
        let mut rng = IdentityRng;
        for name in ["a.jpg", "b.jpg", "c.jpg"] {
            list.record_fresh(&PathBuf::from(name), &mut rng);
        }
        assert!(list.rescan(paths(&["a.jpg", "c.jpg"]), &mut rng));
        // 历史原样保留；回看候选跳过已删的 b，不把它写回本轮进度。
        assert_eq!(list.history(), paths(&["a.jpg", "b.jpg", "c.jpg"]));
        assert_eq!(list.history_back_candidates(), paths(&["a.jpg"]));
        assert!(!list.is_seen(&PathBuf::from("b.jpg")));
    }

    #[test]
    fn rescan_to_an_empty_library_leaves_nothing() {
        let mut list = Playlist::new(paths(&["a.jpg"]), PlaybackMode::Random);
        let mut rng = IdentityRng;
        list.record_fresh(&PathBuf::from("a.jpg"), &mut rng);
        assert!(!list.rescan(paths(&[]), &mut rng));
        assert_eq!(list.len(), 0);
        assert_eq!(list.current_path(), None);
        assert!(list.bag_candidates(&mut rng).is_empty());
    }
}

#[cfg(test)]
mod rescan_order_tests {
    use super::common::IdentityRng;
    use moe_frame::{config::PlaybackMode, playlist::Playlist};
    use std::path::PathBuf;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn rescan_preserves_the_relative_order_of_surviving_unseen_paths() {
        let mut list = Playlist::new(
            paths(&["a.jpg", "b.jpg", "c.jpg", "d.jpg", "e.jpg"]),
            PlaybackMode::Random,
        );
        let mut rng = IdentityRng;
        list.record_fresh(&PathBuf::from("a.jpg"), &mut rng);
        // 先把洗牌袋备出来（顺序已定），再删 b、加 f 重扫。
        let bag_before = list.bag_candidates(&mut rng);
        assert!(list.rescan(
            paths(&["a.jpg", "c.jpg", "d.jpg", "e.jpg", "f.jpg"]),
            &mut rng
        ));
        let bag_after = list.bag_candidates(&mut rng);
        // 存活未看路径的相对顺序与重扫前一致（不整轮重洗），f 已并入。
        let survivors_before: Vec<&PathBuf> = bag_before
            .iter()
            .filter(|path| *path != &PathBuf::from("b.jpg"))
            .collect();
        let survivors_after: Vec<&PathBuf> = bag_after
            .iter()
            .filter(|path| *path != &PathBuf::from("f.jpg"))
            .collect();
        assert_eq!(survivors_before, survivors_after);
        assert!(bag_after.contains(&PathBuf::from("f.jpg")));
    }
}
