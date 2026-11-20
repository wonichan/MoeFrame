//! 集成测试共享小工具：按前缀 + 标签在系统临时目录下开一块干净的真实目录。

use std::path::PathBuf;

pub fn temp_root(prefix: &str, tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
