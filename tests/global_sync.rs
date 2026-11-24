//! 多实例吸附专题工单 04：全局设置跨实例同步的纯逻辑与配置层测试。
//!
//! 口径：各实例轮询 config.toml 的修改时间，变化时重读磁盘、只应用
//! 全局段（外观 / 播放 / 通用，含窗口吸附开关），不碰任何实例段、
//! 不因同步触发写回；文件未变不重复应用，全局段没变（例如对方只
//! 改了自己的几何）也不重复应用。

mod common;

use moe_frame::{
    config::{Animation, AppearanceMode, ConfigStore, PlaybackMode},
    global_sync::{GlobalSettings, GlobalSyncState},
};
use std::{fs, path::PathBuf, time::SystemTime};

fn temp_config(tag: &str) -> PathBuf {
    common::temp_root("moeframe-global-sync-test", tag).join("config.toml")
}

#[test]
fn unchanged_mtime_never_signals_a_reload() {
    let base = ConfigStore::load(&temp_config("base"));
    let applied = GlobalSettings::from_config(base.config());
    let t0 = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let mut state = GlobalSyncState::new(applied, Some(t0));
    assert!(!state.mtime_changed(Some(t0)), "文件未变不重复应用");
    assert!(!state.mtime_changed(None), "取不到修改时间不误报");
    let t1 = t0 + std::time::Duration::from_secs(2);
    assert!(state.mtime_changed(Some(t1)), "修改时间变了才发信号");
    assert!(!state.mtime_changed(Some(t1)), "同一修改时间只报一次");
}

#[test]
fn apply_reloaded_reports_only_real_global_changes() {
    let path = temp_config("apply");
    let mut a = ConfigStore::load_for_instance(&path, 1);
    let mut b = ConfigStore::load_for_instance(&path, 2);
    let mut state = GlobalSyncState::new(GlobalSettings::from_config(b.config()), None);

    // A 只改自己的几何：全局段没变，B 重读后不应重复应用。
    a.set_window_geometry(Some(11), Some(22), 640, 400).unwrap();
    let reloaded = b.reload_globals();
    assert_eq!(state.apply_reloaded(reloaded), None);

    // A 改全局外观与开关：B 重读后得到新全局值并报出变化。
    a.set_appearance_mode(AppearanceMode::Minimal).unwrap();
    a.set_window_snap(false).unwrap();
    a.set_interval_secs(77).unwrap();
    let reloaded = b.reload_globals();
    let changed = state.apply_reloaded(reloaded).expect("全局段变了应报出");
    assert_eq!(changed.appearance, AppearanceMode::Minimal);
    assert!(!changed.window_snap);
    assert_eq!(changed.interval_secs, 77);
    assert_eq!(changed.mode, PlaybackMode::Random);
    assert_eq!(changed.animation, Animation::Curl);

    // 同一份全局值再应用一次：无变化，不重复应用。
    let reloaded = b.reload_globals();
    assert_eq!(state.apply_reloaded(reloaded), None);
}

#[test]
fn sync_reload_never_touches_instance_sections_or_geometry() {
    let path = temp_config("isolation");
    let mut a = ConfigStore::load_for_instance(&path, 1);
    let mut b = ConfigStore::load_for_instance(&path, 2);
    b.set_image_folder(Some("D:\\B 图库")).unwrap();
    b.set_window_geometry(Some(700), Some(500), 640, 400)
        .unwrap();
    let b_instances_before = b.config().instances.clone();

    a.set_appearance_mode(AppearanceMode::Custom).unwrap();
    let globals = b.reload_globals();
    assert_eq!(globals.appearance, AppearanceMode::Custom);
    // B 的图库、几何与全部实例段快照在同步前后不变。
    assert_eq!(b.config().image_folder.as_deref(), Some("D:\\B 图库"));
    assert_eq!(
        (
            b.config().window.x,
            b.config().window.y,
            b.config().window.width,
            b.config().window.height
        ),
        (Some(700), Some(500), 640, 400)
    );
    assert_eq!(b.config().instances, b_instances_before);
}

#[test]
fn sync_reload_does_not_write_back_to_disk() {
    let path = temp_config("readonly");
    let mut a = ConfigStore::load_for_instance(&path, 1);
    let mut b = ConfigStore::load_for_instance(&path, 2);
    a.set_window_snap(false).unwrap();
    let bytes_before = fs::read(&path).unwrap();
    let globals = b.reload_globals();
    assert!(!globals.window_snap);
    assert_eq!(
        fs::read(&path).unwrap(),
        bytes_before,
        "同步是只读应用，不得触发写回"
    );
}
