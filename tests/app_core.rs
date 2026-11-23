//! 工单 10/11：应用行为入口测试——通过意图/效果接缝驱动相框核心，不触碰 GUI 与线程。
//! 时间通过意图携带的 now_ms 可控推进，不用真实等待。

// 随机性已归播放序列持有（工单 03 深挖）：核心测试不再注入确定性随机源。
use moe_frame::app_core::{
    ConcreteAnimation, CoreArgs, Effect, FrameCore, Intent, ResumeData, View,
};
use moe_frame::config::{Animation, PlaybackMode};
use moe_frame::gesture::GestureThresholds;
use moe_frame::playlist::{Direction, RandomSource};
use std::path::PathBuf;

fn path(text: &str) -> PathBuf {
    PathBuf::from(text)
}

fn img(name: &str) -> PathBuf {
    PathBuf::from(format!("D:\\图库\\{name}"))
}

/// 驱动核心到「已显示第一张图」的状态（t=0 完成加载），返回核心与首图加载产生的效果。
/// 手势阈值（工单 15）：拖动 4px、双击 500ms/4px，与系统默认值同量级，测试可控。
fn test_thresholds() -> GestureThresholds {
    GestureThresholds {
        drag_threshold: [4.0, 4.0],
        double_click_ms: 500,
        double_click_dist: [4.0, 4.0],
    }
}

/// 导航加载成功后核心先起转场；这一步把转场结算掉，返回提交效果（工单 14）。
fn settle_transition(core: &mut FrameCore, now_ms: u64) -> Vec<Effect> {
    core.handle(Intent::TransitionCompleted { now_ms })
}

fn showing_core(names: &[&str], interval_secs: i64) -> (FrameCore, Vec<Effect>) {
    showing_core_mode(names, interval_secs, PlaybackMode::Sequential)
}

fn showing_core_mode(
    names: &[&str],
    interval_secs: i64,
    mode: PlaybackMode,
) -> (FrameCore, Vec<Effect>) {
    let mut core = FrameCore::new(
        CoreArgs {
            saved_folder: Some(path("D:\\图库")),
            interval_secs,
            mode,
            animation: Animation::Curl,
            resume: None,
        },
        test_thresholds(),
    );
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
    let mut core = FrameCore::new(
        CoreArgs {
            saved_folder: None,
            interval_secs: 30,
            mode: PlaybackMode::Sequential,
            animation: Animation::Curl,
            resume: None,
        },
        test_thresholds(),
    );
    assert_eq!(core.handle(Intent::Started), vec![Effect::PickFolder]);
    assert_eq!(core.handle(Intent::FolderPickCancelled), vec![]);
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn picking_a_folder_saves_scans_loads_and_shows_the_first_image() {
    let mut core = FrameCore::new(
        CoreArgs {
            saved_folder: None,
            interval_secs: 30,
            mode: PlaybackMode::Sequential,
            animation: Animation::Curl,
            resume: None,
        },
        test_thresholds(),
    );
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
        vec![
            Effect::Preload { path: img("b.png") },
            Effect::SaveResumeState {
                current: img("a.png"),
                seen: vec![img("a.png")],
                history: vec![img("a.png")],
            },
        ]
    );
    assert_eq!(core.view(), View::Showing(img("a.png")));
}

#[test]
fn a_saved_folder_skips_the_prompt_on_the_next_launch() {
    let mut core = FrameCore::new(
        CoreArgs {
            saved_folder: Some(path("D:\\图库")),
            interval_secs: 30,
            mode: PlaybackMode::Sequential,
            animation: Animation::Curl,
            resume: None,
        },
        test_thresholds(),
    );
    assert_eq!(
        core.handle(Intent::Started),
        vec![Effect::Scan(path("D:\\图库"))]
    );
}

#[test]
fn an_empty_scan_enters_the_empty_state_without_loading() {
    let mut core = FrameCore::new(
        CoreArgs {
            saved_folder: Some(path("D:\\空文件夹")),
            interval_secs: 30,
            mode: PlaybackMode::Sequential,
            animation: Animation::Curl,
            resume: None,
        },
        test_thresholds(),
    );
    core.handle(Intent::Started);
    // 空扫描不加载，但要清掉可能残留的落盘恢复状态（工单 16）。
    assert_eq!(
        core.handle(Intent::ScanCompleted(vec![])),
        vec![Effect::ClearResumeState]
    );
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn when_every_candidate_fails_the_frame_stays_operable_and_empty() {
    let mut core = FrameCore::new(
        CoreArgs {
            saved_folder: Some(path("D:\\坏图库")),
            interval_secs: 30,
            mode: PlaybackMode::Sequential,
            animation: Animation::Curl,
            resume: None,
        },
        test_thresholds(),
    );
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
    let mut core = FrameCore::new(
        CoreArgs {
            saved_folder: None,
            interval_secs: 30,
            mode: PlaybackMode::Sequential,
            animation: Animation::Curl,
            resume: None,
        },
        test_thresholds(),
    );
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
    // 自动轮播同样先起转场（方向 Next），转场完成时才提交并重计。
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 30_000,
        }),
        vec![Effect::StartTransition {
            from: img("a.png"),
            to: img("b.png"),
            direction: Direction::Next,
            animation: ConcreteAnimation::Curl,
            duration_ms: 550,
        }]
    );
    assert_eq!(
        settle_transition(&mut core, 30_000),
        vec![
            Effect::Preload { path: img("c.png") },
            Effect::SaveResumeState {
                current: img("b.png"),
                seen: vec![img("a.png"), img("b.png")],
                history: vec![img("a.png"), img("b.png")],
            },
        ]
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
fn due_autoplay_waits_for_release_then_switches_once() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 5);
    core.set_autoplay_deferred(true);
    assert_eq!(core.handle(Intent::Tick { now_ms: 5_000 }), vec![]);
    assert_eq!(core.handle(Intent::Tick { now_ms: 16_000 }), vec![]);
    assert_eq!(core.view(), View::Showing(img("a.png")));
    assert!(core.is_playing());
    assert_eq!(core.next_deadline_ms(), None);

    core.set_autoplay_deferred(false);
    assert_eq!(core.next_deadline_ms(), Some(5_000));
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 16_050 }),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
    assert_eq!(core.handle(Intent::Tick { now_ms: 16_100 }), vec![]);
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 16_100,
    });
    settle_transition(&mut core, 17_000);
    assert_eq!(core.view(), View::Showing(img("b.png")));
    assert_eq!(core.next_deadline_ms(), Some(22_000));
}

#[test]
fn resize_defers_autoplay_but_not_manual_or_hidden_navigation() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 5);
    core.set_autoplay_deferred(true);
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
    assert!(!core.autoplay_load_in_flight());
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 5_000,
    });
    settle_transition(&mut core, 5_600);
    assert_eq!(core.handle(Intent::CloseRequested), vec![Effect::HideFrame]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 10_600 }),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("a.png")],
        }]
    );
    assert!(core.autoplay_load_in_flight());
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
    settle_transition(&mut core, 12_000);
    assert_eq!(core.handle(Intent::Tick { now_ms: 41_999 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 42_000 }),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("a.png")],
        }]
    );
}

#[test]
fn prev_from_the_first_image_wraps_to_the_tail() {
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
        vec![Effect::StartTransition {
            from: img("a.png"),
            to: img("b.png"),
            direction: Direction::Next,
            animation: ConcreteAnimation::Curl,
            duration_ms: 550,
        }]
    );
    assert_eq!(
        settle_transition(&mut core, 20_000),
        vec![
            Effect::Preload { path: img("c.png") },
            Effect::SaveResumeState {
                current: img("b.png"),
                seen: vec![img("a.png"), img("b.png")],
                history: vec![img("a.png"), img("b.png")],
            },
        ]
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
    assert_eq!(
        effects,
        vec![Effect::SaveResumeState {
            current: img("a.png"),
            seen: vec![img("a.png")],
            history: vec![img("a.png")],
        }]
    );
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
    // 加载成功先起转场；排队中的上一张等转场提交后才执行。
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 5_000,
        }),
        vec![Effect::StartTransition {
            from: img("a.png"),
            to: img("b.png"),
            direction: Direction::Next,
            animation: ConcreteAnimation::Curl,
            duration_ms: 550,
        }]
    );
    assert_eq!(
        settle_transition(&mut core, 5_000),
        vec![
            Effect::Preload { path: img("c.png") },
            Effect::SaveResumeState {
                current: img("b.png"),
                seen: vec![img("a.png"), img("b.png")],
                history: vec![img("a.png"), img("b.png")],
            },
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

// ---------- 工单 12：随机防重复与历史回看 ----------

#[test]
fn random_prev_walks_history_and_next_returns_to_the_newest_first() {
    let (mut core, _) = showing_core_mode(&["a.png", "b.png", "c.png"], 30, PlaybackMode::Random);
    for (next, now) in [("b.png", 100), ("c.png", 200)] {
        core.handle(Intent::NextRequested);
        core.handle(Intent::LoadCompleted {
            shown: Some(img(next)),
            now_ms: now,
        });
        settle_transition(&mut core, now);
    }
    // 上一张按实际历史回看。
    assert_eq!(
        core.handle(Intent::PrevRequested),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("a.png")],
        }]
    );
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 300,
    });
    settle_transition(&mut core, 300);
    assert_eq!(core.view(), View::Showing(img("b.png")));
    // 下一张先沿历史回到最新图，而不是抽新的随机图。
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("c.png")],
        }]
    );
    core.handle(Intent::LoadCompleted {
        shown: Some(img("c.png")),
        now_ms: 400,
    });
    settle_transition(&mut core, 400);
    // 回到最新端后，上一张仍可回看（历史未被回看改写）。
    assert_eq!(
        core.handle(Intent::PrevRequested),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("a.png")],
        }]
    );
}

#[test]
fn a_failed_random_navigation_records_nothing() {
    let (mut core, _) = showing_core_mode(&["a.png", "b.png", "c.png"], 30, PlaybackMode::Random);
    // 候选集与顺序归播放序列测试面（工单 03 深挖）；这里只断言核心侧
    // 事实：失败不记账、重试给出的仍是同一份候选。
    let first = core.handle(Intent::NextRequested);
    let [Effect::Load { candidates }] = first.as_slice() else {
        panic!("应发起导航加载：{first:?}");
    };
    let mut sorted = candidates.clone();
    sorted.sort();
    assert_eq!(sorted, vec![img("b.png"), img("c.png")]);
    core.handle(Intent::LoadCompleted {
        shown: None,
        now_ms: 100,
    });
    assert_eq!(core.view(), View::Showing(img("a.png")));
    // 失败不记账：没有历史可回看，袋子原样再来。
    assert_eq!(core.handle(Intent::PrevRequested), vec![]);
    assert_eq!(core.handle(Intent::NextRequested), first);
}

#[test]
fn switching_playback_mode_saves_it_and_keeps_the_deadline() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    assert_eq!(
        core.handle(Intent::PlaybackModeSelected(PlaybackMode::Random)),
        vec![Effect::SavePlaybackMode(PlaybackMode::Random)]
    );
    assert_eq!(core.playback_mode(), PlaybackMode::Random);
    // 同值再选无效果；计时不因切模式重置：仍在 30 秒处触发，
    // 且顺序模式下看过的 a 已记入本轮进度，随机只抽未看的 b。
    assert_eq!(
        core.handle(Intent::PlaybackModeSelected(PlaybackMode::Random)),
        vec![]
    );
    assert_eq!(core.handle(Intent::Tick { now_ms: 29_999 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 30_000 }),
        vec![Effect::Load {
            candidates: vec![img("b.png")],
        }]
    );
}

#[test]
fn a_single_image_in_random_mode_never_navigates() {
    let (mut core, _) = showing_core_mode(&["a.png"], 30, PlaybackMode::Random);
    assert_eq!(core.handle(Intent::NextRequested), vec![]);
    assert_eq!(core.handle(Intent::PrevRequested), vec![]);
    assert_eq!(core.handle(Intent::Tick { now_ms: 10_000_000 }), vec![]);
    assert_eq!(core.view(), View::Showing(img("a.png")));
}

// ---------- 工单 13：跨启动恢复 ----------

fn resumed_core(mode: PlaybackMode, resume: ResumeData) -> FrameCore {
    let mut core = FrameCore::new(
        CoreArgs {
            saved_folder: Some(path("D:\\图库")),
            interval_secs: 30,
            mode,
            animation: Animation::Curl,
            resume: Some(resume),
        },
        test_thresholds(),
    );
    core.handle(Intent::Started);
    core
}

#[test]
fn random_resume_restores_the_current_image_and_continues_with_unseen() {
    let mut core = resumed_core(
        PlaybackMode::Random,
        ResumeData {
            current: Some(img("b.png")),
            seen: vec![img("a.png"), img("b.png")],
            history: vec![img("a.png"), img("b.png")],
        },
    );
    // 首图先恢复 b；b 加载失败时才轮到未看的 c。
    assert_eq!(
        core.handle(Intent::ScanCompleted(vec![
            img("a.png"),
            img("b.png"),
            img("c.png"),
        ])),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 0,
    });
    assert_eq!(core.view(), View::Showing(img("b.png")));
    // 恢复显示照常落盘，但不重复记账：进度与历史与保存时一致。
    assert!(effects.contains(&Effect::SaveResumeState {
        current: img("b.png"),
        seen: vec![img("a.png"), img("b.png")],
        history: vec![img("a.png"), img("b.png")],
    }));
    // 继续播放只抽未看的 c：防重复跨启动生效。
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("c.png")],
        }]
    );
    core.handle(Intent::LoadCompleted {
        shown: Some(img("c.png")),
        now_ms: 100,
    });
    settle_transition(&mut core, 100);
    // 上一张仍能按恢复的历史回看。
    assert_eq!(
        core.handle(Intent::PrevRequested),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("a.png")],
        }]
    );
}

#[test]
fn random_resume_with_a_missing_current_starts_a_new_round() {
    let mut core = resumed_core(
        PlaybackMode::Random,
        ResumeData {
            current: Some(img("gone.png")),
            seen: vec![img("a.png"), img("gone.png")],
            history: vec![img("a.png"), img("gone.png")],
        },
    );
    // 旧本轮进度清空：首图候选是全图库的洗牌袋（顺序归播放序列测试面）。
    let effects = core.handle(Intent::ScanCompleted(vec![img("a.png"), img("b.png")]));
    let [Effect::Load { candidates }] = effects.as_slice() else {
        panic!("应发起首图加载：{effects:?}");
    };
    let mut sorted = candidates.clone();
    sorted.sort();
    assert_eq!(sorted, vec![img("a.png"), img("b.png")]);
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img("a.png")),
        now_ms: 0,
    });
    // 新一轮只记新图；缺失旧路径不被追加（它原有的旧条目保留，回看时跳过）。
    assert!(effects.contains(&Effect::SaveResumeState {
        current: img("a.png"),
        seen: vec![img("a.png")],
        history: vec![img("a.png"), img("gone.png"), img("a.png")],
    }));
}

#[test]
fn sequential_resume_restores_the_position_without_re_recording() {
    let mut core = resumed_core(
        PlaybackMode::Sequential,
        ResumeData {
            current: Some(img("b.png")),
            seen: vec![img("a.png"), img("b.png")],
            history: vec![img("a.png"), img("b.png")],
        },
    );
    assert_eq!(
        core.handle(Intent::ScanCompleted(vec![
            img("a.png"),
            img("b.png"),
            img("c.png"),
        ])),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png"), img("a.png")],
        }]
    );
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 0,
    });
    // 历史不因恢复显示再追加一条 b。
    assert!(effects.contains(&Effect::SaveResumeState {
        current: img("b.png"),
        seen: vec![img("a.png"), img("b.png")],
        history: vec![img("a.png"), img("b.png")],
    }));
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("a.png")],
        }]
    );
}

#[test]
fn sequential_resume_with_a_missing_current_takes_the_natural_successor() {
    let mut core = resumed_core(
        PlaybackMode::Sequential,
        ResumeData {
            current: Some(img("b.png")),
            seen: Vec::new(),
            history: Vec::new(),
        },
    );
    // b 已不在图库：从自然序中它的下一张 c 起循环寻找。
    assert_eq!(
        core.handle(Intent::ScanCompleted(vec![
            img("a.png"),
            img("c.png"),
            img("d.png"),
        ])),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("d.png"), img("a.png")],
        }]
    );
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img("c.png")),
        now_ms: 0,
    });
    assert_eq!(core.view(), View::Showing(img("c.png")));
    assert!(effects.contains(&Effect::SaveResumeState {
        current: img("c.png"),
        seen: vec![img("c.png")],
        history: vec![img("c.png")],
    }));
}

#[test]
fn playback_state_survives_across_two_launches() {
    // 第一次启动：随机模式看过 a、b 后退出，落盘状态被第二次启动接手。
    let mut first = FrameCore::new(
        CoreArgs {
            saved_folder: Some(path("D:\\图库")),
            interval_secs: 30,
            mode: PlaybackMode::Random,
            animation: Animation::Curl,
            resume: None,
        },
        test_thresholds(),
    );
    first.handle(Intent::Started);
    first.handle(Intent::ScanCompleted(vec![
        img("a.png"),
        img("b.png"),
        img("c.png"),
    ]));
    first.handle(Intent::LoadCompleted {
        shown: Some(img("a.png")),
        now_ms: 0,
    });
    first.handle(Intent::NextRequested);
    first.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 100,
    });
    let effects = settle_transition(&mut first, 100);
    let saved = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::SaveResumeState {
                current,
                seen,
                history,
            } => Some(ResumeData {
                current: Some(current.clone()),
                seen: seen.clone(),
                history: history.clone(),
            }),
            _ => None,
        })
        .expect("a successful display saves resume state");

    let mut second = resumed_core(PlaybackMode::Random, saved);
    assert_eq!(
        second.handle(Intent::ScanCompleted(vec![
            img("a.png"),
            img("b.png"),
            img("c.png"),
        ])),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
    second.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 0,
    });
    assert_eq!(second.view(), View::Showing(img("b.png")));
    // 第二次启动继续只抽未看的 c。
    assert_eq!(
        second.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("c.png")],
        }]
    );
}

// ---------- 工单 14：滑动转场 ----------

#[test]
fn transitions_report_direction_and_commit_only_on_completion() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::NextRequested);
    // 目标就绪先起转场：方向 Next，画面与记账都还没提交。
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 1_000,
        }),
        vec![Effect::StartTransition {
            from: img("a.png"),
            to: img("b.png"),
            direction: Direction::Next,
            animation: ConcreteAnimation::Curl,
            duration_ms: 550,
        }]
    );
    assert_eq!(core.view(), View::Showing(img("a.png")));
    // 转场期间自动轮播不触发（旧截止时刻已过也不行）。
    assert_eq!(core.handle(Intent::Tick { now_ms: 999_999 }), vec![]);
    // 完成时才提交：落盘、记账与从完成时起计满间隔。
    assert_eq!(
        settle_transition(&mut core, 1_300),
        vec![
            Effect::Preload { path: img("c.png") },
            Effect::SaveResumeState {
                current: img("b.png"),
                seen: vec![img("a.png"), img("b.png")],
                history: vec![img("a.png"), img("b.png")],
            },
        ]
    );
    assert_eq!(core.view(), View::Showing(img("b.png")));
    assert_eq!(core.handle(Intent::Tick { now_ms: 31_299 }), vec![]);
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 31_300 }),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("a.png")],
        }]
    );
    // 上一张的转场方向是 Prev。
    core.handle(Intent::LoadCompleted {
        shown: Some(img("c.png")),
        now_ms: 31_300,
    });
    settle_transition(&mut core, 31_300);
    core.handle(Intent::PrevRequested);
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 32_000,
        }),
        vec![Effect::StartTransition {
            from: img("c.png"),
            to: img("b.png"),
            direction: Direction::Prev,
            animation: ConcreteAnimation::Curl,
            duration_ms: 550,
        }]
    );
}

#[test]
fn intents_during_a_transition_keep_only_the_last_one() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::NextRequested);
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 1_000,
    });
    // 转场中连按下一张又改上一张：只保留最后一个（上一张），不并行、不累积。
    assert_eq!(core.handle(Intent::NextRequested), vec![]);
    assert_eq!(core.handle(Intent::PrevRequested), vec![]);
    assert_eq!(
        settle_transition(&mut core, 1_300),
        vec![
            Effect::Preload { path: img("c.png") },
            Effect::SaveResumeState {
                current: img("b.png"),
                seen: vec![img("a.png"), img("b.png")],
                history: vec![img("a.png"), img("b.png")],
            },
            Effect::Load {
                candidates: vec![img("a.png"), img("c.png")],
            },
        ]
    );
}

#[test]
fn pausing_during_a_transition_still_commits_the_display() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::NextRequested);
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 1_000,
    });
    core.handle(Intent::PlayPauseToggled { now_ms: 1_100 });
    // 转场照常提交显示与落盘，只是提交后不设计时。
    let effects = settle_transition(&mut core, 1_300);
    assert!(effects.contains(&Effect::SaveResumeState {
        current: img("b.png"),
        seen: vec![img("a.png"), img("b.png")],
        history: vec![img("a.png"), img("b.png")],
    }));
    assert_eq!(core.view(), View::Showing(img("b.png")));
    assert_eq!(core.handle(Intent::Tick { now_ms: 999_999 }), vec![]);
}

#[test]
fn a_transition_completed_without_a_transition_is_ignored() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    assert_eq!(settle_transition(&mut core, 0), vec![]);
    assert_eq!(core.view(), View::Showing(img("a.png")));
}

#[test]
fn switching_mode_during_a_history_transition_never_blanks_the_frame() {
    // 随机模式回看中段后向前返回，转场期间切到顺序模式（会清历史游标）：
    // 提交时历史落位落空也不能把已成功显示的图清成无图（审查发现）。
    let (mut core, _) = showing_core_mode(&["a.png", "b.png", "c.png"], 30, PlaybackMode::Random);
    for (next, now) in [("b.png", 100), ("c.png", 200)] {
        core.handle(Intent::NextRequested);
        core.handle(Intent::LoadCompleted {
            shown: Some(img(next)),
            now_ms: now,
        });
        settle_transition(&mut core, now);
    }
    core.handle(Intent::PrevRequested);
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 300,
    });
    settle_transition(&mut core, 300);
    core.handle(Intent::NextRequested);
    core.handle(Intent::LoadCompleted {
        shown: Some(img("c.png")),
        now_ms: 400,
    });
    core.handle(Intent::PlaybackModeSelected(PlaybackMode::Sequential));
    settle_transition(&mut core, 400);
    assert_eq!(core.view(), View::Showing(img("c.png")));
}

#[test]
fn a_failed_navigation_after_a_transition_keeps_the_new_image() {
    // 转场提交后的一次导航全失败：不产生转场，画面停在刚提交的新图上。
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::NextRequested);
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 100,
    });
    settle_transition(&mut core, 100);
    core.handle(Intent::NextRequested);
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: None,
            now_ms: 200,
        }),
        vec![]
    );
    assert_eq!(core.view(), View::Showing(img("b.png")));
}

// ---------- 切换动画专题工单 01：档位、时长与随机抽法 ----------

/// 指定动画档位驱动到「已显示第一张图」（随机档走默认系统随机源，
/// 只用于不关心抽法结果的场景）。
fn showing_core_animation(names: &[&str], animation: Animation) -> (FrameCore, Vec<Effect>) {
    let mut core = FrameCore::new(
        CoreArgs {
            saved_folder: Some(path("D:\\图库")),
            interval_secs: 30,
            mode: PlaybackMode::Sequential,
            animation,
            resume: None,
        },
        test_thresholds(),
    );
    core.handle(Intent::Started);
    let list: Vec<PathBuf> = names.iter().map(|name| img(name)).collect();
    core.handle(Intent::ScanCompleted(list));
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img(names[0])),
        now_ms: 0,
    });
    (core, effects)
}

/// 脚本随机源：按预设下标依次供核心抽动画，并把每次的 bound 记下来，
/// 使「等概率池 + 禁连续重复」能写成确定性断言。
struct ScriptedRandom {
    values: std::collections::VecDeque<usize>,
    bounds: std::rc::Rc<std::cell::RefCell<Vec<usize>>>,
}

impl RandomSource for ScriptedRandom {
    fn next_below(&mut self, bound: usize) -> usize {
        self.bounds.borrow_mut().push(bound);
        let value = self.values.pop_front().expect("scripted values exhausted");
        assert!(
            value < bound,
            "scripted {value} must be below bound {bound}"
        );
        value
    }
}

fn showing_core_random(
    names: &[&str],
    script: &[usize],
) -> (FrameCore, std::rc::Rc<std::cell::RefCell<Vec<usize>>>) {
    let bounds = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut core = FrameCore::with_animation_random(
        CoreArgs {
            saved_folder: Some(path("D:\\图库")),
            interval_secs: 30,
            mode: PlaybackMode::Sequential,
            animation: Animation::Random,
            resume: None,
        },
        test_thresholds(),
        Box::new(ScriptedRandom {
            values: script.iter().copied().collect(),
            bounds: bounds.clone(),
        }),
    );
    core.handle(Intent::Started);
    let list: Vec<PathBuf> = names.iter().map(|name| img(name)).collect();
    core.handle(Intent::ScanCompleted(list));
    core.handle(Intent::LoadCompleted {
        shown: Some(img(names[0])),
        now_ms: 0,
    });
    (core, bounds)
}

/// 导航到下一张并取回转场开始效果里的 (动画， 时长）。
fn next_transition(core: &mut FrameCore, shown: &str, now_ms: u64) -> (ConcreteAnimation, u64) {
    core.handle(Intent::NextRequested);
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img(shown)),
        now_ms,
    });
    match effects.as_slice() {
        [Effect::StartTransition {
            animation,
            duration_ms,
            ..
        }] => (*animation, *duration_ms),
        other => panic!("expected a single StartTransition, got {other:?}"),
    }
}

#[test]
fn each_animation_tier_reports_its_kind_and_duration() {
    for (tier, kind, duration_ms) in [
        (Animation::Curl, ConcreteAnimation::Curl, 550),
        (Animation::Blinds, ConcreteAnimation::Blinds, 650),
        (Animation::Ink, ConcreteAnimation::Ink, 750),
        (Animation::Card, ConcreteAnimation::Card, 600),
        (Animation::Pixel, ConcreteAnimation::Pixel, 550),
        // 淡化：2026-11-23 收编合并新增档，时长沿用对方交叉淡化的 240ms。
        (Animation::Fade, ConcreteAnimation::Fade, 240),
    ] {
        let (mut core, _) = showing_core_animation(&["a.png", "b.png"], tier);
        assert_eq!(
            next_transition(&mut core, "b.png", 1_000),
            (kind, duration_ms),
            "{tier:?}"
        );
    }
}

#[test]
fn the_none_tier_commits_immediately_without_a_transition() {
    let (mut core, _) = showing_core_animation(&["a.png", "b.png"], Animation::None);
    core.handle(Intent::NextRequested);
    // 无转场效果：加载成功即提交（预加载 + 落盘），画面立刻是新图。
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 1_000,
        }),
        vec![
            Effect::Preload { path: img("a.png") },
            Effect::SaveResumeState {
                current: img("b.png"),
                seen: vec![img("a.png"), img("b.png")],
                history: vec![img("a.png"), img("b.png")],
            },
        ]
    );
    assert_eq!(core.view(), View::Showing(img("b.png")));
}

#[test]
fn a_hidden_frame_commits_immediately_without_a_transition() {
    let (mut core, _) = showing_core_animation(&["a.png", "b.png"], Animation::Curl);
    assert_eq!(core.handle(Intent::CloseRequested), vec![Effect::HideFrame]);
    core.handle(Intent::NextRequested);
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 1_000,
    });
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::StartTransition { .. })),
        "hidden frame must not start a transition: {effects:?}"
    );
    assert_eq!(core.view(), View::Showing(img("b.png")));
}

#[test]
fn the_random_tier_draws_concrete_animations_without_repeating() {
    // 脚本恒抽下标 0：首抽在全池 6 种（含收编的淡化）里取 Curl；
    // 之后池子排除上一次，bound 变 5，下标 0 依次落到 Blinds、再回到
    // Curl（非连续重复合法）。
    let (mut core, bounds) = showing_core_random(&["a.png", "b.png", "c.png", "d.png"], &[0, 0, 0]);
    let mut kinds = Vec::new();
    for (shown, now) in [("b.png", 100), ("c.png", 200), ("d.png", 300)] {
        kinds.push(next_transition(&mut core, shown, now).0);
        settle_transition(&mut core, now);
    }
    assert_eq!(
        kinds,
        vec![
            ConcreteAnimation::Curl,
            ConcreteAnimation::Blinds,
            ConcreteAnimation::Curl,
        ]
    );
    assert_eq!(*bounds.borrow(), vec![6, 5, 5]);
}

#[test]
fn selecting_an_animation_saves_it_and_applies_from_the_next_transition() {
    let (mut core, _) = showing_core_animation(&["a.png", "b.png", "c.png"], Animation::Curl);
    // 转场已按旧档位（卷页）开始后改档：当前条不变，新档位产出 SaveAnimation。
    core.handle(Intent::NextRequested);
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 100,
        }),
        vec![Effect::StartTransition {
            from: img("a.png"),
            to: img("b.png"),
            direction: Direction::Next,
            animation: ConcreteAnimation::Curl,
            duration_ms: 550,
        }]
    );
    assert_eq!(
        core.handle(Intent::AnimationSelected(Animation::Pixel)),
        vec![Effect::SaveAnimation(Animation::Pixel)]
    );
    assert_eq!(core.animation(), Animation::Pixel);
    // 同值再选不重写。
    assert_eq!(
        core.handle(Intent::AnimationSelected(Animation::Pixel)),
        vec![]
    );
    settle_transition(&mut core, 200);
    // 下一次转场用新档位。
    assert_eq!(
        next_transition(&mut core, "c.png", 300),
        (ConcreteAnimation::Pixel, 550)
    );
}

#[test]
fn a_rescan_replacement_image_commits_without_a_transition() {
    let (mut core, _) = showing_core_animation(&["a.png", "b.png", "c.png"], Animation::Ink);
    core.handle(Intent::RescanRequested);
    core.handle(Intent::ScanCompleted(vec![img("b.png"), img("c.png")]));
    // 当前图被删后的替代图是「新首图」：直接提交，不起转场。
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 500,
    });
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::StartTransition { .. })),
        "rescan replacement must not start a transition: {effects:?}"
    );
    assert_eq!(core.view(), View::Showing(img("b.png")));
}

// ---------- 工单 15：指针手势 ----------

/// 在 480 宽相框的右区（x=440）点一下：按下并松开，不移动。
fn tap_right(core: &mut FrameCore, now_ms: u64) {
    core.handle(Intent::PointerDown {
        pos: [440.0, 160.0],
        frame_width: 480.0,
        now_ms,
    });
    core.handle(Intent::PointerUp {
        pos: [440.0, 160.0],
        now_ms,
    });
}

#[test]
fn a_single_click_never_navigates_from_the_black_margin_or_image_edge() {
    for x in [40.0, 440.0] {
        let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
        core.handle(Intent::PointerDown {
            pos: [x, 160.0],
            frame_width: 480.0,
            now_ms: 1_000,
        });
        core.handle(Intent::PointerUp {
            pos: [x, 160.0],
            now_ms: 1_000,
        });
        assert_eq!(core.handle(Intent::Tick { now_ms: 1_500 }), vec![]);
        assert_eq!(core.view(), View::Showing(img("a.png")));
    }
}

#[test]
fn a_double_click_opens_the_image_shown_at_the_first_press() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    // 第一下落在右区：若按单击处理本会切到 b；双击成立则不切换、打开 a。
    tap_right(&mut core, 1_000);
    core.handle(Intent::PointerDown {
        pos: [442.0, 160.0],
        frame_width: 480.0,
        now_ms: 1_200,
    });
    assert_eq!(
        core.handle(Intent::PointerUp {
            pos: [442.0, 160.0],
            now_ms: 1_250,
        }),
        vec![Effect::OpenImage { path: img("a.png") }]
    );
    assert_eq!(core.view(), View::Showing(img("a.png")));
    // 双击后没有等待中的单击残留。
    assert_eq!(core.handle(Intent::Tick { now_ms: 9_999 }), vec![]);
}

#[test]
fn a_second_press_beyond_the_double_click_distance_never_navigates() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    tap_right(&mut core, 1_000);
    assert_eq!(
        core.handle(Intent::PointerDown {
            pos: [40.0, 160.0],
            frame_width: 480.0,
            now_ms: 1_200,
        }),
        vec![]
    );
    assert_eq!(
        core.handle(Intent::PointerUp {
            pos: [40.0, 160.0],
            now_ms: 1_250,
        }),
        vec![]
    );
    assert_eq!(core.handle(Intent::Tick { now_ms: 1_750 }), vec![]);
    assert_eq!(core.view(), View::Showing(img("a.png")));
}

#[test]
fn a_press_that_becomes_a_drag_never_clicks() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::PointerDown {
        pos: [440.0, 160.0],
        frame_width: 480.0,
        now_ms: 1_000,
    });
    core.handle(Intent::PointerMoved {
        pos: [460.0, 160.0],
    });
    assert_eq!(
        core.handle(Intent::PointerUp {
            pos: [460.0, 160.0],
            now_ms: 1_100,
        }),
        vec![]
    );
    assert_eq!(core.handle(Intent::Tick { now_ms: 9_999 }), vec![]);
    assert_eq!(core.view(), View::Showing(img("a.png")));
}

#[test]
fn a_second_press_turning_into_a_drag_does_not_navigate() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    tap_right(&mut core, 1_000);
    core.handle(Intent::PointerDown {
        pos: [441.0, 160.0],
        frame_width: 480.0,
        now_ms: 1_200,
    });
    assert_eq!(
        core.handle(Intent::PointerMoved {
            pos: [470.0, 160.0]
        }),
        vec![]
    );
    assert_eq!(
        core.handle(Intent::PointerUp {
            pos: [470.0, 160.0],
            now_ms: 1_300,
        }),
        vec![]
    );
    assert_eq!(core.view(), View::Showing(img("a.png")));
}

#[test]
fn a_right_press_cancels_a_waiting_click_and_rolls_back_an_active_drag() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    // 等待中的单击被右键取消：窗口结束也不切换。
    tap_right(&mut core, 1_000);
    assert_eq!(core.handle(Intent::RightPressed), vec![]);
    assert_eq!(core.handle(Intent::Tick { now_ms: 9_999 }), vec![]);
    assert_eq!(core.view(), View::Showing(img("a.png")));
    // 拖动中右键：取消并产出位置回滚效果；松开不再有后续。
    core.handle(Intent::PointerDown {
        pos: [240.0, 160.0],
        frame_width: 480.0,
        now_ms: 2_000,
    });
    core.handle(Intent::PointerMoved {
        pos: [300.0, 160.0],
    });
    assert_eq!(
        core.handle(Intent::RightPressed),
        vec![Effect::RollbackFrameDrag]
    );
    assert_eq!(
        core.handle(Intent::PointerUp {
            pos: [300.0, 160.0],
            now_ms: 2_100,
        }),
        vec![]
    );
}

#[test]
fn a_click_settled_during_a_transition_never_queues_navigation() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::NextRequested);
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 500,
    });
    tap_right(&mut core, 600);
    assert_eq!(core.handle(Intent::Tick { now_ms: 1_100 }), vec![]);
    let effects = settle_transition(&mut core, 1_100);
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::Load { .. })));
    assert_eq!(core.view(), View::Showing(img("b.png")));
}

#[test]
fn a_second_press_dragging_by_release_displacement_does_not_navigate() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    tap_right(&mut core, 1_000);
    core.handle(Intent::PointerDown {
        pos: [441.0, 160.0],
        frame_width: 480.0,
        now_ms: 1_200,
    });
    assert_eq!(
        core.handle(Intent::PointerUp {
            pos: [480.0, 160.0],
            now_ms: 1_300,
        }),
        vec![]
    );
    assert_eq!(core.view(), View::Showing(img("a.png")));
}

#[test]
fn a_second_click_after_the_double_click_window_does_not_navigate() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::PointerDown {
        pos: [440.0, 160.0],
        frame_width: 480.0,
        now_ms: 1_000,
    });
    assert_eq!(
        core.handle(Intent::PointerUp {
            pos: [440.0, 160.0],
            now_ms: 1_400,
        }),
        vec![]
    );
    assert_eq!(
        core.handle(Intent::PointerDown {
            pos: [441.0, 160.0],
            frame_width: 480.0,
            now_ms: 1_600,
        }),
        vec![]
    );
    assert_eq!(
        core.handle(Intent::PointerUp {
            pos: [441.0, 160.0],
            now_ms: 1_650,
        }),
        vec![]
    );
    assert_eq!(core.view(), View::Showing(img("a.png")));
}
#[test]
fn rescan_keeps_the_current_image_and_saves_the_merged_state() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    // 顺序模式显示 a；重扫：c 删除、d 新增。
    assert_eq!(
        core.handle(Intent::RescanRequested),
        vec![Effect::Scan(path("D:\\图库"))]
    );
    let effects = core.handle(Intent::ScanCompleted(vec![
        img("a.png"),
        img("b.png"),
        img("d.png"),
    ]));
    // 当前图不打断：不重新加载 a；只刷新预加载并落盘合并后的状态。
    assert!(effects.contains(&Effect::SaveResumeState {
        current: img("a.png"),
        seen: vec![img("a.png")],
        history: vec![img("a.png")],
    }));
    assert!(!effects.iter().any(|e| matches!(e, Effect::Load { .. })));
    assert_eq!(core.view(), View::Showing(img("a.png")));
    // 后续导航走新图库：下一张是 b，再下一张是 d，不再有 c。
    core.handle(Intent::NextRequested);
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 1_000,
    });
    settle_transition(&mut core, 1_000);
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("d.png"), img("a.png")],
        }]
    );
}

#[test]
fn rescan_with_the_current_image_deleted_starts_a_new_random_round() {
    let (mut core, _) = showing_core_mode(&["a.png", "b.png", "c.png"], 30, PlaybackMode::Random);
    // 随机模式当前图 a 被删：重扫后整轮作废，对剩余图库重新开轮。
    core.handle(Intent::RescanRequested);
    let effects = core.handle(Intent::ScanCompleted(vec![img("b.png"), img("c.png")]));
    let [Effect::Load { candidates }] = effects.as_slice() else {
        panic!("应发起替代图加载：{effects:?}");
    };
    let mut sorted = candidates.clone();
    sorted.sort();
    assert_eq!(sorted, vec![img("b.png"), img("c.png")]);
}

#[test]
fn rescan_with_the_current_image_deleted_takes_the_sequential_successor() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png", "d.png"], 30);
    // 顺序模式当前图 a 被删：从自然序后继 b 起循环找图。
    core.handle(Intent::RescanRequested);
    assert_eq!(
        core.handle(Intent::ScanCompleted(vec![
            img("b.png"),
            img("c.png"),
            img("d.png")
        ])),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png"), img("d.png")],
        }]
    );
}

#[test]
fn rescan_to_an_empty_library_returns_to_the_empty_state() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    core.handle(Intent::RescanRequested);
    // 图库清空：落盘的当前图/进度/历史同步清掉，重启不恢复已剔除状态。
    assert_eq!(
        core.handle(Intent::ScanCompleted(Vec::new())),
        vec![Effect::ClearResumeState]
    );
    assert_eq!(core.view(), View::Empty);
    // 空图库下导航无动作，不产生加载循环。
    assert_eq!(core.handle(Intent::NextRequested), vec![]);
}

#[test]
fn a_second_rescan_while_one_is_in_flight_is_ignored() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    assert_eq!(
        core.handle(Intent::RescanRequested),
        vec![Effect::Scan(path("D:\\图库"))]
    );
    assert_eq!(core.handle(Intent::RescanRequested), vec![]);
}

#[test]
fn rescan_is_ignored_while_a_load_is_in_flight() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    // 导航加载在飞：重扫不发起，避免扫描回送冲掉在飞状态。
    core.handle(Intent::NextRequested);
    assert_eq!(core.handle(Intent::RescanRequested), vec![]);
    // 加载收尾后重扫恢复可用。
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 1_000,
    });
    settle_transition(&mut core, 1_000);
    assert_eq!(
        core.handle(Intent::RescanRequested),
        vec![Effect::Scan(path("D:\\图库"))]
    );
}

#[test]
fn closing_the_frame_hides_it_instead_of_exiting() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    assert_eq!(core.handle(Intent::CloseRequested), vec![Effect::HideFrame]);
    assert!(!core.frame_visible());
    // 当前图与播放状态不动；隐藏中再关一次无动作；明确退出仍然有效。
    assert_eq!(core.view(), View::Showing(img("a.png")));
    assert!(core.is_playing());
    assert_eq!(core.handle(Intent::CloseRequested), vec![]);
    assert_eq!(core.handle(Intent::ExitRequested), vec![Effect::Exit]);
}

#[test]
fn toggling_visibility_follows_the_three_branch_recall_contract() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    // 显示且有焦点 → 隐藏。
    assert_eq!(
        core.handle(Intent::ToggleVisibilityRequested { focused: true }),
        vec![Effect::HideFrame]
    );
    assert!(!core.frame_visible());
    // 隐藏 → 显示（焦点标志不影响本分支）。
    assert_eq!(
        core.handle(Intent::ToggleVisibilityRequested { focused: false }),
        vec![Effect::ShowFrame]
    );
    assert!(core.frame_visible());
    // 显示但无焦点 → 只唤回聚焦，不隐藏、可见性不变。
    assert_eq!(
        core.handle(Intent::ToggleVisibilityRequested { focused: false }),
        vec![Effect::RaiseFrame]
    );
    assert!(core.frame_visible());
    // 唤回后有焦点，再点才隐藏。
    assert_eq!(
        core.handle(Intent::ToggleVisibilityRequested { focused: true }),
        vec![Effect::HideFrame]
    );
    assert!(!core.frame_visible());
}
#[test]
fn the_tray_menu_item_sets_the_visibility_named_by_its_label() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    // 「关闭相框」：菜单弹出时相框必然失焦，但这条意图不看焦点。
    assert_eq!(
        core.handle(Intent::SetFrameVisibleRequested { visible: false }),
        vec![Effect::HideFrame]
    );
    assert!(!core.frame_visible());
    // 「显示相框」：隐藏时给终态可见就显示并聚焦。
    assert_eq!(
        core.handle(Intent::SetFrameVisibleRequested { visible: true }),
        vec![Effect::ShowFrame]
    );
    assert!(core.frame_visible());
    // 已可见再要"显示"只唤到最前；已隐藏再要"关闭"无动作。
    assert_eq!(
        core.handle(Intent::SetFrameVisibleRequested { visible: true }),
        vec![Effect::RaiseFrame]
    );
    assert_eq!(
        core.handle(Intent::SetFrameVisibleRequested { visible: false }),
        vec![Effect::HideFrame]
    );
    assert_eq!(
        core.handle(Intent::SetFrameVisibleRequested { visible: false }),
        vec![]
    );
}

#[test]
fn the_slideshow_and_navigation_continue_while_hidden() {
    let (mut core, _) = showing_core(&["a.png", "b.png", "c.png"], 30);
    core.handle(Intent::CloseRequested);
    // 隐藏期间到点自动轮播，走同一套加载与提交（含落盘）。
    assert_eq!(
        core.handle(Intent::Tick { now_ms: 30_000 }),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
    // 隐藏时不起转场（切换动画专题工单 01）：加载成功直接提交并落盘。
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 30_000,
    });
    assert!(effects.contains(&Effect::SaveResumeState {
        current: img("b.png"),
        seen: vec![img("a.png"), img("b.png")],
        history: vec![img("a.png"), img("b.png")],
    }));
    // 隐藏期间手动导航同样有效。
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("c.png"), img("a.png")],
        }]
    );
    // 唤回后看到的是实际当前图。
    assert_eq!(
        core.handle(Intent::ToggleVisibilityRequested { focused: false }),
        vec![Effect::ShowFrame]
    );
    assert_eq!(core.view(), View::Showing(img("b.png")));
}

#[test]
fn pausing_from_the_tray_stops_the_hidden_slideshow() {
    let (mut core, _) = showing_core(&["a.png", "b.png"], 30);
    core.handle(Intent::CloseRequested);
    core.handle(Intent::PlayPauseToggled { now_ms: 1_000 });
    assert!(!core.is_playing());
    assert_eq!(core.handle(Intent::Tick { now_ms: 60_000 }), vec![]);
}
