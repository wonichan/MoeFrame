//! 工单 11：播放序列纯逻辑测试——顺序、循环、轮转候选、空与单图。

use moe_frame::playlist::{Direction, Playlist};
use std::path::PathBuf;

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

#[test]
fn empty_playlist_has_no_current_and_no_candidates() {
    let list = Playlist::new(Vec::new());
    assert_eq!(list.current_path(), None);
    assert_eq!(list.next_path(), None);
    assert!(list.rotation_candidates(Direction::Next).is_empty());
    assert!(list.rotation_candidates(Direction::Prev).is_empty());
}

#[test]
fn single_image_never_rotates() {
    let mut list = Playlist::new(paths(&["a.jpg"]));
    assert!(list.set_current(&PathBuf::from("a.jpg")));
    // 单图：没有「下一张」，导航与预加载都无目标，避免自转忙循环。
    assert_eq!(list.next_path(), None);
    assert!(list.rotation_candidates(Direction::Next).is_empty());
    assert!(list.rotation_candidates(Direction::Prev).is_empty());
}

#[test]
fn next_wraps_from_tail_to_head() {
    let mut list = Playlist::new(paths(&["a.jpg", "b.jpg", "c.jpg"]));
    assert!(list.set_current(&PathBuf::from("c.jpg")));
    assert_eq!(list.next_path(), Some(PathBuf::from("a.jpg")));
    assert_eq!(
        list.rotation_candidates(Direction::Next),
        paths(&["a.jpg", "b.jpg"])
    );
}

#[test]
fn rotation_candidates_follow_direction_and_exclude_current() {
    let mut list = Playlist::new(paths(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]));
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
    let mut list = Playlist::new(paths(&["a.jpg", "b.jpg", "c.jpg"]));
    assert!(list.set_current(&PathBuf::from("a.jpg")));
    // 加载时 b 坏了跳到 c：当前位置按实际显示路径落位。
    assert!(list.set_current(&PathBuf::from("c.jpg")));
    assert_eq!(list.current_path(), Some(&PathBuf::from("c.jpg")));
    assert!(!list.set_current(&PathBuf::from("zzz.jpg")));
    assert_eq!(list.current_path(), Some(&PathBuf::from("c.jpg")));
}

#[test]
fn no_current_means_no_rotation() {
    let list = Playlist::new(paths(&["a.jpg", "b.jpg"]));
    assert_eq!(list.next_path(), None);
    assert!(list.rotation_candidates(Direction::Next).is_empty());
}
