//! 工单 10/11：应用行为入口测试——通过意图/效果接缝驱动相框核心，不触碰 GUI 与线程。
//! 时间通过意图携带的 now_ms 可控推进，不用真实等待。

use moe_frame::app_core::{Effect, FrameCore, Intent, View};
use std::path::PathBuf;

fn path(text: &str) -> PathBuf {
    PathBuf::from(text)
}

fn img(name: &str) -> PathBuf {
    PathBuf::from(format!("D:\\图库\\{name}"))
}

/// 驱动核心到「已显示第一张图」的状态（t=0 完成加载），返回核心与首图加载产生的效果。
fn showing_core(names: &[&str], interval_secs: i64) -> (FrameCore, Vec<Effect>) {
    let mut core = FrameCore::new(Some(path("D:\\图库")), interval_secs);
    core.handle(Intent::Started);
    let list: Vec<PathBuf> = names.iter().map(|name| img(name)).collect();
    core.handle(Intent::ScanCompleted(list));
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img(names[0])),
        now_ms: 0,
    });
    (core, effects)
}

// ---------- 工单 10：首次启动与第一张图 ----------

#[test]
fn first_run_without_a_saved_folder_prompts_once_and_cancel_stays_empty() {
    let mut core = FrameCore::new(None, 30);
    assert_eq!(core.handle(Intent::Started), vec![Effect::PickFolder]);
    assert_eq!(core.handle(Intent::FolderPickCancelled), vec![]);
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn picking_a_folder_saves_scans_loads_and_shows_the_first_image() {
    let mut core = FrameCore::new(None, 30);
    core.handle(Intent::Started);
    let folder = path("D:\\图库");
    assert_eq!(
        core.handle(Intent::FolderPicked(folder.clone())),
        vec![Effect::SaveFolder(folder.clone()), Effect::Scan(folder)]
    );
    let candidates = vec![img("a.png"), img("b.png")];
    assert_eq!(
        core.handle(Intent::ScanCompleted(candidates.clone())),
        vec![Effect::Load { candidates }]
    );
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("a.png")),
            now_ms: 0,
        }),
        vec![Effect::Preload { path: img("b.png") }]
    );
    assert_eq!(core.view(), View::Showing(img("a.png")));
}

#[test]
fn a_saved_folder_skips_the_prompt_on_the_next_launch() {
    let mut core = FrameCore::new(Some(path("D:\\图库")), 30);
    assert_eq!(
        core.handle(Intent::Started),
        vec![Effect::Scan(path("D:\\图库"))]
    );
}

#[test]
fn an_empty_scan_enters_the_empty_state_without_loading() {
    let mut core = FrameCore::new(Some(path("D:\\空文件夹")), 30);
    core.handle(Intent::Started);
    assert_eq!(core.handle(Intent::ScanCompleted(vec![])), vec![]);
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn when_every_candidate_fails_the_frame_stays_operable_and_empty() {
    let mut core = FrameCore::new(Some(path("D:\\坏图库")), 30);
    core.handle(Intent::Started);
    core.handle(Intent::ScanCompleted(vec![path("D:\\坏图库\\x.png")]));
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: None,
            now_ms: 0
        }),
        vec![]
    );
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn cancelling_a_menu_repick_keeps_the_current_image() {
    let (mut core, _) = showing_core(&["a.png"], 30);
    assert_eq!(
        core.handle(Intent::FolderPickRequested),
        vec![Effect::PickFolder]
    );
    assert_eq!(core.handle(Intent::FolderPickCancelled), vec![]);
    assert_eq!(core.view(), View::Showing(img("a.png")));
    assert_eq!(core.folder(), Some(&path("D:\\图库")));
}

#[test]
fn exit_is_an_explicit_effect() {
    let mut core = FrameCore::new(None, 30);
    assert_eq!(core.handle(Intent::ExitRequested), vec![Effect::Exit]);
}

// ---------- 工单 11：顺序浏览与定时轮播 ----------

#[test]
fn auto_advance_fires_exactly_at_the_deadline() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    assert_eq!(core.handle(Intent::Tick { now_ms: 29_999 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 30_000 }),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 30_000,
        }),
        vec![Effect::Preload { path: img("c.png") }]
    );
    assert_eq!(core.handle(Intent::Tick { now_ms: 59_999 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 60_000 }),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("a.png")],
        }]
    );
}

#[test]
fn manual_next_recounts_a_full_interval_from_the_successful_switch() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 12_000,
    });
    assert_eq!(core.handle(Intent::Tick { now_ms: 41_999 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 42_000 }),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("a.png")],
        }]
    );
}

#[test]
fn prev_wraps_around_with_rotation_candidates() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    assert_eq!(
        core.handle(Intent::PrevRequested),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("b.png")],
        }]
    );
}

#[test]
fn pause_stops_auto_advance_but_not_manual_navigation_and_resume_recounts() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    assert_eq!(
        core.handle(Intent::PlayPauseToggled { now_ms: 5_000 }),
        vec![]
    );
    assert!(!core.is_playing());
    assert_eq!(core.handle(Intent::Tick { now_ms: 999_999 }), vec![]);
    // 暂停中手动切换照常，成功后仍预加载，但不恢复计时。
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 20_000,
        }),
        vec![Effect::Preload { path: img("c.png") }]
    );
    assert_eq!(core.handle(Intent::Tick { now_ms: 999_999 }), vec![]);
    // 恢复播放：从恢复时刻计满一个间隔。
    assert_eq!(
        core.handle(Intent::PlayPauseToggled { now_ms: 50_000 }),
        vec![]
    );
    assert!(core.is_playing());
    assert_eq!(core.handle(Intent::Tick { now_ms: 79_999 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 80_000 }),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("a.png")],
        }]
    );
}

#[test]
fn changing_the_interval_saves_and_recounts_but_the_same_value_does_nothing() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    assert_eq!(
        core.handle(Intent::IntervalSelected {
            secs: 60,
            now_ms: 10_000,
        }),
        vec![Effect::SaveInterval(60)]
    );
    assert_eq!(core.interval_secs(), 60);
    // 相同值：不重计、不重写。
    assert_eq!(
        core.handle(Intent::IntervalSelected {
            secs: 60,
            now_ms: 20_000,
        }),
        vec![]
    );
    assert_eq!(core.handle(Intent::Tick { now_ms: 69_999 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 70_000 }),
        vec![Effect::Load {
            candidates: vec![img("b.png")],
        }]
    );
}

#[test]
fn changing_the_interval_while_paused_only_records_it() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    core.handle(Intent::PlayPauseToggled { now_ms: 1_000 });
    assert_eq!(
        core.handle(Intent::IntervalSelected {
            secs: 10,
            now_ms: 2_000,
        }),
        vec![Effect::SaveInterval(10)]
    );
    assert_eq!(core.handle(Intent::Tick { now_ms: 999_999 }), vec![]);
    core.handle(Intent::PlayPauseToggled { now_ms: 5_000 });
    assert_eq!(core.handle(Intent::Tick { now_ms: 14_999 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 15_000 }),
        vec![Effect::Load {
            candidates: vec![img("b.png")],
        }]
    );
}

#[test]
fn a_failed_manual_navigation_keeps_the_image_and_the_original_deadline() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::NextRequested);
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: None,
            now_ms: 12_000,
        }),
        vec![]
    );
    // 失败候选不算成功切换：当前图保留，计时不重置。
    assert_eq!(core.view(), View::Showing(img("a.png")));
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 30_000 }),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
}

#[test]
fn a_failed_auto_advance_retries_after_a_full_interval_instead_of_spinning() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    core.handle(Intent::Tick { now_ms: 30_000 });
    core.handle(Intent::LoadCompleted {
        shown: None,
        now_ms: 30_500,
    });
    assert_eq!(core.view(), View::Showing(img("a.png")));
    assert_eq!(core.handle(Intent::Tick { now_ms: 31_000 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 60_500 }),
        vec![Effect::Load {
            candidates: vec![img("b.png")],
        }]
    );
}

#[test]
fn a_single_image_never_advances_navigates_or_preloads() {
    let (mut core, effects) = showing_core(&["a.png"], 30);
    assert_eq!(effects, vec![]);
    assert_eq!(core.handle(Intent::Tick { now_ms: 10_000_000 }), vec![]);
    assert_eq!(core.handle(Intent::NextRequested), vec![]);
    assert_eq!(core.handle(Intent::PrevRequested), vec![]);
    assert_eq!(core.view(), View::Showing(img("a.png")));
}

#[test]
fn ticks_do_not_fire_while_a_load_is_in_flight() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::NextRequested);
    assert_eq!(core.handle(Intent::Tick { now_ms: 999_999 }), vec![]);
}

#[test]
fn a_navigation_pressed_during_a_load_runs_after_it_settles() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::NextRequested);
    // 加载在飞时按的上一张不丢弃：当场无效果，收尾后接着执行（只留最后一个）。
    assert_eq!(core.handle(Intent::NextRequested), vec![]);
    assert_eq!(core.handle(Intent::PrevRequested), vec![]);
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 5_000,
        }),
        vec![
            Effect::Preload { path: img("c.png") },
            Effect::Load {
                candidates: vec![img("a.png"), img("c.png")],
            },
        ]
    );
}

#[test]
fn an_interval_changed_during_a_failed_manual_navigation_still_recounts() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::NextRequested);
    assert_eq!(
        core.handle(Intent::IntervalSelected {
            secs: 60,
            now_ms: 10_000,
        }),
        vec![Effect::SaveInterval(60)]
    );
    core.handle(Intent::LoadCompleted {
        shown: None,
        now_ms: 12_000,
    });
    // 导航失败不算成功切换，但间隔已变：按新间隔从收尾时刻计满。
    assert_eq!(core.handle(Intent::Tick { now_ms: 71_999 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 72_000 }),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
}
