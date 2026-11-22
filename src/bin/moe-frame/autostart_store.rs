//! 开机自启的注册表实现。

use super::*;

/// 开机自启的注册表实现（工单 21）：当前用户 Run 项的 MoeFrame 值，
/// 存在即启用。业务配置不进注册表，只有这一项。
pub(crate) struct RegAutostartStore;
impl RegAutostartStore {
    pub(crate) fn run_key(&self, write: bool) -> io::Result<Option<winreg::RegKey>> {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        if write {
            hkcu.create_subkey_with_flags(RUN_KEY_PATH, winreg::enums::KEY_WRITE)
                .map(|(key, _)| Some(key))
        } else {
            match hkcu.open_subkey_with_flags(RUN_KEY_PATH, winreg::enums::KEY_READ) {
                Ok(key) => Ok(Some(key)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        }
    }
}
impl AutostartStore for RegAutostartStore {
    fn read_enabled(&mut self) -> io::Result<bool> {
        let Some(run) = self.run_key(false)? else {
            return Ok(false);
        };
        // 值存在即启用：字符串读不到时再看原始值——值在但类型不对也算
        // 「项存在」，不落进读失败退镜像的分支。
        match run.get_value::<String, _>(RUN_VALUE_NAME) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(_) => match run.get_raw_value(RUN_VALUE_NAME) {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error),
            },
        }
    }
    fn set_enabled(&mut self, enabled: bool, command: &str) -> io::Result<()> {
        let run = self
            .run_key(true)?
            .expect("写模式打开 Run 项必得键（打开或创建）");
        if enabled {
            run.set_value(RUN_VALUE_NAME, &command)
        } else {
            match run.delete_value(RUN_VALUE_NAME) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        }
    }
}
