//! 工单 20：宽高比锁定尺寸修正与离屏位置钳制（库层纯函数）。
use moe_frame::window_geom::{aspect_locked_size, offscreen_position};

fn assert_size(actual: [f32; 2], expected: [f32; 2]) {
    assert!(
        (actual[0] - expected[0]).abs() < 0.51 && (actual[1] - expected[1]).abs() < 0.51,
        "期望 {expected:?}，实际 {actual:?}"
    );
}

#[test]
fn a_width_driven_resize_derives_the_height() {
    // 480×320（比例 1.5）拉宽到 600：高随到 400。
    assert_size(
        aspect_locked_size([480.0, 320.0], [600.0, 320.0], 1.5),
        [600.0, 400.0],
    );
}

#[test]
fn a_height_driven_resize_derives_the_width() {
    assert_size(
        aspect_locked_size([480.0, 320.0], [480.0, 400.0], 1.5),
        [600.0, 400.0],
    );
}

#[test]
fn a_corner_resize_prefers_the_width() {
    assert_size(
        aspect_locked_size([480.0, 320.0], [600.0, 500.0], 1.5),
        [600.0, 400.0],
    );
}

#[test]
fn the_minimum_side_clamps_and_rederives() {
    // 宽触底 120：按比例高应为 80，又触底 → 高 120、宽反推 180。
    assert_size(
        aspect_locked_size([480.0, 320.0], [100.0, 320.0], 1.5),
        [180.0, 120.0],
    );
    // 竖长比例 0.5：宽触底 120 → 高 240，都不触底。
    assert_size(
        aspect_locked_size([160.0, 320.0], [100.0, 320.0], 0.5),
        [120.0, 240.0],
    );
}

#[test]
fn an_invalid_aspect_passes_the_size_through() {
    assert_size(
        aspect_locked_size([480.0, 320.0], [600.0, 500.0], 0.0),
        [600.0, 500.0],
    );
}

#[test]
fn a_fully_offscreen_position_moves_to_the_primary_bottom_right() {
    // 副屏拔掉后 x=5000 落在主屏（1920×1080）之外：回主屏右下内缩 24。
    assert_eq!(
        offscreen_position([5000.0, 100.0], [480.0, 320.0], [1920.0, 1080.0]),
        Some([1416.0, 736.0])
    );
    // 完全在主屏上方同样算离屏。
    assert_eq!(
        offscreen_position([100.0, -900.0], [480.0, 320.0], [1920.0, 1080.0]),
        Some([1416.0, 736.0])
    );
}

#[test]
fn a_partially_visible_position_is_left_alone() {
    assert_eq!(
        offscreen_position([-100.0, 50.0], [480.0, 320.0], [1920.0, 1080.0]),
        None
    );
    assert_eq!(
        offscreen_position([100.0, 100.0], [480.0, 320.0], [1920.0, 1080.0]),
        None
    );
}
