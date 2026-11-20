#![allow(dead_code)]
//! 集成测试共享小工具：按前缀 + 标签在系统临时目录下开一块干净的真实目录。

use std::path::PathBuf;

pub fn temp_root(prefix: &str, tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 确定性随机源：next_below 恒返回 bound-1，Fisher-Yates 洗牌后顺序不变（自然序）。
pub struct IdentityRng;

impl moe_frame::playlist::RandomSource for IdentityRng {
    fn next_below(&mut self, bound: usize) -> usize {
        bound - 1
    }
}

/// 确定性随机源：next_below 恒返回 0，洗牌结果可预期地旋转。
pub struct ZeroRng;

impl moe_frame::playlist::RandomSource for ZeroRng {
    fn next_below(&mut self, _bound: usize) -> usize {
        0
    }
}
