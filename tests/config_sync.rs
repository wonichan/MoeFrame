//! 工单 04（深挖）：持久化同步测试——启动翻译（含自启对账装配）在
//! 真实临时配置上验证；镜像写回落盘可重读；写回失败口径有行为测试
//! （不阻塞、记一行日志、下次启动可收敛）。

mod common;

use common::temp_root;
use moe_frame::{
    autostart::AutostartStore,
    config::{Animation, AppearanceMode, BuiltinFrame, ConfigStore, FramePreset, PlaybackMode},
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
    store.set_animation(Animation::Ink).expect("seed");
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
    assert_eq!(package.core.animation, Animation::Ink);
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
fn builtin_frame_params_flow_through_startup_and_mirror_writes() {
    // 内置画框参数（V2 相框外观专题工单 02）：启动包携带初值，
    // 宿主改值经 save_builtin_frame 写回落盘。
    let root = temp_root("config-sync", "frame");
    let (config_path, log_path) = paths(&root);
    let mut store = ConfigStore::load(&config_path);
    store
        .set_builtin_frame(BuiltinFrame {
            preset: FramePreset::White,
            frame_width: 8.0,
            mat_width: 26.0,
        })
        .expect("seed");
    drop(store);
    let mut registry = FakeStore::default();
    let (mut sync, package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    assert_eq!(
        package.host.frame,
        BuiltinFrame {
            preset: FramePreset::White,
            frame_width: 8.0,
            mat_width: 26.0,
        }
    );
    sync.save_builtin_frame(BuiltinFrame {
        preset: FramePreset::Oak,
        frame_width: 30.0,
        mat_width: 2.0,
    });
    let config = ConfigStore::load(&config_path).config().clone();
    assert_eq!(
        config.display.frame,
        BuiltinFrame {
            preset: FramePreset::Oak,
            frame_width: 30.0,
            mat_width: 2.0,
        }
    );
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
    sync.save_animation(Animation::Pixel);
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
    assert_eq!(config.playback.animation, Animation::Pixel);
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

// ---------- 多实例吸附专题工单 01：实例贯通与级联 ----------

#[test]
fn two_instances_interleave_through_config_sync_without_rollback() {
    let root = temp_root("config-sync", "two-instances");
    let (config_path, log_path) = paths(&root);
    let mut registry = FakeStore::default();
    let (mut a, package_a) =
        ConfigSync::startup_for_instance(&config_path, &log_path, &mut registry, 1);
    let mut registry = FakeStore::default();
    let (mut b, package_b) =
        ConfigSync::startup_for_instance(&config_path, &log_path, &mut registry, 2);
    assert_eq!(package_a.host.instance_number, 1);
    assert_eq!(package_b.host.instance_number, 2);
    // A 改图库、B 改几何、交错落盘，最终两处都在。
    a.save_folder(Path::new(r"D:\A 图库"));
    b.save_window_geometry((Some(700), Some(500), 640, 400));
    a.save_window_geometry((Some(10), Some(20), 480, 320));
    b.save_folder(Path::new(r"D:\B 图库"));
    let a_config = ConfigStore::load_for_instance(&config_path, 1)
        .config()
        .clone();
    assert_eq!(a_config.image_folder.as_deref(), Some(r"D:\A 图库"));
    assert_eq!((a_config.window.x, a_config.window.y), (Some(10), Some(20)));
    let b_config = ConfigStore::load_for_instance(&config_path, 2)
        .config()
        .clone();
    assert_eq!(b_config.image_folder.as_deref(), Some(r"D:\B 图库"));
    assert_eq!(
        (b_config.window.x, b_config.window.y),
        (Some(700), Some(500))
    );
    // 核心入参也按实例取数：B 的启动包不带 A 的图库。
    assert_eq!(package_b.core.saved_folder, None);
}

#[test]
fn a_new_instance_cascades_from_the_previous_saved_position() {
    let root = temp_root("config-sync", "cascade");
    let (config_path, log_path) = paths(&root);
    {
        let mut store = ConfigStore::load_for_instance(&config_path, 1);
        store
            .set_window_geometry(Some(100), Some(100), 640, 400)
            .expect("seed");
    }
    let mut registry = FakeStore::default();
    let (_sync, package) =
        ConfigSync::startup_for_instance(&config_path, &log_path, &mut registry, 2);
    assert_eq!(
        package.host.saved_position,
        Some([124.0, 124.0]),
        "级联 +24/+24"
    );
    assert_eq!(package.host.saved_geom, (Some(124), Some(124), 480, 320));
}

#[test]
fn a_new_instance_cascades_from_the_live_partner_when_available() {
    // 审查修复 S4：有实时枚举基点时以它为准（已开相框的当前
    // 位置），落盘段（100,100）只作兜底、本次不生效。
    let root = temp_root("config-sync", "cascade-live");
    let (config_path, log_path) = paths(&root);
    {
        let mut store = ConfigStore::load_for_instance(&config_path, 1);
        store
            .set_window_geometry(Some(100), Some(100), 640, 400)
            .expect("seed");
    }
    let mut registry = FakeStore::default();
    let (_sync, package) = ConfigSync::startup_for_instance_with_cascade(
        &config_path,
        &log_path,
        &mut registry,
        2,
        Some([500.0, 300.0]),
    );
    assert_eq!(
        package.host.saved_position,
        Some([524.0, 324.0]),
        "相对已开相框的当前位置级联 +24/+24"
    );
}

#[test]
fn an_instance_with_saved_geometry_opens_exactly_there_without_cascade() {
    let root = temp_root("config-sync", "no-cascade");
    let (config_path, log_path) = paths(&root);
    {
        let mut store = ConfigStore::load_for_instance(&config_path, 1);
        store
            .set_window_geometry(Some(100), Some(100), 640, 400)
            .expect("seed");
        let mut store = ConfigStore::load_for_instance(&config_path, 2);
        store
            .set_window_geometry(Some(50), Some(60), 480, 320)
            .expect("seed");
    }
    let mut registry = FakeStore::default();
    let (_sync, package) =
        ConfigSync::startup_for_instance(&config_path, &log_path, &mut registry, 2);
    assert_eq!(package.host.saved_position, Some([50.0, 60.0]));
    assert_eq!(package.host.saved_geom, (Some(50), Some(60), 480, 320));
}

#[test]
fn window_snap_flows_through_startup_and_mirror_writes() {
    // 窗口吸附开关（多实例吸附专题工单 02）：全局段、默认开；启动包
    // 携带初值，宿主切换经 save_window_snap 走既有写入纪律写回。
    let root = temp_root("config-sync", "window-snap");
    let (config_path, log_path) = paths(&root);
    let mut registry = FakeStore::default();
    let (mut sync, package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    assert!(package.host.window_snap, "默认开");
    sync.save_window_snap(false);
    let config = ConfigStore::load(&config_path).config().clone();
    assert!(!config.general.window_snap);
    let mut registry = FakeStore::default();
    let (_sync, package) = ConfigSync::startup(&config_path, &log_path, &mut registry);
    assert!(!package.host.window_snap, "重读后按落盘值");
}
