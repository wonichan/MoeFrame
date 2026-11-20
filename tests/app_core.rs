//! 工单 10/11：应用行为入口测试——通过意图/效果接缝驱动相框核心，不触碰 GUI 与线程。
//! 时间通过意图携带的 now_ms 可控推进，不用真实等待。

mod common;

use common::{IdentityRng, ZeroRng};
use moe_frame::app_core::{Effect, FrameCore, Intent, ResumeData, View};
use moe_frame::config::PlaybackMode;
use std::path::PathBuf;

fn path(text: &str) -> PathBuf {
    PathBuf::from(text)
}

fn img(name: &str) -> PathBuf {
    PathBuf::from(format!("D:\\图库\\{name}"))
}

/// 驱动核心到「已显示第一张图」的状态（t=0 完成加载），返回核心与首图加载产生的效果。
fn showing_core(names: &[&str], interval_secs: i64) -> (FrameCore, Vec<Effect>) {
    showing_core_mode(names, interval_secs, PlaybackMode::Sequential)
}

fn showing_core_mode(
    names: &[&str],
    interval_secs: i64,
    mode: PlaybackMode,
) -> (FrameCore, Vec<Effect>) {
    let mut core = FrameCore::new(
        Some(path("D:\\图库")),
        interval_secs,
        mode,
        Box::new(IdentityRng),
        None,
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
        None,
        30,
        PlaybackMode::Sequential,
        Box::new(IdentityRng),
        None,
    );
    assert_eq!(core.handle(Intent::Started), vec![Effect::PickFolder]);
    assert_eq!(core.handle(Intent::FolderPickCancelled), vec![]);
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn picking_a_folder_saves_scans_loads_and_shows_the_first_image() {
    let mut core = FrameCore::new(
        None,
        30,
        PlaybackMode::Sequential,
        Box::new(IdentityRng),
        None,
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
        Some(path("D:\\图库")),
        30,
        PlaybackMode::Sequential,
        Box::new(IdentityRng),
        None,
    );
    assert_eq!(
        core.handle(Intent::Started),
        vec![Effect::Scan(path("D:\\图库"))]
    );
}

#[test]
fn an_empty_scan_enters_the_empty_state_without_loading() {
    let mut core = FrameCore::new(
        Some(path("D:\\空文件夹")),
        30,
        PlaybackMode::Sequential,
        Box::new(IdentityRng),
        None,
    );
    core.handle(Intent::Started);
    assert_eq!(core.handle(Intent::ScanCompleted(vec![])), vec![]);
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn when_every_candidate_fails_the_frame_stays_operable_and_empty() {
    let mut core = FrameCore::new(
        Some(path("D:\\坏图库")),
        30,
        PlaybackMode::Sequential,
        Box::new(IdentityRng),
        None,
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
        None,
        30,
        PlaybackMode::Sequential,
        Box::new(IdentityRng),
        None,
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
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 30_000,
        }),
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
    assert_eq!(
        core.handle(Intent::LoadCompleted {
            shown: Some(img("b.png")),
            now_ms: 5_000,
        }),
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
fn random_next_draws_only_unseen_and_a_new_round_avoids_the_last_image() {
    let (mut core, effects) =
        showing_core_mode(&["a.png", "b.png", "c.png"], 30, PlaybackMode::Random);
    // 首图显示后预加载的是袋首（未看路径），并落盘本次成功显示。
    assert_eq!(
        effects,
        vec![
            Effect::Preload { path: img("b.png") },
            Effect::SaveResumeState {
                current: img("a.png"),
                seen: vec![img("a.png")],
                history: vec![img("a.png")],
            },
        ]
    );
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 100,
    });
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("c.png")],
        }]
    );
    core.handle(Intent::LoadCompleted {
        shown: Some(img("c.png")),
        now_ms: 200,
    });
    // 全看完：新一轮候选避开刚显示的 c。
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("a.png"), img("b.png")],
        }]
    );
}

#[test]
fn random_prev_walks_history_and_next_returns_to_the_newest_first() {
    let (mut core, _) = showing_core_mode(&["a.png", "b.png", "c.png"], 30, PlaybackMode::Random);
    for (next, now) in [("b.png", 100), ("c.png", 200)] {
        core.handle(Intent::NextRequested);
        core.handle(Intent::LoadCompleted {
            shown: Some(img(next)),
            now_ms: now,
        });
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
    core.handle(Intent::NextRequested);
    core.handle(Intent::LoadCompleted {
        shown: None,
        now_ms: 100,
    });
    assert_eq!(core.view(), View::Showing(img("a.png")));
    // 失败不记账：没有历史可回看，袋子原样再来。
    assert_eq!(core.handle(Intent::PrevRequested), vec![]);
    assert_eq!(
        core.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("b.png"), img("c.png")],
        }]
    );
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

#[test]
fn a_random_first_image_is_drawn_from_the_shuffle_bag() {
    let mut core = FrameCore::new(
        Some(path("D:\\图库")),
        30,
        PlaybackMode::Random,
        Box::new(ZeroRng),
        None,
    );
    core.handle(Intent::Started);
    // ZeroRng 洗牌 [a,b,c] → [b,c,a]：首图候选是袋序，不是自然序。
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
}

// ---------- 工单 13：跨启动恢复 ----------

fn resumed_core(mode: PlaybackMode, resume: ResumeData) -> FrameCore {
    let mut core = FrameCore::new(
        Some(path("D:\\图库")),
        30,
        mode,
        Box::new(IdentityRng),
        Some(resume),
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
    // 旧本轮进度清空：首图候选是全图库的洗牌袋（恒等洗牌下即自然序）。
    assert_eq!(
        core.handle(Intent::ScanCompleted(vec![img("a.png"), img("b.png")])),
        vec![Effect::Load {
            candidates: vec![img("a.png"), img("b.png")],
        }]
    );
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
        Some(path("D:\\图库")),
        30,
        PlaybackMode::Random,
        Box::new(IdentityRng),
        None,
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
    let effects = first.handle(Intent::LoadCompleted {
        shown: Some(img("b.png")),
        now_ms: 100,
    });
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
