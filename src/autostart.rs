//! 开机自启同步（工单 21）：开关状态以实际系统状态为准，config 的
//! autostart 只是镜像。系统操作经 AutostartStore 接缝注入——真实实现
//! 写当前用户 Run 项，测试替身可控成功/失败。本模块只做决策：启动
//! 对齐（实际优先、读不到时用镜像）、切换（先写系统，成功才更新状态
//! 并给出要保存的镜像值；失败状态不动，即回滚）。

use std::{io, path::Path};

/// 自启项所在注册表路径（父规格第 10 节）：当前用户 Run 项，不写系统级。
pub const RUN_KEY_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
/// 自启项名（父规格第 10 节）。
pub const RUN_VALUE_NAME: &str = "MoeFrame";

/// 开机自启的系统操作接缝：read 返回实际是否已启用，set 按目标写入
/// 或移除自启项（command 为启用时的启动命令）。
pub trait AutostartStore {
    fn read_enabled(&mut self) -> io::Result<bool>;
    fn set_enabled(&mut self, enabled: bool, command: &str) -> io::Result<()>;
}

/// 自启启动命令：程序路径加引号，含空格/中文/特殊字符时仍正确指向。
pub fn autostart_command(exe_path: &Path) -> String {
    format!("\"{}\"", exe_path.to_string_lossy())
}

/// 开关状态：菜单与设置窗都读它，保证两处反映同一真实状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutostartSync {
    enabled: bool,
}

impl AutostartSync {
    /// 启动对齐：能读到实际状态时以它为准（与镜像不一致时返回需要
    /// 回写配置的实际值）；读失败时退回镜像值，不回写。
    pub fn startup(store: &mut dyn AutostartStore, config_mirror: bool) -> (Self, Option<bool>) {
        match store.read_enabled() {
            Ok(actual) => {
                let rewrite = (actual != config_mirror).then_some(actual);
                (Self { enabled: actual }, rewrite)
            }
            Err(_) => (
                Self {
                    enabled: config_mirror,
                },
                None,
            ),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// 切换到目标状态（set 语义，非无参翻转）：先写系统，成功才更新
    /// 自身状态并返回要保存的镜像值；失败返回 None 且状态保持操作前
    /// （回滚），不伪报成功。
    pub fn apply(
        &mut self,
        store: &mut dyn AutostartStore,
        target: bool,
        command: &str,
    ) -> Option<bool> {
        match store.set_enabled(target, command) {
            Ok(()) => {
                self.enabled = target;
                Some(target)
            }
            Err(_) => None,
        }
    }
}
