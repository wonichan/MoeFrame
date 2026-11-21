//! 显示数据管线（工单 01，architecture-deepening）：记账、转正、过期判定
//! 与重载决策只在 display_data 一处，测试直接驱动它的 interface——喂加载
//! 回送与每帧观察，断言它给出的裁决与刷新命令。

use moe_frame::{
    decode_validation::DisplayMode,
    display_data::{DisplayKey, DisplayPipeline, NavDecision, Observation, RefreshDecision},
};
use std::path::PathBuf;

fn img(name: &str) -> PathBuf {
    PathBuf::from(format!(r"D:\图库\{name}"))
}

fn key(name: &str, target: [u32; 2], mode: DisplayMode) -> DisplayKey {
    DisplayKey::new(img(name), target, mode)
}

fn obs(showing: Option<&str>, mode: DisplayMode, target: [u32; 2], now_secs: f64) -> Observation {
    Observation {
        showing: showing.map(img),
        fit_mode: mode,
        target,
        slide_active: false,
        now_secs,
    }
}

/// 走完一次「发起加载 → 回送成功 → 直接转正」，当前槽记为指定口径。
fn showing_pipeline(name: &str, mode: DisplayMode, target: [u32; 2]) -> DisplayPipeline {
    let mut pipeline = DisplayPipeline::new();
    let seq = pipeline.begin_load(mode, target);
    assert_eq!(
        pipeline.nav_reply(seq, Some(&img(name))),
        NavDecision::Accept
    );
    pipeline.promote_incoming();
    pipeline
}

#[test]
fn first_load_promotes_to_current_with_its_load_key() {
    let pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    assert_eq!(
        pipeline.current_key(),
        Some(&key("a.jpg", [800, 600], DisplayMode::Fit))
    );
    assert_eq!(pipeline.incoming_key(), None);
}

#[test]
fn stale_seq_reply_is_discarded_and_records_nothing() {
    let mut pipeline = DisplayPipeline::new();
    let seq = pipeline.begin_load(DisplayMode::Fit, [800, 600]);
    pipeline.invalidate();
    assert_eq!(
        pipeline.nav_reply(seq, Some(&img("a.jpg"))),
        NavDecision::Stale
    );
    assert_eq!(pipeline.incoming_key(), None);
    assert_eq!(pipeline.current_key(), None);
}

#[test]
fn failed_nav_reply_clears_in_flight_and_keeps_current() {
    let mut pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    let seq = pipeline.begin_load(DisplayMode::Fit, [800, 600]);
    assert_eq!(pipeline.nav_reply(seq, None), NavDecision::Failed);
    assert_eq!(
        pipeline.current_key(),
        Some(&key("a.jpg", [800, 600], DisplayMode::Fit))
    );
    // 在飞已清：同一序号的迟到回送按过期处理。
    assert_eq!(
        pipeline.nav_reply(seq, Some(&img("b.jpg"))),
        NavDecision::Stale
    );
}

#[test]
fn mode_change_fires_refresh_immediately_and_swap_updates_current() {
    let mut pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    let fired = pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.0));
    assert_eq!(fired, Some(key("a.jpg", [800, 600], DisplayMode::Fill)));
    // 待决刷新在飞：不再重复触发。
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.1)),
        None
    );
    assert_eq!(
        pipeline.refresh_reply(Some(&img("a.jpg")), Some(&img("a.jpg")), false),
        RefreshDecision::Swap
    );
    assert_eq!(
        pipeline.current_key(),
        Some(&key("a.jpg", [800, 600], DisplayMode::Fill))
    );
    // 口径已一致：不再触发。
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.2)),
        None
    );
}

#[test]
fn target_only_change_waits_for_settle_and_restarts_on_wobble() {
    let mut pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    // 目标刚变：先记候选，不触发。
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fit, [1024, 768], 1.0)),
        None
    );
    // 未满 250ms：仍不触发。
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fit, [1024, 768], 1.2)),
        None
    );
    // 途中又变：重新计时。
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fit, [640, 480], 1.3)),
        None
    );
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fit, [640, 480], 1.5)),
        None
    );
    // 停稳满 250ms：触发，键是停稳后的目标。
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fit, [640, 480], 1.56)),
        Some(key("a.jpg", [640, 480], DisplayMode::Fit))
    );
}

#[test]
fn failed_refresh_keeps_current_and_suppresses_same_key_retry() {
    let mut pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    assert!(pipeline
        .tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.0))
        .is_some());
    assert_eq!(
        pipeline.refresh_reply(None, Some(&img("a.jpg")), false),
        RefreshDecision::Discard
    );
    // 旧纹理保留，口径记录不变。
    assert_eq!(
        pipeline.current_key(),
        Some(&key("a.jpg", [800, 600], DisplayMode::Fit))
    );
    // 同一期望口径不再重试（定案策略：不重试，等口径变化或导航换图）。
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 2.0)),
        None
    );
    // 期望口径再变（模式仍为填充、目标也变）：是新的期望键，不受旧失败
    // 压制；模式过期不走目标防抖，立即触发。
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fill, [1024, 768], 2.1)),
        Some(key("a.jpg", [1024, 768], DisplayMode::Fill))
    );
}

#[test]
fn refresh_reply_after_view_moved_on_is_discarded() {
    let mut pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    assert!(pipeline
        .tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.0))
        .is_some());
    // 画面已切到 b.jpg：a 的刷新数据过期，丢弃且不记失败（不是失败，是过期）。
    assert_eq!(
        pipeline.refresh_reply(Some(&img("a.jpg")), Some(&img("b.jpg")), false),
        RefreshDecision::Discard
    );
    assert_eq!(
        pipeline.current_key(),
        Some(&key("a.jpg", [800, 600], DisplayMode::Fit))
    );
}

#[test]
fn in_flight_nav_load_with_desired_key_suppresses_refresh() {
    let mut pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    // 核心已按新口径发出导航加载：其转正会刷新当前槽记录，不重复重载。
    let seq = pipeline.begin_load(DisplayMode::Fill, [800, 600]);
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.0)),
        None
    );
    assert_eq!(
        pipeline.nav_reply(seq, Some(&img("b.jpg"))),
        NavDecision::Accept
    );
    pipeline.promote_incoming();
    assert_eq!(
        pipeline.current_key(),
        Some(&key("b.jpg", [800, 600], DisplayMode::Fill))
    );
    assert_eq!(
        pipeline.tick(&obs(Some("b.jpg"), DisplayMode::Fill, [800, 600], 1.1)),
        None
    );
}

#[test]
fn slide_active_and_no_showing_block_refresh() {
    let mut pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    let mut sliding = obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.0);
    sliding.slide_active = true;
    assert_eq!(pipeline.tick(&sliding), None);
    assert_eq!(
        pipeline.tick(&obs(None, DisplayMode::Fill, [800, 600], 1.0)),
        None
    );
}

#[test]
fn invalidate_drops_pending_refresh_and_its_late_reply() {
    let mut pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    assert!(pipeline
        .tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.0))
        .is_some());
    pipeline.invalidate();
    assert_eq!(
        pipeline.refresh_reply(Some(&img("a.jpg")), Some(&img("a.jpg")), false),
        RefreshDecision::Discard
    );
}

#[test]
fn refresh_reply_during_slide_is_discarded_without_failure_memory() {
    let mut pipeline = showing_pipeline("a.jpg", DisplayMode::Fit, [800, 600]);
    assert!(pipeline
        .tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.0))
        .is_some());
    // 转场中到达：丢弃，但不记失败——转正后按新口径重新评估，
    // 模式仍过期时应能再次触发。
    assert_eq!(
        pipeline.refresh_reply(Some(&img("a.jpg")), Some(&img("a.jpg")), true),
        RefreshDecision::Discard
    );
    assert_eq!(
        pipeline.tick(&obs(Some("a.jpg"), DisplayMode::Fill, [800, 600], 1.1)),
        Some(key("a.jpg", [800, 600], DisplayMode::Fill))
    );
}

#[test]
fn display_key_identity_is_path_target_and_mode() {
    assert_eq!(
        key("a.jpg", [800, 600], DisplayMode::Fit),
        key("a.jpg", [800, 600], DisplayMode::Fit)
    );
    assert_ne!(
        key("a.jpg", [800, 600], DisplayMode::Fit),
        key("a.jpg", [800, 600], DisplayMode::Fill)
    );
    assert_ne!(
        key("a.jpg", [800, 600], DisplayMode::Fit),
        key("a.jpg", [1024, 768], DisplayMode::Fit)
    );
    assert_ne!(
        key("a.jpg", [800, 600], DisplayMode::Fit),
        key("b.jpg", [800, 600], DisplayMode::Fit)
    );
}
