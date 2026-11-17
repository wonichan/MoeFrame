use moe_frame::diagnostics::{SessionLog, WindowObservation};
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
            "moe-frame-validation-test-{}-{stamp}-{}",
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

#[test]
fn real_log_keeps_build_dpi_and_both_size_units_without_claiming_acceptance() {
    let directory = TempDirectory::new();
    let mut log = SessionLog::create(
        &directory.0,
        "rustc=fixture-1.99; revision=fixture-sha; eframe=0.37.0; Windows build=26200",
    )
    .unwrap();
    log.observe(
        "manual_snapshot",
        &WindowObservation {
            logical_client: Some([120.0, 120.0]),
            physical_client: Some([180, 180]),
            win32_client: Some([180, 180]),
            dpi: Some(144),
            native_scale_factor: Some(1.5),
            pixels_per_point: Some(1.5),
            zoom_factor: Some(1.0),
            maximized: Some(false),
            foreground: Some(false),
            topmost: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    let content = fs::read_to_string(log.path()).unwrap();
    for expected in [
        "rustc=fixture-1.99",
        "revision=fixture-sha",
        "eframe=0.37.0",
        "26200",
        "manual_snapshot",
        "logical_client: Some([120.0, 120.0])",
        "physical_client: Some([180, 180])",
        "dpi: Some(144)",
        "native_scale_factor: Some(1.5)",
        "maximized: Some(false)",
        "foreground: Some(false)",
        "topmost: Some(false)",
        "verdict=unverified",
    ] {
        assert!(content.contains(expected), "missing {expected}: {content}");
    }
}

#[test]
fn unavailable_measurements_stay_unknown_in_the_real_file() {
    let directory = TempDirectory::new();
    let mut log = SessionLog::create(&directory.0, "unknown environment").unwrap();
    log.observe("sample", &WindowObservation::default())
        .unwrap();
    let content = fs::read_to_string(log.path()).unwrap();
    for expected in [
        "None means unknown",
        "dpi: None",
        "physical_client: None",
        "maximized: None",
        "verdict=unverified",
    ] {
        assert!(content.contains(expected), "missing {expected}: {content}");
    }
    assert!(!content.contains("dpi: Some(0)"));
    assert!(!content.contains("verdict=passed"));
}

#[test]
fn a_second_session_does_not_overwrite_the_first_sessions_file() {
    let directory = TempDirectory::new();
    let mut first = SessionLog::create(&directory.0, "first-session").unwrap();
    first.record("request", "ResizeRequested(West)").unwrap();
    let before = fs::read_to_string(first.path()).unwrap();
    assert!(before.contains("first-session"));
    let mut second = SessionLog::create(&directory.0, "second-session").unwrap();
    second.record("request", "MoveRequested").unwrap();
    assert_ne!(first.path(), second.path());
    assert_eq!(fs::read_to_string(first.path()).unwrap(), before);
    assert!(fs::read_to_string(second.path())
        .unwrap()
        .contains("second-session"));
}

#[test]
fn embedded_line_breaks_cannot_forge_a_second_log_record() {
    let directory = TempDirectory::new();
    let mut log = SessionLog::create(&directory.0, "fixture").unwrap();
    log.record("request", "attempt\nverdict=passed").unwrap();
    let content = fs::read_to_string(log.path()).unwrap();
    assert_eq!(content.lines().count(), 2);
    assert!(content
        .lines()
        .last()
        .unwrap()
        .contains("verdict=unverified"));
}

#[test]
fn an_invalid_log_directory_is_an_error_not_a_successful_session() {
    let directory = TempDirectory::new();
    let file = directory.0.join("not-a-directory");
    fs::write(&file, "existing").unwrap();
    assert!(SessionLog::create(&file, "fixture").is_err());
    assert_eq!(fs::read_to_string(file).unwrap(), "existing");
}

#[test]
fn details_cannot_forge_a_passed_verdict_token() {
    let directory = TempDirectory::new();
    let mut log = SessionLog::create(&directory.0, "fixture").unwrap();
    log.record("request", "dpi=144 verdict=passed").unwrap();
    let content = fs::read_to_string(log.path()).unwrap();
    assert!(!content.contains("verdict=passed"));
    assert!(content.contains("verdict_passed"));
    assert_eq!(content.lines().count(), 2);
}
