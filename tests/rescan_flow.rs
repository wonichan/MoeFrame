//! 工单 16 验收标准 9：真实临时目录全流程——增删文件、手动重扫、导航、
//! 配置落盘与重启恢复，断言可见图片与落盘状态不变量（而非扫描器调用次数）。
//! 图片加载仍走行为入口回送（扫描只枚举路径，解码不在本流程范围内）。
mod common;

use common::temp_root;
use moe_frame::app_core::{CoreArgs, Effect, FrameCore, Intent, ResumeData, View};
use moe_frame::config::{ConfigStore, PlaybackMode};
use moe_frame::gesture::GestureThresholds;
use moe_frame::library::scan_library;
use std::path::PathBuf;

fn thresholds() -> GestureThresholds {
    GestureThresholds {
        drag_threshold: [4.0, 4.0],
        double_click_ms: 500,
        double_click_dist: [4.0, 4.0],
    }
}

fn strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

/// 像宿主一样执行落盘类效果（其余效果与本流程无关，忽略）。
fn apply(store: &mut ConfigStore, effects: &[Effect]) {
    for effect in effects {
        match effect {
            Effect::SaveResumeState {
                current,
                seen,
                history,
            } => {
                store
                    .save_resume_state(current.to_str(), &strings(seen), &strings(history))
                    .unwrap();
            }
            Effect::ClearResumeState => {
                store.save_resume_state(None, &[], &[]).unwrap();
            }
            Effect::SaveFolder(folder) => {
                store.set_image_folder(folder.to_str()).unwrap();
            }
            _ => {}
        }
    }
}

/// 像宿主启动时一样，从落盘配置组装跨启动恢复数据。
fn resume_from(store: &ConfigStore) -> Option<ResumeData> {
    let general = &store.config().general;
    if general.current_image.is_none()
        && general.round_seen.is_empty()
        && general.history.is_empty()
    {
        return None;
    }
    Some(ResumeData {
        current: general.current_image.as_ref().map(PathBuf::from),
        seen: general.round_seen.iter().map(PathBuf::from).collect(),
        history: general.history.iter().map(PathBuf::from).collect(),
    })
}

fn new_core(folder: &std::path::Path, resume: Option<ResumeData>) -> FrameCore {
    FrameCore::new(
        CoreArgs {
            saved_folder: Some(folder.to_path_buf()),
            interval_secs: 30,
            mode: PlaybackMode::Sequential,
            resume,
        },
        thresholds(),
    )
}

#[test]
fn rescan_merges_disk_changes_and_the_merged_state_survives_a_restart() {
    let library = temp_root("moeframe-rescan-flow", "library");
    let config_path = temp_root("moeframe-rescan-flow", "config").join("config.toml");
    for name in ["a.jpg", "b.jpg", "c.jpg"] {
        std::fs::write(library.join(name), b"").unwrap();
    }
    let mut store = ConfigStore::load(&config_path);
    let img = |name: &str| library.join(name);

    // 第一次启动：扫描真实目录，显示 a，再导航到 b（转场提交后落盘）。
    let mut core = new_core(&library, None);
    core.handle(Intent::Started);
    let scanned = scan_library(&library);
    assert_eq!(scanned, vec![img("a.jpg"), img("b.jpg"), img("c.jpg")]);
    core.handle(Intent::ScanCompleted(scanned));
    let effects = core.handle(Intent::LoadCompleted {
        shown: Some(img("a.jpg")),
        now_ms: 0,
    });
    apply(&mut store, &effects);
    core.handle(Intent::NextRequested);
    core.handle(Intent::LoadCompleted {
        shown: Some(img("b.jpg")),
        now_ms: 1_000,
    });
    let effects = core.handle(Intent::TransitionCompleted { now_ms: 1_000 });
    apply(&mut store, &effects);
    assert_eq!(core.view(), View::Showing(img("b.jpg")));

    // 用户在磁盘上删掉 c、新增 d，然后从菜单手动重扫。
    std::fs::remove_file(img("c.jpg")).unwrap();
    std::fs::write(img("d.jpg"), b"").unwrap();
    assert_eq!(
        core.handle(Intent::RescanRequested),
        vec![Effect::Scan(library.clone())]
    );
    let effects = core.handle(Intent::ScanCompleted(scan_library(&library)));
    apply(&mut store, &effects);
    // 当前图 b 不被打断；落盘状态是合并后的（进度与历史不含已删路径的新增项）。
    assert_eq!(core.view(), View::Showing(img("b.jpg")));
    let saved = store.config().general.clone();
    assert_eq!(saved.current_image.as_deref(), img("b.jpg").to_str());
    assert_eq!(saved.round_seen, strings(&[img("a.jpg"), img("b.jpg")]));
    assert_eq!(saved.history, strings(&[img("a.jpg"), img("b.jpg")]));

    // 重启：从同一份落盘配置恢复，首图仍是 b，导航走新图库（c 不复活、d 在列）。
    let store2 = ConfigStore::load(&config_path);
    let mut core2 = new_core(&library, resume_from(&store2));
    core2.handle(Intent::Started);
    let effects = core2.handle(Intent::ScanCompleted(scan_library(&library)));
    let [Effect::Load { candidates }] = effects.as_slice() else {
        panic!("重启后应先恢复当前图：{effects:?}");
    };
    assert_eq!(candidates.first(), Some(&img("b.jpg")));
    core2.handle(Intent::LoadCompleted {
        shown: Some(img("b.jpg")),
        now_ms: 0,
    });
    assert_eq!(core2.view(), View::Showing(img("b.jpg")));
    assert_eq!(
        core2.handle(Intent::NextRequested),
        vec![Effect::Load {
            candidates: vec![img("d.jpg"), img("a.jpg")],
        }]
    );
}
