//! 切换动画无方向组（工单 03）的纯计算接缝：百叶帘条带、墨晕
//! 扇形、像素块序与抖动、旧图 fit 矩形、墙钟进度。行为断言只经
//! 库 transition_ambient 的公开函数，不触宿主绘制。

use moe_frame::frame_layout::LayoutRect;
use moe_frame::transition_ambient::{
    blinds_strips, fit_rect, ink_fan, ink_ring_point, pixel_block, pixel_jitter, pixel_order,
    pixel_visible_count, transition_progress, BLINDS_STRIPS, INK_SEGMENTS, PIXEL_BLOCKS,
    PIXEL_JITTER_MS,
};

fn stage() -> LayoutRect {
    LayoutRect::new(10.0, 20.0, 810.0, 620.0) // 800x600
}

#[test]
fn wall_clock_progress_clamps_to_the_duration() {
    assert_eq!(transition_progress(0, 650), 0.0);
    assert_eq!(transition_progress(325, 650), 0.5);
    assert_eq!(transition_progress(650, 650), 1.0);
    // 掉帧跳进度：超过时长不欠账，直接满格。
    assert_eq!(transition_progress(900, 650), 1.0);
    assert_eq!(transition_progress(10, 0), 1.0);
}

#[test]
fn fit_rect_centers_the_old_image_without_stretching() {
    // 舞台 800x600（4:3），旧图 16:9：宽满、高按比例，上下居中。
    let fit = fit_rect(stage(), 16.0 / 9.0);
    assert!((fit.width() - 800.0).abs() < 0.01);
    assert!((fit.height() - 450.0).abs() < 0.01);
    assert!((fit.left - 10.0).abs() < 0.01);
    assert!((fit.top - (20.0 + 75.0)).abs() < 0.01);
    // 旧图 3:4（竖图）：高满、左右居中。
    let fit = fit_rect(stage(), 0.75);
    assert!((fit.height() - 600.0).abs() < 0.01);
    assert!((fit.width() - 450.0).abs() < 0.01);
    assert!((fit.left - (10.0 + 175.0)).abs() < 0.01);
    // 同比例：恰好铺满舞台。
    let fit = fit_rect(stage(), 4.0 / 3.0);
    assert!((fit.width() - 800.0).abs() < 0.01 && (fit.height() - 600.0).abs() < 0.01);
}

#[test]
fn blinds_reveal_nothing_at_zero_and_everything_at_one() {
    assert!(blinds_strips(stage(), 0.0).is_empty());
    let strips = blinds_strips(stage(), 1.0);
    assert_eq!(strips.len(), BLINDS_STRIPS);
    // 满格时 8 条恰好拼满舞台：横向无缝、纵向全高、UV 同步拼满。
    let mut x = stage().left;
    for strip in &strips {
        assert!((strip.dest.left - x).abs() < 0.01);
        assert!((strip.dest.top - stage().top).abs() < 0.01);
        assert!((strip.dest.height() - 600.0).abs() < 0.01);
        assert!((strip.uv[0] - (x - stage().left) / 800.0).abs() < 0.001);
        assert!((strip.uv[3] - 1.0).abs() < 0.001);
        x = strip.dest.right;
    }
    assert!((x - stage().right).abs() < 0.01);
}

#[test]
fn blinds_stagger_left_to_right_from_the_top() {
    // 中途：左边的条进度领先右边的条（错峰自左至右）。
    let strips = blinds_strips(stage(), 0.5);
    assert!(!strips.is_empty());
    let heights: Vec<f32> = strips.iter().map(|s| s.dest.height()).collect();
    for pair in heights.windows(2) {
        assert!(pair[0] >= pair[1], "左条应不落后于右条: {heights:?}");
    }
    assert!(heights[0] > heights[heights.len() - 1]);
    // 每条都从舞台顶部落下（上沿翻下）：dest 顶边贴舞台顶，
    // UV 的 v 起点为 0、v 终点与 dest 高度占比一致（同纹理子区域）。
    for strip in &strips {
        assert!((strip.dest.top - stage().top).abs() < 0.01);
        assert!((strip.uv[1]).abs() < 0.001);
        assert!((strip.uv[3] - strip.dest.height() / 600.0).abs() < 0.001);
        // 翻转感的明暗：未满格的条比满格暗。
        assert!(strip.shade >= 0.55 && strip.shade <= 1.0);
    }
    // 满格时全部 shade 回到 1（无残余压暗）。
    assert!(blinds_strips(stage(), 1.0)
        .iter()
        .all(|s| (s.shade - 1.0).abs() < 0.001));
}

#[test]
fn ink_fan_grows_from_the_stage_center_and_covers_the_corners() {
    let stage = stage();
    let fan = ink_fan(stage, 0.0);
    assert_eq!(fan.radius, 0.0);
    // 圆心在画面中心。
    assert!((fan.center[0] - 410.0).abs() < 0.01);
    assert!((fan.center[1] - 320.0).abs() < 0.01);
    // 半径单调增长，满格时不小于半对角线（四角都被晕开盖住）：
    // 半对角线 = sqrt(400^2 + 300^2) = 500。
    let mid = ink_fan(stage, 0.5);
    let full = ink_fan(stage, 1.0);
    assert!(mid.radius > 0.0 && mid.radius < full.radius);
    assert!(full.radius >= 500.0 - 0.01);
    // ease-out 手感：中途半径已过半程。
    assert!(mid.radius > 250.0);
}

#[test]
fn ink_veil_and_old_shade_follow_the_progress() {
    // 灰纱近似去色（用户批准定案）：进度越深纱越强、旧图越暗；
    // 进度 0 时两者都不动旧图。
    let start = ink_fan(stage(), 0.0);
    assert_eq!(start.veil_alpha, 0);
    assert!((start.old_shade - 1.0).abs() < 0.001);
    let mid = ink_fan(stage(), 0.5);
    let full = ink_fan(stage(), 1.0);
    assert!(mid.veil_alpha > 0 && mid.veil_alpha < full.veil_alpha);
    assert!(full.veil_alpha <= 160);
    assert!(mid.old_shade < 1.0 && mid.old_shade > full.old_shade);
    assert!(full.old_shade >= 0.5);
}

#[test]
fn ink_ring_points_lie_on_the_circle_with_stage_uvs() {
    let stage = stage();
    let fan = ink_fan(stage, 1.0);
    for i in 0..INK_SEGMENTS {
        let (pos, uv) = ink_ring_point(stage, &fan, i, 1.0);
        let dist = ((pos[0] - fan.center[0]).powi(2) + (pos[1] - fan.center[1]).powi(2)).sqrt();
        assert!((dist - fan.radius).abs() < 0.5, "环点应落在圆周上");
        // UV 是该点在舞台内的归一化坐标（单纹理取样）。
        assert!((uv[0] - (pos[0] - stage.left) / 800.0).abs() < 0.001);
        assert!((uv[1] - (pos[1] - stage.top) / 600.0).abs() < 0.001);
    }
    // 羽化外环同角度、同 UV，半径更大。
    let (inner, _) = ink_ring_point(stage, &fan, 0, 1.0);
    let (outer, _) = ink_ring_point(stage, &fan, 0, 1.12);
    let d_inner = (inner[0] - fan.center[0]).abs();
    let d_outer = (outer[0] - fan.center[0]).abs();
    assert!(d_outer > d_inner);
}

#[test]
fn pixel_order_is_a_full_permutation_fixed_by_the_seed() {
    let order = pixel_order(42);
    assert_eq!(order.len(), PIXEL_BLOCKS);
    let mut seen = [false; PIXEL_BLOCKS];
    for &block in &order {
        assert!((block as usize) < PIXEL_BLOCKS);
        assert!(!seen[block as usize], "块序不得重复");
        seen[block as usize] = true;
    }
    assert!(seen.iter().all(|&s| s), "块序须覆盖全部 96 块");
    // 同一种子逐帧可复现（每帧重洗会闪烁）；不同种子顺序不同
    // （每条动画重新洗）。
    assert_eq!(pixel_order(42), order);
    assert_ne!(pixel_order(43), order);
    // 不是顺序表：洗过牌。
    assert!(!order.iter().enumerate().all(|(i, &b)| b as usize == i));
}

#[test]
fn pixel_visible_count_grows_with_progress_to_full_coverage() {
    assert_eq!(pixel_visible_count(0.0), 0);
    assert_eq!(pixel_visible_count(1.0), PIXEL_BLOCKS);
    let mid = pixel_visible_count(0.5);
    assert!(mid > 0 && mid < PIXEL_BLOCKS);
    assert!(pixel_visible_count(0.25) <= mid);
    assert!(pixel_visible_count(0.75) >= mid);
}

#[test]
fn pixel_blocks_tile_the_stage_with_matching_uvs() {
    let stage = stage();
    // 四角与中心块：dest 与 UV 同步切分（同纹理子区域）。
    for index in [0usize, 11, 84, 95, 46] {
        let (dest, uv) = pixel_block(stage, index);
        let col = (index % 12) as f32;
        let row = (index / 12) as f32;
        assert!((dest.left - (stage.left + col * 800.0 / 12.0)).abs() < 0.01);
        assert!((dest.top - (stage.top + row * 600.0 / 8.0)).abs() < 0.01);
        assert!((dest.width() - 800.0 / 12.0).abs() < 0.01);
        assert!((dest.height() - 600.0 / 8.0).abs() < 0.01);
        assert!((uv[0] - col / 12.0).abs() < 0.001);
        assert!((uv[1] - row / 8.0).abs() < 0.001);
        assert!((uv[2] - (col + 1.0) / 12.0).abs() < 0.001);
        assert!((uv[3] - (row + 1.0) / 8.0).abs() < 0.001);
    }
}

#[test]
fn pixel_jitter_lives_only_in_the_opening_window() {
    // 抖动窗口 = 开头约 120ms（spec 冻结值），窗口内偏移有界。
    assert_eq!(PIXEL_JITTER_MS, 120);
    for elapsed in [0u64, 33, 66, 99, 119] {
        let offset = pixel_jitter(elapsed, 7);
        assert!(offset[0].abs() <= 5.0 && offset[1].abs() <= 5.0);
    }
    // 窗口一过立刻归零，后面全程不再抖。
    for elapsed in [120u64, 200, 549] {
        assert_eq!(pixel_jitter(elapsed, 7), [0.0, 0.0]);
    }
    // 窗口内随帧变化（是抖不是静态偏移）：四帧里至少两种偏移。
    let frames: Vec<[f32; 2]> = [0u64, 33, 66, 99]
        .iter()
        .map(|&e| pixel_jitter(e, 7))
        .collect();
    assert!(frames.windows(2).any(|pair| pair[0] != pair[1]));
}
