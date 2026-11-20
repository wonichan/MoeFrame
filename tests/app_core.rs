//! 工单 10：应用行为入口测试——通过意图/效果接缝驱动相框核心，不触碰 GUI 与线程。

use moe_frame::app_core::{Effect, FrameCore, Intent, View};
use std::path::PathBuf;

fn path(text: &str) -> PathBuf {
    PathBuf::from(text)
}

#[test]
fn first_run_without_a_saved_folder_prompts_once_and_cancel_stays_empty() {
    let mut core = FrameCore::new(None);
    assert_eq!(core.handle(Intent::Started), vec![Effect::PickFolder]);
    assert_eq!(core.handle(Intent::FolderPickCancelled), vec![]);
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn picking_a_folder_saves_scans_loads_and_shows_the_first_image() {
    let mut core = FrameCore::new(None);
    core.handle(Intent::Started);
    let folder = path("D:\\图库");
    assert_eq!(
        core.handle(Intent::FolderPicked(folder.clone())),
        vec![Effect::SaveFolder(folder.clone()), Effect::Scan(folder)]
    );
    let candidates = vec![path("D:\\图库\\a.png"), path("D:\\图库\\b.png")];
    assert_eq!(
        core.handle(Intent::ScanCompleted(candidates.clone())),
        vec![Effect::LoadFirst { candidates }]
    );
    assert_eq!(
        core.handle(Intent::LoadCompleted(Some(path("D:\\图库\\a.png")))),
        vec![]
    );
    assert_eq!(core.view(), View::Showing(path("D:\\图库\\a.png")));
}

#[test]
fn a_saved_folder_skips_the_prompt_on_the_next_launch() {
    let mut core = FrameCore::new(Some(path("D:\\图库")));
    assert_eq!(
        core.handle(Intent::Started),
        vec![Effect::Scan(path("D:\\图库"))]
    );
}

#[test]
fn an_empty_scan_enters_the_empty_state_without_loading() {
    let mut core = FrameCore::new(Some(path("D:\\空文件夹")));
    core.handle(Intent::Started);
    assert_eq!(core.handle(Intent::ScanCompleted(vec![])), vec![]);
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn when_every_candidate_fails_the_frame_stays_operable_and_empty() {
    let mut core = FrameCore::new(Some(path("D:\\坏图库")));
    core.handle(Intent::Started);
    core.handle(Intent::ScanCompleted(vec![path("D:\\坏图库\\x.png")]));
    assert_eq!(core.handle(Intent::LoadCompleted(None)), vec![]);
    assert_eq!(core.view(), View::Empty);
}

#[test]
fn cancelling_a_menu_repick_keeps_the_current_image() {
    let mut core = FrameCore::new(Some(path("D:\\图库")));
    core.handle(Intent::Started);
    core.handle(Intent::ScanCompleted(vec![path("D:\\图库\\a.png")]));
    core.handle(Intent::LoadCompleted(Some(path("D:\\图库\\a.png"))));
    assert_eq!(
        core.handle(Intent::FolderPickRequested),
        vec![Effect::PickFolder]
    );
    assert_eq!(core.handle(Intent::FolderPickCancelled), vec![]);
    assert_eq!(core.view(), View::Showing(path("D:\\图库\\a.png")));
    assert_eq!(core.folder(), Some(&path("D:\\图库")));
}

#[test]
fn exit_is_an_explicit_effect() {
    let mut core = FrameCore::new(None);
    assert_eq!(core.handle(Intent::ExitRequested), vec![Effect::Exit]);
}
