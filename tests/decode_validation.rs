#[path = "support/decode_samples.rs"]
mod decode_samples;
use image::{Rgb, RgbImage};
use moe_frame::{
    decode_validation::load_display,
    loading::{DecodeLimits, DecodePhase, DisplayMode, LoadRequest},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct TempDirectory(PathBuf);
impl TempDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "moe-frame-decode-test-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn request(mode: DisplayMode) -> LoadRequest {
    LoadRequest {
        target_pixels: [720, 480],
        mode,
        limits: DecodeLimits {
            max_width: 4096,
            max_height: 4096,
            max_alloc: 64 * 1024 * 1024,
        },
    }
}

#[test]
fn real_1080p_is_prepared_for_physical_fit_without_retaining_source_pixels() {
    let directory = TempDirectory::new();
    let path = directory.0.join("1080p.png");
    RgbImage::from_pixel(1920, 1080, Rgb([30, 100, 220]))
        .save(&path)
        .unwrap();

    let display = load_display(&path, request(DisplayMode::Fit), |_| {}).unwrap();

    assert_eq!(display.path, path);
    assert_eq!(display.source_pixels, [1920, 1080]);
    assert_eq!(display.scaled_pixels, [720, 405]);
    assert_eq!(display.display_pixels, [720, 405]);
    assert_eq!(display.rgba.len(), 1_166_400);
    assert_eq!(&display.rgba[..4], &[30, 100, 220, 255]);
}

#[test]
fn real_fill_uses_coverage_then_center_crop_for_physical_pixels() {
    let directory = TempDirectory::new();
    let path = directory.0.join("wide.png");
    RgbImage::from_fn(2000, 1000, |x, _| {
        if !(200..1800).contains(&x) {
            Rgb([220, 20, 20])
        } else {
            Rgb([20, 160, 40])
        }
    })
    .save(&path)
    .unwrap();

    let display = load_display(&path, request(DisplayMode::Fill), |_| {}).unwrap();

    assert_eq!(display.scaled_pixels, [960, 480]);
    assert_eq!(display.display_pixels, [720, 480]);
    assert_eq!(display.rgba.len(), 1_382_400);
    assert_eq!(&display.rgba[..4], &[20, 160, 40, 255]);
    assert_eq!(&display.rgba[display.rgba.len() - 4..], &[20, 160, 40, 255]);
}

#[test]
fn extreme_fill_is_rejected_before_allocating_the_coverage_buffer() {
    let directory = TempDirectory::new();
    let path = directory.0.join("extreme.bmp");
    RgbImage::from_pixel(256, 1, Rgb([20, 80, 160]))
        .save(&path)
        .unwrap();
    let mut policy = request(DisplayMode::Fill);
    policy.target_pixels = [16, 16];
    policy.limits.max_alloc = 8192;
    let mut phases = Vec::new();

    let result = load_display(&path, policy, |phase| phases.push(phase));

    assert!(
        result.is_err(),
        "coverage output above the explicit experimental allocation must be refused"
    );
    let error = result.unwrap_err();
    assert_eq!(error.path, path);
    assert!(error.reason.contains("coverage buffer"));
    assert!(!phases.contains(&DecodePhase::Resizing));
}

#[test]
fn a_valid_image_disguised_as_another_extension_is_refused_before_decode() {
    let directory = TempDirectory::new();
    let original = directory.0.join("real.png");
    let disguised = directory.0.join("伪装.jpg");
    RgbImage::from_pixel(64, 32, Rgb([10, 50, 100]))
        .save(&original)
        .unwrap();
    fs::copy(&original, &disguised).unwrap();
    let mut phases = Vec::new();

    let error =
        load_display(&disguised, request(DisplayMode::Fit), |p| phases.push(p)).unwrap_err();

    assert_eq!(error.path, disguised);
    assert!(error.reason.contains("extension/header mismatch"));
    assert!(!phases.contains(&DecodePhase::Decoding));
}
#[test]
fn all_four_required_formats_decode_real_files_with_display_sized_output() {
    let directory = TempDirectory::new();
    let wanted = request(DisplayMode::Fit);
    for extension in ["png", "jpg", "webp", "bmp"] {
        let path = directory.0.join(format!("real.{extension}"));
        RgbImage::from_pixel(19, 11, Rgb([30, 180, 60]))
            .save(&path)
            .unwrap();
        let output = load_display(&path, wanted, |_| {}).unwrap();
        assert_eq!(output.source_pixels, [19, 11]);
        assert!(output.display_pixels[0] <= wanted.target_pixels[0]);
        assert!(output.display_pixels[1] <= wanted.target_pixels[1]);
        assert_eq!(
            output.rgba.len() as u64,
            u64::from(output.display_pixels[0]) * u64::from(output.display_pixels[1]) * 4
        );
    }
}

#[test]
fn malformed_palette_decoder_panic_is_rejected_and_the_real_cycle_continues() {
    use moe_frame::decode_validation::{CycleEvent, ValidationCycle};
    let directory = TempDirectory::new();
    let bad = directory.0.join("bad-palette.png");
    fs::write(&bad, decode_samples::malformed_palette_png()).unwrap();
    let good = directory.0.join("after-palette.png");
    RgbImage::from_pixel(16, 16, Rgb([20, 90, 160]))
        .save(&good)
        .unwrap();
    let error = load_display(&bad, request(DisplayMode::Fit), |_| {}).unwrap_err();
    assert_eq!(error.path, bad);
    assert!(error.reason.contains("decoder panic"));
    let mut cycle =
        ValidationCycle::new(vec![bad.clone(), good.clone()], request(DisplayMode::Fit));
    let mut events = Vec::new();
    cycle.prime_current(|event| events.push(event));
    let snapshot = cycle.preload_next(|event| events.push(event));
    assert_eq!(snapshot.current.as_ref().unwrap().path, good);
    assert_eq!(snapshot.next.as_ref().unwrap().path, good);
    assert!(events.iter().any(|e| matches!(e, CycleEvent::Rejected { error, .. } if error.path == bad && error.reason.contains("decoder panic"))));
}

#[test]
fn explicit_dimension_and_decoded_output_limits_stop_before_full_decode() {
    let directory = TempDirectory::new();
    let path = directory.0.join("bounded.png");
    RgbImage::from_pixel(512, 256, Rgb([50, 100, 150]))
        .save(&path)
        .unwrap();
    let mut dimension_limit = request(DisplayMode::Fit);
    dimension_limit.limits.max_width = 128;
    let mut output_limit = request(DisplayMode::Fit);
    output_limit.limits.max_alloc = 200_000;
    for policy in [dimension_limit, output_limit] {
        let mut phases = Vec::new();
        let error = load_display(&path, policy, |p| phases.push(p)).unwrap_err();
        assert_eq!(error.path, path);
        assert!(!error.reason.is_empty());
        assert!(!phases.contains(&DecodePhase::Decoding));
    }
}

#[test]
fn corrupt_and_truncated_inputs_keep_the_real_path_and_error() {
    let directory = TempDirectory::new();
    let good = directory.0.join("good.png");
    RgbImage::from_pixel(64, 32, Rgb([20, 60, 100]))
        .save(&good)
        .unwrap();
    let corrupt = directory.0.join("damaged.png");
    fs::write(&corrupt, b"not an image").unwrap();
    let truncated = directory.0.join("truncated.png");
    fs::write(&truncated, &fs::read(&good).unwrap()[..35]).unwrap();
    for path in [corrupt, truncated, directory.0.join("missing.png")] {
        let error = load_display(&path, request(DisplayMode::Fit), |_| {}).unwrap_err();
        assert_eq!(error.path, path);
        assert!(!error.reason.is_empty());
    }
}

#[test]
fn release_is_observed_before_display_ready_but_is_not_a_working_set_measurement() {
    let directory = TempDirectory::new();
    let path = directory.0.join("portrait.png");
    RgbImage::from_pixel(480, 960, Rgb([40, 80, 120]))
        .save(&path)
        .unwrap();
    let mut phases = Vec::new();
    let display = load_display(&path, request(DisplayMode::Fit), |p| phases.push(p)).unwrap();
    assert_eq!(display.display_pixels, [240, 480]);
    assert_eq!(display.rgba.len(), 460_800);
    assert_eq!(
        phases,
        [
            DecodePhase::Opening,
            DecodePhase::Decoding,
            DecodePhase::Resizing,
            DecodePhase::FullResolutionReleased,
            DecodePhase::DisplayReady,
        ]
    );
}

#[test]
fn real_cycle_preloads_one_next_and_promotes_it_without_decoding_it_again() {
    use moe_frame::decode_validation::{CycleEvent, ValidationCycle};
    let directory = TempDirectory::new();
    let first = directory.0.join("first.png");
    let second = directory.0.join("second.png");
    RgbImage::from_pixel(64, 32, Rgb([20, 40, 80]))
        .save(&first)
        .unwrap();
    RgbImage::from_pixel(64, 32, Rgb([80, 40, 20]))
        .save(&second)
        .unwrap();
    let mut policy = request(DisplayMode::Fit);
    policy.target_pixels = [16, 16];
    let mut cycle = ValidationCycle::new(vec![first.clone(), second.clone()], policy);
    cycle.prime_current(|_| {});
    let ready = cycle.preload_next(|_| {});
    assert_eq!(ready.current.as_ref().unwrap().path, first);
    assert_eq!(ready.next.as_ref().unwrap().path, second);
    assert_eq!(ready.resident_rgba_bytes(), 1024);
    drop(ready);

    let mut events = Vec::new();
    let advanced = cycle.advance(|e| events.push(e));

    assert_eq!(advanced.current.as_ref().unwrap().path, second);
    assert!(advanced.next.is_none());
    assert!(!events
        .iter()
        .any(|e| matches!(e, CycleEvent::Phase { path, .. } if path == &second)));
    drop(advanced);
    let repreloaded = cycle.preload_next(|_| {});
    assert_eq!(repreloaded.next.as_ref().unwrap().path, first);
    assert_eq!(repreloaded.resident_rgba_bytes(), 1024);
}

#[test]
fn cycle_releases_retired_and_cleared_images_when_snapshot_owners_leave() {
    use moe_frame::decode_validation::{CycleEvent, ValidationCycle};
    use std::sync::Arc;
    let directory = TempDirectory::new();
    let first = directory.0.join("first.png");
    let second = directory.0.join("second.png");
    RgbImage::from_pixel(64, 32, Rgb([20, 40, 80]))
        .save(&first)
        .unwrap();
    RgbImage::from_pixel(64, 32, Rgb([80, 40, 20]))
        .save(&second)
        .unwrap();
    let mut policy = request(DisplayMode::Fit);
    policy.target_pixels = [16, 16];
    let mut cycle = ValidationCycle::new(vec![first, second], policy);
    cycle.prime_current(|_| {});
    let ready = cycle.preload_next(|_| {});
    let old = Arc::downgrade(ready.current.as_ref().unwrap());
    let advanced = cycle.advance(|_| {});
    assert!(
        old.upgrade().is_some(),
        "a caller-owned snapshot is a real display-data owner"
    );
    drop(ready);
    assert!(
        old.upgrade().is_none(),
        "retired pixels must not be kept in a history cache"
    );
    let repreloaded = cycle.preload_next(|_| {});
    let current = Arc::downgrade(repreloaded.current.as_ref().unwrap());
    let next = Arc::downgrade(repreloaded.next.as_ref().unwrap());
    drop(advanced);
    drop(repreloaded);
    let mut events = Vec::new();

    cycle.clear(|e| events.push(e));

    assert!(current.upgrade().is_none());
    assert!(next.upgrade().is_none());
    let empty = cycle.snapshot();
    assert!(empty.current.is_none() && empty.next.is_none());
    assert_eq!(empty.resident_rgba_bytes(), 0);
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| matches!(
        e,
        CycleEvent::Retired {
            other_owners_at_retirement: 0,
            ..
        }
    )));
}

#[test]
fn cycle_reports_real_bad_paths_and_keeps_preloading_good_png_and_jpeg() {
    use moe_frame::decode_validation::{CycleEvent, ValidationCycle};
    let directory = TempDirectory::new();
    let corrupt = directory.0.join("broken.png");
    fs::write(&corrupt, b"not a png").unwrap();
    let over = directory.0.join("over-width.png");
    RgbImage::from_pixel(128, 32, Rgb([10, 20, 30]))
        .save(&over)
        .unwrap();
    let first = directory.0.join("first.png");
    let second = directory.0.join("second.jpg");
    RgbImage::from_pixel(32, 16, Rgb([20, 40, 80]))
        .save(&first)
        .unwrap();
    RgbImage::from_pixel(32, 16, Rgb([80, 40, 20]))
        .save(&second)
        .unwrap();
    let disguised = directory.0.join("png-disguised-as.jpg");
    fs::copy(&first, &disguised).unwrap();
    let mut policy = request(DisplayMode::Fit);
    policy.target_pixels = [16, 16];
    policy.limits.max_width = 64;
    let mut cycle = ValidationCycle::new(
        vec![
            corrupt.clone(),
            over.clone(),
            disguised.clone(),
            first.clone(),
            second.clone(),
        ],
        policy,
    );
    let mut events = Vec::new();

    cycle.prime_current(|e| events.push(e));
    let ready = cycle.preload_next(|e| events.push(e));

    assert_eq!(ready.current.as_ref().unwrap().path, first);
    assert_eq!(ready.next.as_ref().unwrap().path, second);
    let failures: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            CycleEvent::Rejected { error, .. } => Some((error.path.clone(), error.reason.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        failures.iter().map(|(path, _)| path).collect::<Vec<_>>(),
        vec![&corrupt, &over, &disguised]
    );
    assert!(failures.iter().all(|(_, reason)| !reason.is_empty()));
    assert!(!events.iter().any(|e| matches!(e, CycleEvent::Phase { path, phase: DecodePhase::Decoding, .. } if path == &corrupt || path == &over || path == &disguised)));
    drop(ready);
    let advanced = cycle.advance(|_| {});
    assert_eq!(advanced.current.as_ref().unwrap().path, second);
    drop(advanced);
    let repreloaded = cycle.preload_next(|_| {});
    assert_eq!(repreloaded.next.as_ref().unwrap().path, first);
}

#[test]
fn all_bad_inputs_finish_one_bounded_traversal_without_a_resident_image() {
    use moe_frame::decode_validation::{CycleEvent, ValidationCycle};
    let directory = TempDirectory::new();
    let broken = directory.0.join("broken.png");
    let missing = directory.0.join("missing.jpg");
    fs::write(&broken, b"not a png").unwrap();
    let mut cycle = ValidationCycle::new(
        vec![broken.clone(), missing.clone()],
        request(DisplayMode::Fit),
    );
    let mut failures = Vec::new();

    let empty = cycle.prime_current(|event| {
        if let CycleEvent::Rejected { error, .. } = event {
            failures.push(error);
        }
    });

    assert!(empty.current.is_none() && empty.next.is_none());
    assert_eq!(empty.resident_rgba_bytes(), 0);
    assert_eq!(
        failures.iter().map(|e| &e.path).collect::<Vec<_>>(),
        vec![&broken, &missing]
    );
    assert!(failures.iter().all(|e| !e.reason.is_empty()));
    let mut again = 0;
    let empty = cycle.advance(|event| {
        if matches!(event, CycleEvent::Rejected { .. }) {
            again += 1;
        }
    });
    assert!(empty.current.is_none() && empty.next.is_none());
    assert_eq!(again, 2);
}

#[test]
fn repeated_real_switches_do_not_keep_display_history() {
    use moe_frame::decode_validation::ValidationCycle;
    use std::sync::Arc;
    let directory = TempDirectory::new();
    let first = directory.0.join("first.png");
    let second = directory.0.join("second.png");
    RgbImage::from_pixel(32, 16, Rgb([20, 40, 80]))
        .save(&first)
        .unwrap();
    RgbImage::from_pixel(32, 16, Rgb([80, 40, 20]))
        .save(&second)
        .unwrap();
    let mut policy = request(DisplayMode::Fit);
    policy.target_pixels = [16, 16];
    let mut cycle = ValidationCycle::new(vec![first, second], policy);
    cycle.prime_current(|_| {});
    let mut ready = cycle.preload_next(|_| {});

    for _ in 0..200 {
        let old = Arc::downgrade(ready.current.as_ref().unwrap());
        drop(ready);
        let advanced = cycle.advance(|_| {});
        assert!(old.upgrade().is_none());
        ready = cycle.preload_next(|_| {});
        drop(advanced);
        assert!(ready.current.is_some() && ready.next.is_some());
        assert_eq!(ready.resident_rgba_bytes(), 1024);
    }
}

#[test]
fn cycle_publishes_current_before_touching_the_next_image() {
    use moe_frame::decode_validation::{CycleEvent, ValidationCycle};
    let directory = TempDirectory::new();
    let first = directory.0.join("first.png");
    let second = directory.0.join("second.png");
    RgbImage::from_pixel(64, 32, Rgb([20, 40, 80]))
        .save(&first)
        .unwrap();
    RgbImage::from_pixel(64, 32, Rgb([80, 40, 20]))
        .save(&second)
        .unwrap();
    let mut policy = request(DisplayMode::Fit);
    policy.target_pixels = [16, 16];
    let mut cycle = ValidationCycle::new(vec![first.clone(), second.clone()], policy);
    let mut events = Vec::new();

    let published = cycle.prime_current(|e| events.push(e));

    assert_eq!(published.current.as_ref().unwrap().path, first);
    assert!(
        published.next.is_none(),
        "publishing the current image must not wait for the next image"
    );
    assert!(
        !events.iter().any(|e| matches!(e,
            CycleEvent::Phase { path, .. } | CycleEvent::Prepared { path, .. }
            if path == &second
        )),
        "the next image must not be opened or decoded before the current one is published"
    );
    drop(published);

    let preloaded = cycle.preload_next(|_| {});

    assert_eq!(preloaded.next.as_ref().unwrap().path, second);
}

#[test]
fn advance_promotes_without_decoding_the_new_next_until_preload() {
    use moe_frame::decode_validation::{CycleEvent, ValidationCycle};
    let directory = TempDirectory::new();
    let first = directory.0.join("first.png");
    let second = directory.0.join("second.png");
    RgbImage::from_pixel(64, 32, Rgb([20, 40, 80]))
        .save(&first)
        .unwrap();
    RgbImage::from_pixel(64, 32, Rgb([80, 40, 20]))
        .save(&second)
        .unwrap();
    let mut policy = request(DisplayMode::Fit);
    policy.target_pixels = [16, 16];
    let mut cycle = ValidationCycle::new(vec![first.clone(), second.clone()], policy);
    cycle.prime_current(|_| {});
    cycle.preload_next(|_| {});
    let mut events = Vec::new();

    let advanced = cycle.advance(|e| events.push(e));

    assert_eq!(advanced.current.as_ref().unwrap().path, second);
    assert!(
        advanced.next.is_none(),
        "promotion must be publishable before the new next image is decoded"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, CycleEvent::Phase { path, .. } if path == &first)),
        "advance must not decode the new next image"
    );
    drop(advanced);

    let preloaded = cycle.preload_next(|_| {});

    assert_eq!(preloaded.next.as_ref().unwrap().path, first);
}
