//! 工单 10：配置契约的真实文件测试（临时目录，不 mock 文件系统）。

mod common;

use moe_frame::config::{Animation, ConfigStore, PlaybackMode};
use moe_frame::decode_validation::DisplayMode;
use std::{fs, path::PathBuf};

fn temp_config(tag: &str) -> PathBuf {
    common::temp_root("moeframe-config-test", tag).join("config.toml")
}

#[test]
fn missing_file_yields_defaults_and_writes_back_a_legal_config() {
    let path = temp_config("missing");
    let mut store = ConfigStore::load(&path);
    let config = store.config().clone();
    assert_eq!(config.image_folder, None);
    assert!(config.recursive);
    assert_eq!(config.window.width, 480);
    assert_eq!(config.window.height, 320);
    assert_eq!(config.window.x, None);
    assert!(!config.window.lock_aspect);
    assert_eq!(config.playback.mode, PlaybackMode::Random);
    assert_eq!(config.playback.interval_secs, 30);
    assert!(config.playback.pause_on_hover);
    assert_eq!(config.playback.animation, Animation::Slide);
    assert_eq!(config.display.fit_mode, DisplayMode::Fit);
    assert!(!config.display.show_filename);
    assert!(!config.general.autostart);
    assert_eq!(config.general.current_image, None);

    store.set_image_folder(Some("D:\\Pictures\\Anime")).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    let reparsed = ConfigStore::load(&path);
    assert_eq!(
        reparsed.config().image_folder.as_deref(),
        Some("D:\\Pictures\\Anime")
    );
    // The three P1 keys are written with defaults even though nothing drives them yet.
    assert!(text.contains("pause_on_hover = true"), "{text}");
    assert!(text.contains("animation = \"slide\""), "{text}");
    assert!(text.contains("show_filename = false"), "{text}");
}

#[test]
fn corrupt_file_falls_back_to_defaults_without_crashing() {
    let path = temp_config("corrupt");
    fs::write(&path, "this is = not [ valid toml").unwrap();
    let store = ConfigStore::load(&path);
    assert_eq!(store.config().playback.interval_secs, 30);
    assert_eq!(store.config().image_folder, None);
    // The repaired file on disk must be parseable again.
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.parse::<toml_edit::DocumentMut>().is_ok());
}

#[test]
fn bad_fields_are_repaired_individually_and_legal_ones_survive() {
    let path = temp_config("repair");
    fs::write(
        &path,
        r##"
image_folder = "E:\\图 库"
recursive = "yes"

[window]
x = 1200
y = 600
width = -5
height = 320
lock_aspect = true

[playback]
mode = "bogus"
interval_secs = 2
pause_on_hover = "yes"
animation = "spin"

[display]
fit_mode = "fill"
show_filename = true

[general]
autostart = true
current_image = "E:\\图 库\\a.png"
"##,
    )
    .unwrap();
    let store = ConfigStore::load(&path);
    let config = store.config();
    // Legal hand edits survive untouched.
    assert_eq!(config.image_folder.as_deref(), Some("E:\\图 库"));
    assert_eq!(config.window.x, Some(1200));
    assert_eq!(config.window.y, Some(600));
    assert!(config.window.lock_aspect);
    assert_eq!(config.display.fit_mode, DisplayMode::Fill);
    assert!(config.display.show_filename);
    assert!(config.general.autostart);
    assert_eq!(
        config.general.current_image.as_deref(),
        Some("E:\\图 库\\a.png")
    );
    // Illegal fields fall back per the contract.
    assert!(config.recursive, "non-bool recursive repairs to true");
    assert_eq!(config.window.width, 480, "non-positive width repairs");
    assert_eq!(config.playback.mode, PlaybackMode::Random);
    assert_eq!(config.playback.interval_secs, 3, "integer clamps to range");
    assert!(config.playback.pause_on_hover);
    assert_eq!(config.playback.animation, Animation::Slide);
    // Repairs are written back to disk.
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("interval_secs = 3"), "{text}");
    assert!(text.contains("mode = \"random\""), "{text}");
}

#[test]
fn decimal_interval_falls_back_to_30_not_clamped() {
    let path = temp_config("decimal-interval");
    fs::write(&path, "[playback]\ninterval_secs = 2.5\n").unwrap();
    let store = ConfigStore::load(&path);
    assert_eq!(store.config().playback.interval_secs, 30);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("interval_secs = 30"), "{text}");
}

#[test]
fn unrelated_save_preserves_legal_hand_edits_and_unknown_keys() {
    let path = temp_config("preserve");
    fs::write(
        &path,
        r##"
image_folder = "D:\\old"
my_custom_note = "keep me"

[playback]
interval_secs = 45

[display]
fit_mode = "fill"
"##,
    )
    .unwrap();
    let mut store = ConfigStore::load(&path);
    store.set_image_folder(Some("D:\\new folder")).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("interval_secs = 45"), "{text}");
    assert!(text.contains("fit_mode = \"fill\""), "{text}");
    assert!(text.contains("my_custom_note = \"keep me\""), "{text}");
    let reloaded = ConfigStore::load(&path);
    assert_eq!(
        reloaded.config().image_folder.as_deref(),
        Some("D:\\new folder")
    );
    assert_eq!(reloaded.config().playback.interval_secs, 45);
}

#[test]
fn special_character_folder_path_round_trips() {
    let path = temp_config("special-chars");
    let mut store = ConfigStore::load(&path);
    let folder = "D:\\我的 图库 [test] & 收藏";
    store.set_image_folder(Some(folder)).unwrap();
    let reloaded = ConfigStore::load(&path);
    assert_eq!(reloaded.config().image_folder.as_deref(), Some(folder));
}

#[test]
fn saving_the_interval_changes_only_that_field() {
    let path = temp_config("interval-save");
    let mut store = ConfigStore::load(&path);
    store.set_image_folder(Some("D:\\图库")).unwrap();
    store.set_interval_secs(600).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("interval_secs = 600"), "{text}");
    assert!(text.contains("mode = \"random\""), "{text}");
    let reloaded = ConfigStore::load(&path);
    assert_eq!(reloaded.config().playback.interval_secs, 600);
    assert_eq!(reloaded.config().image_folder.as_deref(), Some("D:\\图库"));
    assert_eq!(reloaded.config().window.width, 480);
}

#[test]
fn saving_the_playback_mode_changes_only_that_field() {
    let path = temp_config("mode-save");
    let mut store = ConfigStore::load(&path);
    assert_eq!(store.config().playback.mode, PlaybackMode::Random);
    store.set_playback_mode(PlaybackMode::Sequential).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("mode = \"sequential\""), "{text}");
    assert!(text.contains("interval_secs = 30"), "{text}");
    let reloaded = ConfigStore::load(&path);
    assert_eq!(reloaded.config().playback.mode, PlaybackMode::Sequential);
    assert_eq!(reloaded.config().playback.interval_secs, 30);
}
