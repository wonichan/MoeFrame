//! 布局接缝（V2 相框外观专题工单 02 / 03）：内置画框的三层绘制
//! 矩形、自绘软阴影的扩散层与自定义相框图的九宫格矩形都在这个
//! 纯函数上验证——喂窗口矩形 + 四边内距 + 框宽（或相框图分析
//! 结论），断言各绘制矩形；宿主只照单绘制，不另算几何。先行
//! 先例：宿主 next_frame_delay 的纯函数回归测试。

use moe_frame::{
    config::{AppearanceMode, BuiltinFrame, FramePreset},
    frame_art::{FrameArtAnalysis, HoleRect},
    frame_layout::{
        builtin_layers, builtin_shading, effective_appearance, nine_slice_layout, paint_plan,
        EffectiveAppearance, FrameEdge, FramePaint, HintPaint, LayoutRect, PaintPlan,
    },
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

/// 用户自己的内置参数（原木、框宽 30 / 卡纸宽 6，故意远离默认
/// 值）：回退口径的测试靠它区分「用用户参数」与「用默认参数」。
fn user_frame() -> BuiltinFrame {
    BuiltinFrame {
        preset: FramePreset::Oak,
        frame_width: 30.0,
        mat_width: 6.0,
    }
}

/// 经生效外观裁决再问绘制计划（宿主的实际问法）。
fn plan_for(mode: AppearanceMode, custom_active: bool, has_image: bool) -> PaintPlan {
    paint_plan(
        effective_appearance(mode, custom_active, user_frame()),
        has_image,
    )
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
    let plan = plan_for(AppearanceMode::Builtin, false, true);
    assert_eq!(plan.frame_paint, FramePaint::Builtin);
    assert!(!plan.panel_opaque);
}

#[test]
fn custom_active_paints_only_the_art_no_builtin_no_shadow() {
    // 异形（工单 04）的钉子：自定义生效时画框层只有九宫格相框
    // 图——计划里不存在内置层，自绘阴影因此无从混入；面板不
    // 铺底，相框图的透明像素（洞与不规则外轮廓）直接透出桌面。
    let plan = plan_for(AppearanceMode::Custom, true, true);
    assert_eq!(plan.frame_paint, FramePaint::CustomArt);
    assert!(!plan.panel_opaque);
}

#[test]
fn custom_inactive_falls_back_to_the_builtin_frame() {
    // 自定义未生效（未选图或分析回退）：回退即内置，画框层与
    // 内置模式逐项一致（含自绘阴影的资格）。
    let plan = plan_for(AppearanceMode::Custom, false, true);
    assert_eq!(plan.frame_paint, FramePaint::Builtin);
    assert!(!plan.panel_opaque);
}

#[test]
fn minimal_keeps_the_v1_opaque_panel_and_no_frame() {
    // 极简：不画任何画框层，面板铺黑底保持 V1 视觉。
    let plan = plan_for(AppearanceMode::Minimal, false, true);
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
        assert_eq!(plan_for(mode, true, true).hint, HintPaint::None);
    }
    // 自定义生效的空状态：框体（相框图）照画，提示浮在洞中央；
    // 背景是透出的桌面、明暗未知，浅字必须衬深色描影才恒可读。
    let plan = plan_for(AppearanceMode::Custom, true, false);
    assert_eq!(plan.frame_paint, FramePaint::CustomArt);
    assert_eq!(plan.hint, HintPaint::Floating { outline: true });
    // 极简空状态：提示浮在整窗中央，背景是自家黑底，不用描影。
    assert_eq!(
        plan_for(AppearanceMode::Minimal, false, false).hint,
        HintPaint::Floating { outline: false }
    );
    // 内置（含自定义回退）空状态：提示由内置绘制内部按卡纸
    // 在场与否选字色，不走浮排。
    assert_eq!(
        plan_for(AppearanceMode::Builtin, false, false).hint,
        HintPaint::InFrame
    );
    assert_eq!(
        plan_for(AppearanceMode::Custom, false, false).hint,
        HintPaint::InFrame
    );
}

#[test]
fn builtin_mode_resolves_with_the_users_frame() {
    // 内置模式：生效参数就是用户参数（单边内距 = 30 + 6）。
    let effective = effective_appearance(AppearanceMode::Builtin, false, user_frame());
    assert_eq!(effective, EffectiveAppearance::Builtin(user_frame()));
    assert_eq!(effective.insets(), uniform(36.0));
}

#[test]
fn custom_inactive_falls_back_to_the_default_frame_not_the_users() {
    // spec 定案：相框图未生效时回退内置「默认」画框——胡桃木 +
    // 默认宽（单边内距 = 14 + 18），不是用户当前的内置参数
    // （30 + 6）。回退是运行期状态：裁决只读用户参数、不改写它。
    let effective = effective_appearance(AppearanceMode::Custom, false, user_frame());
    assert_eq!(
        effective,
        EffectiveAppearance::Builtin(BuiltinFrame::default())
    );
    assert_eq!(effective.insets(), uniform(32.0));
}

#[test]
fn custom_active_and_minimal_resolve_directly() {
    // 自定义生效：画框层只有相框图，用户内置参数不参与裁决。
    assert_eq!(
        effective_appearance(AppearanceMode::Custom, true, user_frame()),
        EffectiveAppearance::CustomArt
    );
    // 极简：无画框层、内距全 0（V1 行为）。
    let minimal = effective_appearance(AppearanceMode::Minimal, false, user_frame());
    assert_eq!(minimal, EffectiveAppearance::Minimal);
    assert_eq!(minimal.insets(), Insets::ZERO);
}

/// 九宫格布局夹具：400×300 的相框图，洞 (100,50,200,200)，四边
/// 内距左 100 / 上 50 / 右 100 / 下 50。布局接缝的输入就是分析
/// 结论本身，故直接构造（洞检测本身在 frame_art 的测试里覆盖）。
fn art_analysis() -> FrameArtAnalysis {
    FrameArtAnalysis {
        art_size: [400, 300],
        hole: HoleRect {
            x: 100,
            y: 50,
            width: 200,
            height: 200,
        },
    }
}

#[test]
fn nine_slice_corners_scale_only_by_the_factor() {
    let layout = nine_slice_layout(&art_analysis(), LayoutRect::new(0.0, 0.0, 800.0, 600.0));
    assert_eq!(layout.factor, 2.0);
    // 四角目标尺寸 = 源尺寸 × 2，不随外框高度之外的因素变化。
    let (tl, tr, bl, br) = (
        layout.regions[0],
        layout.regions[2],
        layout.regions[5],
        layout.regions[7],
    );
    assert_eq!(tl.src, LayoutRect::new(0.0, 0.0, 100.0, 50.0));
    assert_eq!(tl.dst, LayoutRect::new(0.0, 0.0, 200.0, 100.0));
    assert_eq!(tr.src, LayoutRect::new(300.0, 0.0, 400.0, 50.0));
    assert_eq!(tr.dst, LayoutRect::new(600.0, 0.0, 800.0, 100.0));
    assert_eq!(bl.src, LayoutRect::new(0.0, 250.0, 100.0, 300.0));
    assert_eq!(bl.dst, LayoutRect::new(0.0, 500.0, 200.0, 600.0));
    assert_eq!(br.src, LayoutRect::new(300.0, 250.0, 400.0, 300.0));
    assert_eq!(br.dst, LayoutRect::new(600.0, 500.0, 800.0, 600.0));
    // 洞区目标矩形 = 外框减去缩放后的四边内距。
    assert_eq!(layout.hole_dst, LayoutRect::new(200.0, 100.0, 600.0, 500.0));
}

#[test]
fn nine_slice_edges_stretch_only_along_their_length() {
    let analysis = art_analysis();
    let base = nine_slice_layout(&analysis, LayoutRect::new(0.0, 0.0, 800.0, 600.0));
    // 外框只变高（宽不变 → 系数不变）：四角目标尺寸逐项不变；
    // 上边只变长不变厚，左边只变高不变宽。
    let tall = nine_slice_layout(&analysis, LayoutRect::new(0.0, 0.0, 800.0, 900.0));
    for idx in [0usize, 2, 5, 7] {
        assert_eq!(tall.regions[idx].dst.width(), base.regions[idx].dst.width());
        assert_eq!(
            tall.regions[idx].dst.height(),
            base.regions[idx].dst.height()
        );
    }
    let top = tall.regions[1];
    assert_eq!(top.src, LayoutRect::new(100.0, 0.0, 300.0, 50.0));
    assert_eq!(top.dst, LayoutRect::new(200.0, 0.0, 600.0, 100.0));
    let left = tall.regions[3];
    assert_eq!(left.src, LayoutRect::new(0.0, 50.0, 100.0, 250.0));
    assert_eq!(left.dst, LayoutRect::new(0.0, 100.0, 200.0, 800.0));
    // 外框只变宽：上边变长、左边变高都不发生（高不变），角仍按系数变。
    let wide = nine_slice_layout(&analysis, LayoutRect::new(0.0, 0.0, 1200.0, 600.0));
    assert_eq!(wide.factor, 3.0);
    assert_eq!(
        wide.regions[1].dst,
        LayoutRect::new(300.0, 0.0, 900.0, 150.0)
    );
    assert_eq!(
        wide.regions[3].dst,
        LayoutRect::new(0.0, 150.0, 300.0, 450.0)
    );
}

#[test]
fn shading_carries_the_same_three_layers() {
    // 着色计划只是三层之上的光影数据：三层矩形与 builtin_layers
    // 逐项一致，宿主换用着色计划不许漂移绘制口径。
    let shading = builtin_shading(window(), uniform(32.0), 14.0, 18.0);
    assert_eq!(
        shading.layers,
        builtin_layers(window(), uniform(32.0), 14.0)
    );
}

#[test]
fn shading_edges_tile_the_band_and_face_the_light() {
    // 四边矩形恰好铺满框带：上、下边通宽（含四角），左、右边夹
    // 在中间。光自左上：同一档位上边最亮、左边次之、右边再次、
    // 下边最暗；每条边沿带宽方向（外沿 → 内沿）的档位走向是
    // 画框截面：外沿略暗、中段提亮、凹槽压暗、内唇最亮。
    let shading = builtin_shading(window(), uniform(32.0), 14.0, 18.0);
    let edges = shading.edges;
    assert_eq!(edges[0].edge, FrameEdge::Top);
    assert_eq!(edges[0].rect, LayoutRect::new(0.0, 0.0, 480.0, 14.0));
    assert_eq!(edges[1].edge, FrameEdge::Left);
    assert_eq!(edges[1].rect, LayoutRect::new(0.0, 14.0, 14.0, 306.0));
    assert_eq!(edges[2].edge, FrameEdge::Bottom);
    assert_eq!(edges[2].rect, LayoutRect::new(0.0, 306.0, 480.0, 320.0));
    assert_eq!(edges[3].edge, FrameEdge::Right);
    assert_eq!(edges[3].rect, LayoutRect::new(466.0, 14.0, 480.0, 306.0));
    for (i, edge) in edges.iter().enumerate() {
        let fracs: Vec<f32> = edge.stops.iter().map(|stop| stop.0).collect();
        assert_eq!(fracs, vec![0.0, 0.55, 0.82, 1.0], "第 {i} 边分位");
        let tones: Vec<f32> = edge.stops.iter().map(|stop| stop.1).collect();
        assert!(
            tones[3] > tones[0] && tones[3] > tones[1],
            "第 {i} 边内唇最亮：{tones:?}"
        );
        assert!(
            tones[2] < tones[0] && tones[2] < tones[1],
            "第 {i} 边凹槽最暗：{tones:?}"
        );
    }
    for stop in 0..4 {
        let (top, left, bottom, right) = (
            edges[0].stops[stop].1,
            edges[1].stops[stop].1,
            edges[2].stops[stop].1,
            edges[3].stops[stop].1,
        );
        assert!(
            top > left && left > right && right > bottom,
            "档位 {stop} 方向光序：上 {top} 左 {left} 右 {right} 下 {bottom}"
        );
    }
}

#[test]
fn shading_profile_lines_ride_the_inner_side_of_the_band() {
    // 造型线（宿主以 1px 内侧描边落笔）：外沿线即窗口边，凹槽在
    // 内沿外 3px，内唇紧贴卡纸层边界（内沿外 1px 起）。
    let shading = builtin_shading(window(), uniform(32.0), 14.0, 18.0);
    let profile = shading.profile.expect("框宽 14 应有造型线");
    assert_eq!(profile.outer_rim, window());
    assert_eq!(
        profile.groove,
        Some(LayoutRect::new(11.0, 11.0, 469.0, 309.0))
    );
    assert_eq!(profile.lip, LayoutRect::new(13.0, 13.0, 467.0, 307.0));
    // 窄框放不下凹槽：框宽 4 时只剩外沿与内唇；框宽 1 与 0 时
    // 造型线整体缺席（带只剩渐变本身）。
    let narrow = builtin_shading(window(), uniform(22.0), 4.0, 18.0);
    let profile = narrow.profile.expect("框宽 4 应有造型线");
    assert_eq!(profile.groove, None);
    assert_eq!(profile.lip, LayoutRect::new(3.0, 3.0, 477.0, 317.0));
    assert!(builtin_shading(window(), uniform(19.0), 1.0, 18.0)
        .profile
        .is_none());
    assert!(builtin_shading(window(), uniform(18.0), 0.0, 18.0)
        .profile
        .is_none());
}

#[test]
fn mat_shading_hugs_the_mat_and_image_edges() {
    // 框影落在卡纸上、贴框体内沿向内淡出；斜切面是画面外扩
    // 2px 的一圈；卡纸投在画面上的影贴画面边界向内淡出，且
    // 必须比框影更淡（卡纸薄、框体厚）。
    let shading = builtin_shading(window(), uniform(32.0), 14.0, 18.0);
    let mat = shading.mat.expect("卡纸宽 18 应有卡纸着色");
    let cast = mat.cast_shadow.expect("有框带才有框影");
    assert_eq!(cast.rect, LayoutRect::new(14.0, 14.0, 466.0, 306.0));
    assert_eq!(cast.depth, 7.0);
    assert!(cast.max_alpha > 0);
    assert_eq!(mat.bevel, LayoutRect::new(30.0, 30.0, 450.0, 290.0));
    assert_eq!(mat.bevel_width, 2.0);
    assert_eq!(
        mat.image_shadow.rect,
        LayoutRect::new(32.0, 32.0, 448.0, 288.0)
    );
    assert_eq!(mat.image_shadow.depth, 5.0);
    assert!(mat.image_shadow.max_alpha > 0 && mat.image_shadow.max_alpha < cast.max_alpha);
}

#[test]
fn mat_shading_clamps_to_a_narrow_mat_and_vanishes_with_it() {
    // 卡纸只有 3px：框影深度钳到卡纸宽内（不许溢到画面上冒充
    // 卡纸的影），斜切面仍是 2px 且整圈落在卡纸层内。
    let shading = builtin_shading(window(), uniform(17.0), 14.0, 3.0);
    let mat = shading.mat.expect("卡纸宽 3 仍有卡纸着色");
    assert_eq!(mat.cast_shadow.map(|band| band.depth), Some(3.0));
    assert_eq!(mat.bevel_width, 2.0);
    assert_eq!(mat.bevel, LayoutRect::new(15.0, 15.0, 465.0, 305.0));
    // 卡纸宽 0：无卡纸着色（三层退化口径不变，画面层即卡纸层）。
    let none = builtin_shading(window(), uniform(14.0), 14.0, 0.0);
    assert!(none.mat.is_none());
    assert_eq!(none.layers.mat, none.layers.image);
}

#[test]
fn frameless_band_casts_no_shadow_on_the_mat() {
    // 框宽 0：卡纸直接贴窗口边，没有框体可投影——框影缺席；
    // 斜切面与画面影是卡纸自己的，照旧。
    let shading = builtin_shading(window(), uniform(18.0), 0.0, 18.0);
    let mat = shading.mat.expect("卡纸宽 18 应有卡纸着色");
    assert!(mat.cast_shadow.is_none());
    assert_eq!(mat.bevel, LayoutRect::new(16.0, 16.0, 464.0, 304.0));
}

#[test]
fn corners_carry_the_three_boundary_radii_and_centers() {
    // 圆角（圆角工单 01）：三个角边界各有独立定值——窗口外沿
    // 20、框体内沿 10、卡纸内沿 12（逻辑 px，原型选定 C）。
    // 圆心 = 边界矩形四角各向内收半径：顺序左上、右上、右下、
    // 左下。默认内距（框 14 + 卡纸 18）下定值不被夹紧。
    let shading = builtin_shading(window(), uniform(32.0), 14.0, 18.0);
    let corners = shading.corners;
    assert_eq!(corners.outer.radius, 20.0);
    assert_eq!(
        corners.outer.centers,
        [(20.0, 20.0), (460.0, 20.0), (460.0, 300.0), (20.0, 300.0)]
    );
    let frame_inner = corners.frame_inner.expect("框宽 14 应有框体内沿边界");
    assert_eq!(frame_inner.radius, 10.0);
    assert_eq!(
        frame_inner.centers,
        [(24.0, 24.0), (456.0, 24.0), (456.0, 296.0), (24.0, 296.0)]
    );
    assert_eq!(corners.image.radius, 12.0);
    assert_eq!(
        corners.image.centers,
        [(44.0, 44.0), (436.0, 44.0), (436.0, 276.0), (44.0, 276.0)]
    );
}

#[test]
fn corners_degenerate_with_zero_mat_and_zero_frame() {
    // 卡纸宽 = 0：画面边缘即框体内沿呈现，半径取卡纸内沿定值
    // 12（框带 14 夹得住）；框体内沿边界仍在（半径 10），只是
    // 与画面边缘同矩形——宿主跳过卡纸层、直接按画面边缘落笔。
    let shading = builtin_shading(window(), uniform(14.0), 14.0, 0.0);
    assert_eq!(shading.corners.image.radius, 12.0);
    assert_eq!(
        shading.corners.image.centers,
        [(26.0, 26.0), (454.0, 26.0), (454.0, 294.0), (26.0, 294.0)]
    );
    assert_eq!(
        shading.corners.frame_inner.map(|b| b.radius),
        Some(10.0),
        "框宽 14 时框体内沿边界应在场"
    );
    // 框宽 = 0：框体内沿边界不存在，外沿半径落到卡纸带上被
    // 卡纸宽 18 夹紧（定值 20 → 18）；画面边缘半径仍是 12。
    let shading = builtin_shading(window(), uniform(18.0), 0.0, 18.0);
    assert!(shading.corners.frame_inner.is_none());
    assert_eq!(shading.corners.outer.radius, 18.0);
    assert_eq!(
        shading.corners.outer.centers,
        [(18.0, 18.0), (462.0, 18.0), (462.0, 302.0), (18.0, 302.0)]
    );
    assert_eq!(shading.corners.image.radius, 12.0);
}

#[test]
fn corners_clamp_to_thin_bands_and_tiny_windows() {
    // 带厚小于定值（框 4 + 卡纸 2）：外沿被整幅内距 6 夹紧、
    // 框体内沿被框带 4 夹紧、画面边缘被卡纸带 2 夹紧；半径都
    // 不超过各自边界矩形的半宽 / 半高（弧不自交的前提）。
    let shading = builtin_shading(window(), uniform(6.0), 4.0, 2.0);
    let corners = shading.corners;
    assert_eq!(corners.outer.radius, 6.0);
    assert_eq!(corners.frame_inner.map(|b| b.radius), Some(4.0));
    assert_eq!(corners.image.radius, 2.0);
    assert_eq!(
        corners.image.centers,
        [(8.0, 8.0), (472.0, 8.0), (472.0, 312.0), (8.0, 312.0)]
    );
    // 极小窗口（高 36 < 内距 64）：外沿半径被窗口半高 18 夹紧；
    // 画面矩形已退化，画面边缘半径归 0 而不是负数或溢出。
    let tiny = builtin_shading(
        LayoutRect::new(0.0, 0.0, 480.0, 36.0),
        uniform(32.0),
        14.0,
        18.0,
    );
    assert_eq!(tiny.corners.outer.radius, 18.0);
    assert_eq!(tiny.corners.image.radius, 0.0);
}

#[test]
fn corner_companions_carry_their_arc_radii() {
    // 角部弧段配套（圆角工单 01）：带状要素在角部沿弧延续所需
    // 的半径全部由布局接缝给出，宿主不推导——造型线半径按其
    // 落笔矩形在框带中的分位在外沿 20 与框体内沿 10 之间插值
    // （凹槽在内沿外 3px、内唇在内沿外 1px）；框影贴卡纸外沿
    // （半径 = 框体内沿 10），画面影贴画面边缘（12），斜切面
    // 外沿 = 画面边缘半径 + 切面宽（12 + 2）。
    let shading = builtin_shading(window(), uniform(32.0), 14.0, 18.0);
    let profile = shading.profile.expect("框宽 14 应有造型线");
    assert_eq!(profile.outer_radius, 20.0);
    let groove_radius = profile.groove_radius.expect("框宽 14 应有凹槽半径");
    assert!(
        (groove_radius - 12.142_857).abs() < 0.001,
        "凹槽半径应在外沿与框体内沿之间按分位插值：{groove_radius}"
    );
    assert!(
        (profile.lip_radius - 10.714_286).abs() < 0.001,
        "内唇半径应在外沿与框体内沿之间按分位插值：{}",
        profile.lip_radius
    );
    let mat = shading.mat.expect("卡纸宽 18 应有卡纸着色");
    assert_eq!(mat.cast_shadow.map(|band| band.radius), Some(10.0));
    assert_eq!(mat.image_shadow.radius, 12.0);
    assert_eq!(mat.bevel_radius, 14.0);
    // 框宽 4（无凹槽）：凹槽半径随凹槽一同缺席；框体内沿半径
    // 被框带夹紧为 4，内唇半径在外沿 20 与框体内沿 4 之间按
    // 分位（内沿外 1px / 框宽 4）插值 = 8。
    let narrow = builtin_shading(window(), uniform(22.0), 4.0, 18.0);
    let profile = narrow.profile.expect("框宽 4 应有造型线");
    assert_eq!(profile.groove_radius, None);
    assert_eq!(profile.lip_radius, 8.0);
    // 窄卡纸（3px）：画面边缘半径被卡纸带夹紧为 3，斜切面外沿
    // 半径随之为 3 + 2；框影仍贴卡纸外沿（框体内沿半径 10）。
    let narrow_mat = builtin_shading(window(), uniform(17.0), 14.0, 3.0);
    let mat = narrow_mat.mat.expect("卡纸宽 3 应有卡纸着色");
    assert_eq!(mat.image_shadow.radius, 3.0);
    assert_eq!(mat.bevel_radius, 5.0);
    assert_eq!(mat.cast_shadow.map(|band| band.radius), Some(10.0));
}

#[test]
fn corner_strip_radii_come_from_the_seam() {
    // 截面条带的角部弧半径（圆角工单 01，测试决策「截面带在
    // 四角均有弧段对应物」）：12 条带在四角的弧中心半径由布
    // 局接缝逐带给出，宿主只照单描弧——以外沿圆心为心、按条
    // 带中心分位自外沿半径逐带递减框宽 / 12。默认内距（外沿
    // 20、框宽 14）：首带 20 − 14/24、尾带 20 − 14×23/24，
    // 整组严格递减、带间差恒为框宽 / 12。
    let shading = builtin_shading(window(), uniform(32.0), 14.0, 18.0);
    let radii = shading.corner_strip_radii;
    assert_eq!(radii.len(), 12);
    assert!(
        (radii[0] - (20.0 - 14.0 / 24.0)).abs() < 0.001,
        "首带弧半径应为外沿半径 − 框宽 / 24：{}",
        radii[0]
    );
    assert!(
        (radii[11] - (20.0 - 14.0 * 23.0 / 24.0)).abs() < 0.001,
        "尾带弧半径应为外沿半径 − 框宽 ×23/24：{}",
        radii[11]
    );
    for pair in radii.windows(2) {
        assert!(pair[0] > pair[1], "弧半径应自外沿向内沿严格递减");
        assert!(
            (pair[0] - pair[1] - 14.0 / 12.0).abs() < 0.001,
            "相邻带弧半径差应为框宽 / 12"
        );
    }
    // 框带（40）比外沿半径（20）还宽：带中心越过圆心的尾部
    // 各带半径钳到 0（角部无弧可描），整组单调不增、无负数。
    let wide = builtin_shading(window(), uniform(58.0), 40.0, 18.0);
    let radii = wide.corner_strip_radii;
    assert!(radii.iter().all(|r| *r >= 0.0), "弧半径不得为负");
    assert!(
        radii.windows(2).all(|pair| pair[0] >= pair[1]),
        "弧半径应单调不增"
    );
    assert!(radii[0] > 0.0, "首带仍应有弧");
    assert_eq!(radii[11], 0.0, "尾带弧半径应钳到 0");
    // 框宽 = 0：没有框带可描，弧半径整组等于外沿半径（被卡
    // 纸带夹紧为 18）——宿主按框宽缺席整组跳过，本数据不被
    // 消费，仍须是良定义的。
    let frameless = builtin_shading(window(), uniform(18.0), 0.0, 18.0);
    assert!(
        frameless.corner_strip_radii.iter().all(|r| *r == 18.0),
        "框宽 0 时弧半径应整组等于外沿半径"
    );
}
