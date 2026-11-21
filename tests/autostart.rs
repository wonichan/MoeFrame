//! 工单 21：自启同步决策——启动以实际为准、切换先写系统、失败回滚。
use moe_frame::autostart::{autostart_command, AutostartStore, AutostartSync};
use std::{io, path::Path};

struct FakeStore {
    actual: bool,
    fail_read: bool,
    fail_write: bool,
    last_command: Option<String>,
}

impl FakeStore {
    fn new(actual: bool) -> Self {
        Self {
            actual,
            fail_read: false,
            fail_write: false,
            last_command: None,
        }
    }
}

impl AutostartStore for FakeStore {
    fn read_enabled(&mut self) -> io::Result<bool> {
        if self.fail_read {
            return Err(io::Error::other("read failed"));
        }
        Ok(self.actual)
    }

    fn set_enabled(&mut self, enabled: bool, command: &str) -> io::Result<()> {
        if self.fail_write {
            return Err(io::Error::other("write failed"));
        }
        self.actual = enabled;
        self.last_command = Some(command.to_owned());
        Ok(())
    }
}

#[test]
fn startup_prefers_the_actual_system_state_and_rewrites_the_mirror() {
    let mut store = FakeStore::new(true);
    let (sync, rewrite) = AutostartSync::startup(&mut store, false);
    assert!(sync.enabled());
    assert_eq!(rewrite, Some(true));
    // 一致时不回写。
    let mut store = FakeStore::new(false);
    let (sync, rewrite) = AutostartSync::startup(&mut store, false);
    assert!(!sync.enabled());
    assert_eq!(rewrite, None);
}

#[test]
fn startup_falls_back_to_the_config_mirror_when_the_read_fails() {
    let mut store = FakeStore::new(false);
    store.fail_read = true;
    let (sync, rewrite) = AutostartSync::startup(&mut store, true);
    assert!(sync.enabled());
    assert_eq!(rewrite, None);
}

#[test]
fn a_successful_apply_syncs_state_and_reports_the_value_to_save() {
    let mut store = FakeStore::new(false);
    let (mut sync, _) = AutostartSync::startup(&mut store, false);
    let saved = sync.apply(&mut store, true, "\"C:\\MoeFrame.exe\"");
    assert_eq!(saved, Some(true));
    assert!(sync.enabled());
    assert!(store.actual);
    assert_eq!(store.last_command.as_deref(), Some("\"C:\\MoeFrame.exe\""));
    let saved = sync.apply(&mut store, false, "\"C:\\MoeFrame.exe\"");
    assert_eq!(saved, Some(false));
    assert!(!sync.enabled());
}

#[test]
fn a_failed_apply_rolls_back_without_reporting_success() {
    let mut store = FakeStore::new(false);
    let (mut sync, _) = AutostartSync::startup(&mut store, false);
    store.fail_write = true;
    let saved = sync.apply(&mut store, true, "\"C:\\MoeFrame.exe\"");
    assert_eq!(saved, None);
    assert!(!sync.enabled(), "失败后开关状态必须停在操作前");
    assert!(!store.actual);
}

#[test]
fn the_command_quotes_paths_with_spaces_or_non_ascii() {
    assert_eq!(
        autostart_command(Path::new("D:\\我的 图库\\MoeFrame.exe")),
        "\"D:\\我的 图库\\MoeFrame.exe\""
    );
    assert_eq!(
        autostart_command(Path::new("C:\\MoeFrame.exe")),
        "\"C:\\MoeFrame.exe\""
    );
}
