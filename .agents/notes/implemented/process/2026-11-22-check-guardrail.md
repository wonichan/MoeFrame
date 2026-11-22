# Agent Note: 检查收敛到单一入口并由 CI 兜底

Status: implemented

## Problem

仓库有三条该跑的检查（`cargo fmt --check`、`cargo clippy`、`cargo test`），但没有任何强制面：无 CI、无 hook、无统一脚本，日志里也没有一份"该跑什么"的记录。检查能不能跑成，取决于当次代理是否想起来。

代价是可测的。仓库里实际躺着 4 处只在 `-D warnings` 下才暴露的 lint：`src/loading.rs:83` 的 `needless_borrows_for_generic_args`，以及两个验证二进制里 3 处 `field_reassign_with_default`（`src/bin/window-validation/native.rs:49`、`src/bin/decode-validation/observation.rs:30` 与 `:52`）。此前"检查通过"的结论来自更宽松的命令，而不是检查本身生效。工具链也缺件：`clippy` 与 `rust-analyzer` 都不是默认组件，本次得现装，装之前 LSP 诊断直接报 `Unknown binary 'rust-analyzer.exe'`。

另有一处环境噪声：Windows 上 `target/debug/incremental` 收尾会偶发 `拒绝访问 (os error 5)`，每次 `cargo` 调用刷一行与代码无关的错误，容易被读成构建失败。

## Decision

- **单一入口** `scripts/check.sh`：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --all-targets`，并在脚本内 `export CARGO_INCREMENTAL=0`。要加检查就扩这个脚本，不新建第二条命令。
- **CI 复用同一脚本**：`.github/workflows/check.yml` 在 `windows-latest` 上 `bash scripts/check.sh`，不重复声明工具链版本。
- **工具链固定在 `rust-toolchain.toml`**：`channel = "1.99.1"`，`components = ["rustfmt", "clippy", "rust-analyzer"]`。缺件由 toolchain 自动补齐，LSP 不再是可选步骤。
- **关闭 dev/test 增量编译**（`.cargo/config.toml`）：`[profile.dev]` 与 `[profile.test]` 均 `incremental = false`。
- lint 一律修在代码里，不用 `#[allow]` 压制：`src/loading.rs:83` 改为 `map_err(fail)`；两个验证二进制的 `Default::default()` 后赋值改为结构体字面量 + `..Default::default()`。

## Alternatives considered

- **只在 `AGENTS.md` 写"提交前跑这三条"** — 最强理由：零基建、零维护。否决：写在散文里的纪律等于没有纪律；本次 4 处 lint 正是这种做法的产物。
- **用 pre-commit hook 替代 CI** — 最强理由：反馈最快，能在提交前拦住。否决：hook 不随仓库分发（每人要手工装），Windows 上还依赖 bash 可用性；CI 是唯一对所有人同时生效的门。两者不互斥，将来可加，但不作为唯一门禁。
- **只 lint 产品 target，放过两个验证二进制** — 最强理由：那两个是一次性验证工具，不值得维护。否决：它们仍在 `cargo build` 的默认路径上，放过就是把会腐烂的角落留在仓库里；3 处修法各一行。
- **保留增量编译，靠 Defender 排除 `target/`** — 最强理由：debug 构建更快。否决：需要每台机器管理员权限手工配置，不可分发、不可复现。
- **把 clippy 固定到某个旧版本以回避新 lint** — 否决：lint 是信号不是障碍；固定版本的代价是每次升级都要重新面对同一批告警。

## Consequences

- **收益**：`cargo clippy --all-targets -D warnings` 从"失败"变成可当门；任何人（含代理）跑 `bash scripts/check.sh` 就得到与 CI 相同的结论；工具链与组件随仓库走，换机不再缺件。
- **代价与已知上限**：debug 构建失去增量，全量重编变慢；工具链锁在 1.99.1，升级要改一处并处理新 lint；CI 只跑 Windows，其他平台未覆盖（产品本就只支持 Windows 10/11 x64）。
- 重访信号：CI 时长成为阻塞，或出现第四类必须跑的检查（打包、资源预算）——那时扩 `scripts/check.sh`，不新增入口。

## Verification

- `bash scripts/check.sh` 全绿：fmt、clippy（`--all-targets -D warnings`）、全部测试（含 `tests/window_validation.rs` 13 个、宿主 2 个）。
- `cargo build --release --bin MoeFrame --locked` 通过，产出 `target/release/MoeFrame.exe`。
- `rustup component list --installed` 含 `clippy` 与 `rust-analyzer`；`lsp_diagnostics` 对 `src/lib.rs` 返回 0 诊断。
- `cargo` 调用不再出现 `拒绝访问 (os error 5)`（设置前后对照：同一命令设置前 15 次出现，设置后 0 次）。
## 后续调整：纯文档推送不触发 ci

2026-11-22 用户要求「纯文档的改动就不要跑 github action」。`.github/workflows/check.yml` 的
`push` 与 `pull_request` 都加了 `paths-ignore`：`**/*.md`、`docs/**`、`.scratch/**`、
`.agents/**`。同时改了源码/测试/Cargo/脚本的提交照旧跑。

保留的边界：`.github/**` 与 `scripts/**` 不在忽略列表里，所以改 workflow 自身或检查脚本
一定会触发一次运行——这正好能在推送时验证 workflow 语法。代价是纯文档提交的 CI 结论是
"被跳过"而不是"通过"，若将来给 `main` 加必需状态检查，文档提交会卡在 pending，需要
改成条件任务（job 内 `if:`）而不是 `paths-ignore`。
