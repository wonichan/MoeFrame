//! 布局接缝（V2 相框外观专题工单 02）：内置画框的三层绘制矩形与
//! 自绘软阴影的扩散层都在这个纯函数上验证——喂窗口矩形 + 四边
//! 内距 + 框宽，断言框体 / 卡纸 / 画面三层矩形与阴影层序；宿主
//! 只照单绘制，不另算几何。先行先例：宿主 next_frame_delay 的
//! 纯函数回归测试。

use moe_frame::{
    config::AppearanceMode,
    frame_layout::{builtin_layers, paint_plan, shadow_layers, FramePaint, HintPaint, LayoutRect},
    window_geom::Insets,
};

fn window() -> LayoutRect {
    LayoutRect::new(0.0, 0.0, 480.0, 320.0)
}

fn uniform(side: f32) -> Insets {
    Insets {
        left: side,
        top: side,
        right: side,
        bottom: side,
    }
}

#[test]
fn three_layers_nest_by_frame_and_mat_widths() {
    // 框宽 14 + 卡纸宽 18 = 单边内距 32：卡纸层从框体内侧起，
    // 画面层从卡纸内侧起，三层逐层内收。
    let layers = builtin_layers(window(), uniform(32.0), 14.0);
    assert_eq!(layers.body, window());
    assert_eq!(layers.mat, LayoutRect::new(14.0, 14.0, 466.0, 306.0));
    assert_eq!(layers.image, LayoutRect::new(32.0, 32.0, 448.0, 288.0));
}

#[test]
fn zero_mat_width_degenerates_to_two_layers() {
    // 卡纸宽 0：单边内距 = 框宽，画面层与卡纸层重合——三层退化
    // 为框体 + 画面两层，宿主跳过卡纸填充即可。
    let layers = builtin_layers(window(), uniform(14.0), 14.0);
    assert_eq!(layers.mat, LayoutRect::new(14.0, 14.0, 466.0, 306.0));
    assert_eq!(layers.image, layers.mat);
}

#[test]
fn unequal_layer_widths_stay_independent() {
    // 框宽 30、卡纸宽 4：两层厚度互不牵连，画面内收 34。
    let layers = builtin_layers(window(), uniform(34.0), 30.0);
    assert_eq!(layers.mat, LayoutRect::new(30.0, 30.0, 450.0, 290.0));
    assert_eq!(layers.image, LayoutRect::new(34.0, 34.0, 446.0, 286.0));
}

#[test]
fn zero_frame_width_puts_the_mat_layer_on_the_window_edge() {
    // 框宽 0：卡纸层即窗口本身，画面只内收卡纸宽。
    let layers = builtin_layers(window(), uniform(18.0), 0.0);
    assert_eq!(layers.mat, window());
    assert_eq!(layers.image, LayoutRect::new(18.0, 18.0, 462.0, 302.0));
}

#[test]
fn per_side_insets_place_the_image_area_exactly() {
    // 四边内距不等（拍立得式宽下边）：画面层逐边按内距内收，
    // 与几何状态机的画面区域口径一致；卡纸层仍按框宽均匀内收。
    let insets = Insets {
        left: 20.0,
        top: 10.0,
        right: 20.0,
        bottom: 60.0,
    };
    let layers = builtin_layers(window(), insets, 6.0);
    assert_eq!(layers.image, LayoutRect::new(20.0, 10.0, 460.0, 260.0));
    assert_eq!(layers.mat, LayoutRect::new(6.0, 6.0, 474.0, 314.0));
}

#[test]
fn builtin_mode_paints_the_builtin_frame_without_panel_fill() {
    // 内置模式：画框层是内置三层（自绘阴影只随这一层出现），
    // 面板不铺底——框体层自铺满整个窗口。
    let plan = paint_plan(AppearanceMode::Builtin, false, true);
    assert_eq!(plan.frame_paint, FramePaint::Builtin);
    assert!(!plan.panel_opaque);
}

#[test]
fn custom_active_paints_only_the_art_no_builtin_no_shadow() {
    // 异形（工单 04）的钉子：自定义生效时画框层只有九宫格相框
    // 图——计划里不存在内置层，自绘阴影因此无从混入；面板不
    // 铺底，相框图的透明像素（洞与不规则外轮廓）直接透出桌面。
    let plan = paint_plan(AppearanceMode::Custom, true, true);
    assert_eq!(plan.frame_paint, FramePaint::CustomArt);
    assert!(!plan.panel_opaque);
}

#[test]
fn custom_inactive_falls_back_to_the_builtin_frame() {
    // 自定义未生效（未选图或分析回退）：回退即内置，画框层与
    // 内置模式逐项一致（含自绘阴影的资格）。
    let plan = paint_plan(AppearanceMode::Custom, false, true);
    assert_eq!(plan.frame_paint, FramePaint::Builtin);
    assert!(!plan.panel_opaque);
}

#[test]
fn minimal_keeps_the_v1_opaque_panel_and_no_frame() {
    // 极简：不画任何画框层，面板铺黑底保持 V1 视觉。
    let plan = paint_plan(AppearanceMode::Minimal, false, true);
    assert_eq!(plan.frame_paint, FramePaint::None);
    assert!(plan.panel_opaque);
}

#[test]
fn empty_state_hint_placement_follows_the_frame_layer() {
    // 有图时任何模式都不画提示。
    for mode in [
        AppearanceMode::Minimal,
        AppearanceMode::Builtin,
        AppearanceMode::Custom,
    ] {
        assert_eq!(paint_plan(mode, true, true).hint, HintPaint::None);
    }
    // 自定义生效的空状态：框体（相框图）照画，提示浮在洞中央；
    // 背景是透出的桌面、明暗未知，浅字必须衬深色描影才恒可读。
    let plan = paint_plan(AppearanceMode::Custom, true, false);
    assert_eq!(plan.frame_paint, FramePaint::CustomArt);
    assert_eq!(plan.hint, HintPaint::Floating { outline: true });
    // 极简空状态：提示浮在整窗中央，背景是自家黑底，不用描影。
    assert_eq!(
        paint_plan(AppearanceMode::Minimal, false, false).hint,
        HintPaint::Floating { outline: false }
    );
    // 内置（含自定义回退）空状态：提示由内置绘制内部按卡纸
    // 在场与否选字色，不走浮排。
    assert_eq!(
        paint_plan(AppearanceMode::Builtin, false, false).hint,
        HintPaint::InFrame
    );
    assert_eq!(
        paint_plan(AppearanceMode::Custom, false, false).hint,
        HintPaint::InFrame
    );
}

#[test]
fn shadow_layers_diffuse_inward_from_the_window_edge() {
    // 自绘软阴影：多层矩形从窗口边缘逐层内收，透明度逐层递减，
    // 宿主按返回顺序逐层描画即得扩散效果。
    let layers = shadow_layers(window());
    assert!(layers.len() >= 4, "阴影至少四层扩散：{layers:?}");
    assert_eq!(layers[0].0, window(), "最外层贴窗口边缘");
    for pair in layers.windows(2) {
        let (outer_rect, outer_alpha) = pair[0];
        let (inner_rect, inner_alpha) = pair[1];
        assert!(
            inner_rect.left > outer_rect.left
                && inner_rect.top > outer_rect.top
                && inner_rect.right < outer_rect.right
                && inner_rect.bottom < outer_rect.bottom,
            "阴影层逐层严格内收：{outer_rect:?} -> {inner_rect:?}"
        );
        assert!(
            inner_alpha < outer_alpha,
            "阴影透明度逐层递减：{outer_alpha} -> {inner_alpha}"
        );
    }
}
