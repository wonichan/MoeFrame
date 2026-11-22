//! 工单 04（深挖）：持久化同步测试——启动翻译（含自启对账装配）在
//! 真实临时配置上验证；镜像写回落盘可重读；写回失败口径有行为测试
//! （不阻塞、记一行日志、下次启动可收敛）。

mod common;

use common::temp_root;
use moe_frame::{
    autostart::AutostartStore,
    config::{AppearanceMode, ConfigStore, PlaybackMode},
    config_sync::ConfigSync,
};
use std::{
    io,
    path::{Path, PathBuf},
};

/// 可控的注册表接缝假实现：实际值、读写失败都可设定。
#[derive(Default)]
struct FakeStore {
    actual: bool,
    fail_read: bool,
    fail_write: bool,
    commands: Vec<String>,
}

impl AutostartStore for FakeStore {
    fn read_enabled(&mut self) -> io::Result<bool> {
        if self.fail_read {
            Err(io::Error::other("read failed"))
        } else {
            Ok(self.actual)
        }
    }

    fn set_enabled(&mut self, enabled: bool, command: &str) -> io::Result<()> {
        if self.fail_write {
            return Err(io::Error::other("write failed"));
        }
        self.actual = enabled;
        self.commands.push(command.to_owned());
        Ok(())
    }
}

fn paths(root: &Path) -> (PathBuf, PathBuf) {
    (root.join("config.toml"), root.join("moe_frame.log"))
}

/// 用文件 adapter 本身备一份有内容的配置（setter 是它的公开面）。
fn seed_config(config_path: &Path) {
    let mut store = ConfigStore::load(config_path);
    store.set_image_folder(Some(r"D:\图库")).expect("seed");
    store.set_interval_secs(45).expect("seed");
    store
        .set_playback_mode(PlaybackMode::Sequential)
        .expect("seed");
    store
        .set_window_geometry(Some(10), Some(20), 640, 480)
        .expect("seed");
    store.set_autostart(true).expect("seed");
    store
        .set_appearance_mode(AppearanceMode::Custom)
        .expect("seed");
    store
        .save_resume_state(
            Some(r"D:\图库\b.png"),
            &[r"D:\图库\a.png".to_owned(), r"D:\图库\b.png".to_owned()],
            &[r"D:\图库\a.png".to_owned(), r"D:\图库\b.png".to_owned()],
        )
        .expect("seed");
}

#[test]
fn startup_translates_config_into_a_package() {
    let root = temp_root("config-sync", "translate");
    let (config_path, log_path) = paths(&root);
    seed_config(&config_path);
    let mut registry = FakeStore {
        actual: true,
        ..Default::default()
    };
    let (_sync, package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    // 核心入参包。
    assert_eq!(package.core.saved_folder, Some(PathBuf::from(r"D:\图库")));
    assert_eq!(package.core.interval_secs, 45);
    assert_eq!(package.core.mode, PlaybackMode::Sequential);
    let resume = package.core.resume.expect("resume assembled");
    assert_eq!(resume.current, Some(PathBuf::from(r"D:\图库\b.png")));
    assert_eq!(resume.seen.len(), 2);
    assert_eq!(resume.history.len(), 2);
    // 宿主初值包。
    assert_eq!(package.host.saved_geom, (Some(10), Some(20), 640, 480));
    assert_eq!(package.host.saved_position, Some([10.0, 20.0]));
    assert_eq!(package.host.appearance, AppearanceMode::Custom);
    // 自启对账：实际与镜像一致，无回写。
    assert!(package.autostart.enabled());
}

#[test]
fn startup_reconciles_autostart_and_rewrites_the_mirror() {
    let root = temp_root("config-sync", "reconcile");
    let (config_path, log_path) = paths(&root);
    seed_config(&config_path);
    // 镜像 true、实际 false：实际为准，镜像被回写。
    let mut registry = FakeStore::default();
    let (_sync, package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    assert!(!package.autostart.enabled());
    let reloaded = ConfigStore::load(&config_path);
    assert!(!reloaded.config().general.autostart);
}

#[test]
fn startup_falls_back_to_the_mirror_when_the_registry_read_fails() {
    let root = temp_root("config-sync", "fallback");
    let (config_path, log_path) = paths(&root);
    seed_config(&config_path);
    let mut registry = FakeStore {
        fail_read: true,
        ..Default::default()
    };
    let (_sync, package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    assert!(package.autostart.enabled(), "读不到实际状态时退回镜像");
    let reloaded = ConfigStore::load(&config_path);
    assert!(reloaded.config().general.autostart, "无回写");
}

#[test]
fn an_empty_config_yields_no_resume_and_no_saved_position() {
    let root = temp_root("config-sync", "empty");
    let (config_path, log_path) = paths(&root);
    let mut registry = FakeStore::default();
    let (_sync, package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    assert_eq!(package.core.resume, None);
    assert_eq!(package.core.saved_folder, None);
    assert_eq!(package.host.saved_position, None);
}

#[test]
fn mirror_writes_land_in_the_config_file() {
    let root = temp_root("config-sync", "writes");
    let (config_path, log_path) = paths(&root);
    let mut registry = FakeStore::default();
    let (mut sync, _package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    sync.save_folder(Path::new(r"E:\新图库"));
    sync.save_interval(99);
    sync.save_playback_mode(PlaybackMode::Sequential);
    sync.save_window_geometry((Some(1), Some(2), 800, 600));
    sync.save_appearance_mode(AppearanceMode::Minimal);
    sync.save_autostart_mirror(true);
    sync.save_resume_state(
        Path::new(r"E:\新图库\x.png"),
        &[PathBuf::from(r"E:\新图库\x.png")],
        &[PathBuf::from(r"E:\新图库\x.png")],
    );
    let config = ConfigStore::load(&config_path).config().clone();
    assert_eq!(config.image_folder.as_deref(), Some(r"E:\新图库"));
    assert_eq!(config.playback.interval_secs, 99);
    assert_eq!(config.playback.mode, PlaybackMode::Sequential);
    assert_eq!(
        (
            config.window.x,
            config.window.y,
            config.window.width,
            config.window.height
        ),
        (Some(1), Some(2), 800, 600)
    );
    assert!(config.general.autostart);
    assert_eq!(config.display.appearance, AppearanceMode::Minimal);
    assert_eq!(
        config.general.current_image.as_deref(),
        Some(r"E:\新图库\x.png")
    );
    // 清除续播状态同样经它。
    sync.clear_resume_state();
    let config = ConfigStore::load(&config_path).config().clone();
    assert_eq!(config.general.current_image, None);
    assert!(config.general.round_seen.is_empty());
    assert!(config.general.history.is_empty());
}

#[test]
fn a_failed_write_logs_one_line_and_later_startup_can_converge() {
    let root = temp_root("config-sync", "failure");
    let (config_path, log_path) = paths(&root);
    seed_config(&config_path);
    let mut registry = FakeStore {
        actual: true,
        ..Default::default()
    };
    let (mut sync, _package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    // 破坏写目标：配置文件路径被目录占住，setter 必然失败。
    std::fs::remove_file(&config_path).expect("remove config");
    std::fs::create_dir(&config_path).expect("occupy path with a directory");
    // 失败不阻塞：调用照常返回，行为继续；每次失败记一行。
    sync.save_interval(77);
    sync.save_autostart_mirror(false);
    let log = std::fs::read_to_string(&log_path).expect("log written");
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2, "每次失败恰一行：{log}");
    assert!(lines[0].contains("interval_secs"), "{}", lines[0]);
    assert!(lines[1].contains("autostart"), "{}", lines[1]);
    // 收敛：目标恢复可写后，下一次启动装配的写回成功落盘。
    std::fs::remove_dir(&config_path).expect("free the path");
    seed_config(&config_path);
    let mut registry = FakeStore {
        actual: true,
        ..Default::default()
    };
    let (mut sync, _package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    sync.save_interval(77);
    let config = ConfigStore::load(&config_path).config().clone();
    assert_eq!(config.playback.interval_secs, 77);
}

#[test]
fn shutdown_flushes_only_a_changed_geometry() {
    let root = temp_root("config-sync", "shutdown");
    let (config_path, log_path) = paths(&root);
    seed_config(&config_path);
    let mut registry = FakeStore {
        actual: true,
        ..Default::default()
    };
    let (mut sync, package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    let saved = package.host.saved_geom;
    // 无观察值、或与落盘一致：不写，返回原记录。
    assert_eq!(sync.shutdown(None, saved), saved);
    assert_eq!(sync.shutdown(Some(saved), saved), saved);
    // 不同才写：返回新记录且文件已更新。
    let moved = (Some(50), Some(60), 640, 480);
    assert_eq!(sync.shutdown(Some(moved), saved), moved);
    let config = ConfigStore::load(&config_path).config().clone();
    assert_eq!((config.window.x, config.window.y), (Some(50), Some(60)));
}

#[cfg(unix)]
#[test]
fn a_non_utf8_current_image_skips_the_resume_save() {
    use std::os::unix::ffi::OsStrExt;
    let root = temp_root("config-sync", "nonutf8");
    let (config_path, log_path) = paths(&root);
    let mut registry = FakeStore::default();
    let (mut sync, _package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    let bad = PathBuf::from(std::ffi::OsStr::from_bytes(b"bad\xFFpath.png"));
    sync.save_resume_state(&bad, &[bad.clone()], &[bad.clone()]);
    // 整份跳过：不落盘、不记失败日志（跳过不是失败）。
    let config = ConfigStore::load(&config_path).config().clone();
    assert_eq!(config.general.current_image, None);
    assert!(!log_path.exists());
}

// ---------- 相框图路径（V2 相框外观专题工单 03） ----------

#[test]
fn startup_package_carries_the_frame_art_path_and_saves_write_through() {
    let root = temp_root("config-sync", "frame-art");
    let (config_path, log_path) = paths(&root);
    seed_config(&config_path);
    {
        let mut store = ConfigStore::load(&config_path);
        store
            .set_frame_art_path(Some(r"D:\Art\frame.png"))
            .expect("seed");
    }
    let mut registry = FakeStore::default();
    let (mut sync, package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    assert_eq!(
        package.host.frame_art,
        Some(PathBuf::from(r"D:\Art\frame.png"))
    );
    sync.save_frame_art_path(Some(Path::new(r"D:\Art\other.png")));
    let config = ConfigStore::load(&config_path).config().clone();
    assert_eq!(
        config.display.frame_art.as_deref(),
        Some(r"D:\Art\other.png")
    );
    sync.save_frame_art_path(None);
    let config = ConfigStore::load(&config_path).config().clone();
    assert_eq!(config.display.frame_art, None);
}
