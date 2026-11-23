//! 工单 18：显示几何——适应完整放入、填充覆盖居中（尺寸层面），横竖图都验。
use moe_frame::display::display_size;
use moe_frame::loading::DisplayMode;

const AREA: [f32; 2] = [480.0, 320.0];

fn assert_size(actual: [f32; 2], expected: [f32; 2]) {
    assert!(
        (actual[0] - expected[0]).abs() < 0.01 && (actual[1] - expected[1]).abs() < 0.01,
        "期望 {expected:?}，实际 {actual:?}"
    );
}

#[test]
fn fit_puts_a_landscape_image_inside_with_letterbox() {
    // 横图 1000×500 适应 480×320：宽满、高 240，上下留黑边。
    assert_size(
        display_size(DisplayMode::Fit, AREA, [1000.0, 500.0]),
        [480.0, 240.0],
    );
}

#[test]
fn fit_puts_a_portrait_image_inside_with_side_bars() {
    // 竖图 500×1000 适应：高满 320、宽 160，左右留黑边，不变形。
    assert_size(
        display_size(DisplayMode::Fit, AREA, [500.0, 1000.0]),
        [160.0, 320.0],
    );
}

#[test]
fn fill_covers_the_area_with_a_landscape_image() {
    // 横图填充：高满 320、宽 640，横向溢出待居中裁剪。
    assert_size(
        display_size(DisplayMode::Fill, AREA, [1000.0, 500.0]),
        [640.0, 320.0],
    );
}

#[test]
fn fill_covers_the_area_with_a_portrait_image() {
    // 竖图填充：宽满 480、高 960，纵向溢出待居中裁剪。
    assert_size(
        display_size(DisplayMode::Fill, AREA, [500.0, 1000.0]),
        [480.0, 960.0],
    );
}

#[test]
fn an_exact_ratio_image_is_identical_in_both_modes() {
    assert_size(
        display_size(DisplayMode::Fit, AREA, [960.0, 640.0]),
        [480.0, 320.0],
    );
    assert_size(
        display_size(DisplayMode::Fill, AREA, [960.0, 640.0]),
        [480.0, 320.0],
    );
}

#[test]
fn degenerate_sizes_fall_back_to_the_area() {
    assert_size(display_size(DisplayMode::Fit, AREA, [0.0, 0.0]), AREA);
    assert_size(
        display_size(DisplayMode::Fill, [0.0, 320.0], [100.0, 100.0]),
        [0.0, 320.0],
    );
}
