//! 工单 10：配置契约的真实文件测试（临时目录，不 mock 文件系统）。

mod common;

use moe_frame::{
    config::{Animation, AppearanceMode, BuiltinFrame, ConfigStore, FramePreset, PlaybackMode},
    window_geom::Insets,
};
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
    assert_eq!(config.playback.mode, PlaybackMode::Random);
    assert_eq!(config.playback.interval_secs, 30);
    assert!(config.playback.pause_on_hover);
    assert_eq!(config.playback.animation, Animation::Slide);
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
fn legacy_fit_and_aspect_lock_keys_are_removed_on_load() {
    let path = temp_config("legacy-display-settings");
    fs::write(
        &path,
        r#"
recursive = true
my_custom_note = "keep me"

[window]
width = 640
height = 400
lock_aspect = true

[playback]
mode = "random"
interval_secs = 30
pause_on_hover = true
animation = "slide"

[display]
fit_mode = "fill"
show_filename = false

[general]
autostart = false
"#,
    )
    .unwrap();

    let store = ConfigStore::load(&path);
    assert_eq!(store.config().window.width, 640);
    assert_eq!(store.config().window.height, 400);
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("lock_aspect"), "{text}");
    assert!(!text.contains("fit_mode"), "{text}");
    assert!(text.contains("my_custom_note = \"keep me\""), "{text}");
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
    assert!(!text.contains("fit_mode"), "{text}");
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

#[test]
fn resume_state_round_trips_across_two_loads() {
    let path = temp_config("resume-round-trip");
    let seen = vec!["D:\\图库\\a.png".to_string(), "D:\\图库\\b.png".to_string()];
    let history = vec!["D:\\图库\\a.png".to_string(), "D:\\图库\\b.png".to_string()];
    {
        let mut store = ConfigStore::load(&path);
        store
            .save_resume_state(Some("D:\\图库\\b.png"), &seen, &history)
            .unwrap();
    }
    // 第二次启动：三份状态原样读回，其他偏好不受影响。
    let store = ConfigStore::load(&path);
    assert_eq!(
        store.config().general.current_image.as_deref(),
        Some("D:\\图库\\b.png")
    );
    assert_eq!(store.config().general.round_seen, seen);
    assert_eq!(store.config().general.history, history);
    assert_eq!(store.config().playback.interval_secs, 30);
}

#[test]
fn a_corrupt_resume_field_is_dropped_without_touching_the_others() {
    let path = temp_config("resume-field-repair");
    let text = "[general]\ncurrent_image = \"D:\\\\图库\\\\b.png\"\nround_seen = \"不是数组\"\nhistory = [\"D:\\\\图库\\\\a.png\", 42]\n";
    fs::write(&path, text).unwrap();
    let store = ConfigStore::load(&path);
    // round_seen 不是数组、history 混入非字符串：各自丢弃；current_image 合法保留。
    assert_eq!(
        store.config().general.current_image.as_deref(),
        Some("D:\\图库\\b.png")
    );
    assert!(store.config().general.round_seen.is_empty());
    assert!(store.config().general.history.is_empty());
    let rewritten = fs::read_to_string(&path).unwrap();
    assert!(!rewritten.contains("round_seen"), "{rewritten}");
    assert!(!rewritten.contains("42"), "{rewritten}");
}

#[test]
fn an_overlong_saved_history_is_trimmed_to_the_newest_512() {
    let path = temp_config("resume-history-trim");
    let entries: Vec<String> = (0..600).map(|i| format!("D:\\图库\\{i}.png")).collect();
    let array = entries
        .iter()
        .map(|e| format!("\"{}\"", e.replace('\\', "\\\\")))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(&path, format!("[general]\nhistory = [{array}]\n")).unwrap();
    let store = ConfigStore::load(&path);
    let history = &store.config().general.history;
    assert_eq!(history.len(), 512);
    assert_eq!(history.first().unwrap(), "D:\\图库\\88.png");
    assert_eq!(history.last().unwrap(), "D:\\图库\\599.png");
}

#[test]
fn interval_draft_parsing_keeps_the_two_rule_sets_apart() {
    use moe_frame::config::parse_interval_draft;
    // 区间内原样，越界钳制（与读配置的钳制方向一致）。
    assert_eq!(parse_interval_draft("30"), Some(30));
    assert_eq!(parse_interval_draft("3"), Some(3));
    assert_eq!(parse_interval_draft("2"), Some(3));
    assert_eq!(parse_interval_draft("0"), Some(3));
    assert_eq!(parse_interval_draft("-5"), Some(3));
    assert_eq!(parse_interval_draft("99999999"), Some(99_999_999));
    assert_eq!(parse_interval_draft("100000000"), Some(99_999_999));
    assert_eq!(parse_interval_draft("+42"), Some(42));
    // 超 i64 位数的纯数字仍是整数，按符号钳制，不当解析失败。
    assert_eq!(
        parse_interval_draft("99999999999999999999999999"),
        Some(99_999_999)
    );
    assert_eq!(parse_interval_draft("-99999999999999999999999999"), Some(3));
    // 解析失败一律 None：调用方保留上次有效值，绝不回退默认 30。
    assert_eq!(parse_interval_draft(""), None);
    assert_eq!(parse_interval_draft("3.5"), None);
    assert_eq!(parse_interval_draft("30.0"), None);
    assert_eq!(parse_interval_draft("abc"), None);
    assert_eq!(parse_interval_draft("1e3"), None);
    assert_eq!(parse_interval_draft("4 2"), None);
    assert_eq!(parse_interval_draft("-"), None);
}

#[test]
fn window_geometry_round_trips_through_a_real_file() {
    let path = temp_config("window-geom");
    let mut store = ConfigStore::load(&path);
    store
        .set_window_geometry(Some(1500), Some(700), 640, 400)
        .unwrap();
    let reparsed = ConfigStore::load(&path);
    assert_eq!(reparsed.config().window.x, Some(1500));
    assert_eq!(reparsed.config().window.y, Some(700));
    assert_eq!(reparsed.config().window.width, 640);
    assert_eq!(reparsed.config().window.height, 400);
    assert_eq!(reparsed.config().playback.interval_secs, 30);
    store.set_window_geometry(None, None, 480, 320).unwrap();
    let reparsed = ConfigStore::load(&path);
    assert_eq!(reparsed.config().window.x, None);
    assert_eq!(reparsed.config().window.y, None);
    assert_eq!(reparsed.config().window.width, 480);
}

#[test]
fn corrupt_window_fields_are_repaired_without_touching_other_sections() {
    let path = temp_config("window-repair");
    std::fs::write(
        &path,
        "image_folder = \"D:\\\\图库\"\n\n[window]\nx = \"东\"\ny = 40\nwidth = -5\nheight = 0\n",
    )
    .unwrap();
    let store = ConfigStore::load(&path);
    let window = &store.config().window;
    assert_eq!(window.x, None);
    assert_eq!(window.y, Some(40));
    assert_eq!(window.width, 480);
    assert_eq!(window.height, 320);
    assert_eq!(store.config().image_folder.as_deref(), Some("D:\\图库"));
}

// ---------- 外观模式（V2 相框外观专题工单 01） ----------

#[test]
fn appearance_defaults_to_builtin_and_is_written_back() {
    let path = temp_config("appearance-default");
    let store = ConfigStore::load(&path);
    assert_eq!(store.config().display.appearance, AppearanceMode::Builtin);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("appearance = \"builtin\""), "{text}");
}

#[test]
fn appearance_parses_all_three_modes() {
    for (raw, expected) in [
        ("minimal", AppearanceMode::Minimal),
        ("builtin", AppearanceMode::Builtin),
        ("custom", AppearanceMode::Custom),
    ] {
        let path = temp_config(&format!("appearance-{raw}"));
        fs::write(
            &path,
            format!("[display]\nshow_filename = false\nappearance = \"{raw}\"\n"),
        )
        .unwrap();
        let store = ConfigStore::load(&path);
        assert_eq!(store.config().display.appearance, expected, "{raw}");
    }
}

#[test]
fn an_invalid_appearance_repairs_to_builtin() {
    let path = temp_config("appearance-invalid");
    fs::write(
        &path,
        "[display]\nshow_filename = true\nappearance = \"neon\"\n",
    )
    .unwrap();
    let store = ConfigStore::load(&path);
    assert_eq!(store.config().display.appearance, AppearanceMode::Builtin);
    // 修复写回磁盘，同表其他字段不动。
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("appearance = \"builtin\""), "{text}");
    assert!(text.contains("show_filename = true"), "{text}");
}

#[test]
fn set_appearance_mode_round_trips_and_preserves_other_keys() {
    let path = temp_config("appearance-set");
    let mut store = ConfigStore::load(&path);
    store.set_appearance_mode(AppearanceMode::Custom).unwrap();
    let reparsed = ConfigStore::load(&path);
    assert_eq!(reparsed.config().display.appearance, AppearanceMode::Custom);
    assert_eq!(reparsed.config().playback.interval_secs, 30);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("appearance = \"custom\""), "{text}");
}

// ---------- 内置画框参数（V2 相框外观专题工单 02） ----------

#[test]
fn builtin_frame_defaults_to_walnut_and_spec_widths_and_writes_back() {
    let path = temp_config("frame-default");
    let store = ConfigStore::load(&path);
    let frame = store.config().display.frame;
    assert_eq!(frame.preset, FramePreset::Walnut);
    assert_eq!(frame.frame_width, 14.0);
    assert_eq!(frame.mat_width, 18.0);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("frame_preset = \"walnut\""), "{text}");
    assert!(text.contains("frame_width = 14.0"), "{text}");
    assert!(text.contains("mat_width = 18.0"), "{text}");
}

#[test]
fn frame_preset_parses_all_four_and_invalid_repairs_to_walnut() {
    for (raw, expected) in [
        ("walnut", FramePreset::Walnut),
        ("oak", FramePreset::Oak),
        ("black", FramePreset::Black),
        ("white", FramePreset::White),
    ] {
        let path = temp_config(&format!("frame-preset-{raw}"));
        fs::write(&path, format!("[display]\nframe_preset = \"{raw}\"\n")).unwrap();
        let store = ConfigStore::load(&path);
        assert_eq!(store.config().display.frame.preset, expected, "{raw}");
    }
    let path = temp_config("frame-preset-invalid");
    fs::write(&path, "[display]\nframe_preset = \"marble\"\n").unwrap();
    let store = ConfigStore::load(&path);
    assert_eq!(store.config().display.frame.preset, FramePreset::Walnut);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("frame_preset = \"walnut\""), "{text}");
}

#[test]
fn frame_widths_clamp_to_the_slider_range_and_accept_integers() {
    let path = temp_config("frame-width-clamp");
    fs::write(&path, "[display]\nframe_width = 99\nmat_width = -5.0\n").unwrap();
    let store = ConfigStore::load(&path);
    assert_eq!(store.config().display.frame.frame_width, 40.0);
    assert_eq!(store.config().display.frame.mat_width, 0.0);
    // 钳制结果写回磁盘。
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("frame_width = 40.0"), "{text}");
    assert!(text.contains("mat_width = 0.0"), "{text}");
}

#[test]
fn invalid_frame_widths_repair_to_defaults() {
    let path = temp_config("frame-width-invalid");
    fs::write(&path, "[display]\nframe_width = \"wide\"\n").unwrap();
    let store = ConfigStore::load(&path);
    assert_eq!(store.config().display.frame.frame_width, 14.0);
    assert_eq!(store.config().display.frame.mat_width, 18.0);
}

#[test]
fn set_builtin_frame_round_trips_and_preserves_other_keys() {
    let path = temp_config("frame-set");
    let mut store = ConfigStore::load(&path);
    store
        .set_builtin_frame(BuiltinFrame {
            preset: FramePreset::Black,
            frame_width: 22.0,
            mat_width: 0.0,
        })
        .unwrap();
    let reparsed = ConfigStore::load(&path);
    assert_eq!(
        reparsed.config().display.frame,
        BuiltinFrame {
            preset: FramePreset::Black,
            frame_width: 22.0,
            mat_width: 0.0,
        }
    );
    assert_eq!(
        reparsed.config().display.appearance,
        AppearanceMode::Builtin
    );
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("frame_preset = \"black\""), "{text}");
}

#[test]
fn builtin_insets_equal_frame_plus_mat_on_every_side() {
    // 内置模式的单边内距 = 框宽 + 卡纸宽（工单 01 占位在此替换）；
    // 极简恒全 0；自定义仍全 0 占位（内距由工单 03 的分析结果供给）。
    let frame = BuiltinFrame {
        preset: FramePreset::Oak,
        frame_width: 14.0,
        mat_width: 18.0,
    };
    assert_eq!(
        AppearanceMode::Builtin.insets(frame),
        Insets {
            left: 32.0,
            top: 32.0,
            right: 32.0,
            bottom: 32.0,
        }
    );
    assert_eq!(AppearanceMode::Minimal.insets(frame), Insets::ZERO);
    assert_eq!(AppearanceMode::Custom.insets(frame), Insets::ZERO);
}

#[test]
fn autostart_mirror_round_trips_through_a_real_file() {
    let path = temp_config("autostart");
    let mut store = ConfigStore::load(&path);
    assert!(!store.config().general.autostart);
    store.set_autostart(true).unwrap();
    let reparsed = ConfigStore::load(&path);
    assert!(reparsed.config().general.autostart);
    assert_eq!(reparsed.config().playback.interval_secs, 30);
}
