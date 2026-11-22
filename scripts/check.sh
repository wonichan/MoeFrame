#!/usr/bin/env bash
# 本仓库唯一的检查入口：本地与 CI 跑同一套。
# 用法：bash scripts/check.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# target/debug/incremental 的收尾在 Windows 上会偶发「拒绝访问 (os error 5)」，
# 关掉后构建日志不再出现伪错误。CI 上本来也没有增量收益。
export CARGO_INCREMENTAL=0

echo "== cargo fmt --check =="
cargo fmt --all -- --check

echo "== cargo clippy -D warnings =="
cargo clippy --all-targets -- -D warnings

echo "== cargo test --all-targets =="
cargo test --all-targets

echo "全部检查通过"