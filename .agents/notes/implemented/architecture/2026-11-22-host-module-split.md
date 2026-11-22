# Agent Note: 宿主入口文件按职责拆分

Status: implemented

## Problem

正式相框的全部宿主行为长在单文件 `src/bin/moe-frame.rs` 里（拆分前 1399 行，6 个 `impl FrameApp` 块）。hashline 编辑后必须重读才能拿到新锚点，文件越长，每次改动的重读成本越高：本次会话该文件被读取 109 次、约 66 万字符，占全部工具输出的 28%，是单点最大的上下文开销。

结构上的压力同样明确：[宿主层六个深挖候选](../../proposed/architecture/2026-11-21-host-deepening-plan.md) 的落点全在这个文件，逐个落地只会让它更膨胀。拆模块与语义深挖是两件事，但顺序上拆在前更划算——候选逐个落地时，落点是已存在的模块，而不是继续往一个 1400 行的文件里加 `impl` 块。

## Decision

按现有 `impl FrameApp` 块的职责做机械搬移，零行为改动：入口 `src/bin/moe-frame.rs` 只剩 `main`、日志与配置装配、`app_directory`、`resize_debug_record`、`mod` 声明（220 行）；行为分到 `src/bin/moe-frame/`：

| 模块 | 行数 | 职责 |
|---|---|---|
| `app.rs` | 231 | `FrameApp` 定义、帧循环编排、`dispatch` |
| `frame_ui.rs` | 156 | `eframe::App::ui` 与画面绘制 |
| `settings_ui.rs` | 247 | 设置窗会话编排与草稿控件 |
| `input.rs` | 169 | 键与指针事件入口 |
| `geometry.rs` | 99 | 窗口几何观测与命令执行 |
| `tray.rs` | 94 | 托盘图标、菜单与点击意图 |
| `ui_regressions.rs` | 82 | 宿主侧行为测试（原文件末尾的测试模块） |
| `autostart_store.rs` | 54 | 注册表自启读写 |
| `texture.rs` | 48 | 纹理上传与转正 |
| `worker.rs` | 138 | 后台线程骨架（拆分前已是独立文件） |

本拆分曾在 `f48ec36` 上完成并提交（`5439260`）；push 时远端已前进 5 个提交，涉及同一批宿主代码，于是在 `b6e5741` 上重新生成同一套模块、重跑检查。机械拆分可以整批重放，这是它相对手工重构的额外好处。

约定：

- `FrameApp` 的字段与兄弟模块共享的类型一律 `pub(super)`；bin crate 内部可见，库 crate 不外泄。
- 子模块只依赖 `crate::app::FrameApp` 与 `moe_frame` 库；模块之间不互相 `use`，一律经 `FrameApp` 上的方法。
- `include_bytes!("../../assets/tray-icon.png")` 随测试模块下移一层改为 `../../../assets/tray-icon.png`；`assets/` 位置不动。
- 这是结构搬移，不是 interface 提炼：每个模块目前仍是 `impl FrameApp` 的方法集合，没有独立入口。深挖候选仍按原计划各自落地，届时以这些模块为落点。

## Alternatives considered

- **保持单文件，只补节索引注释** — 最强理由：零风险、不动可见性、不产生跨文件 diff。否决：重读成本由文件长度决定，注释不改变"编辑后必须重读"这一事实，开销一分不减。
- **直接实施深挖候选 1–6，不做机械拆分** — 最强理由：一次到位，新 module 自带可测 interface，比拆完再搬更省事。否决：那是六个各自需要行为测试与双轴评审的重构，和"零行为改动"不同量级；混在一起做，任何回归都无法归因到具体一侧。
- **按"宿主 / 库"两层重划，把能抽的都抽进 `src/`** — 否决：可复用逻辑本来就在 `src/`，宿主的问题是职责全挤在一处，不是层次不足；再抽一层只会把 egui 句柄送进库层。

## Consequences

- **收益**：单次编辑后的重读从 1399 行降到 ≤247 行；新增职责有明确的落点，不必再往一个文件里堆 `impl` 块。
- **代价与已知上限**：`FrameApp` 字段可见性从私有放宽到 `pub(super)`，模块间仍靠共享状态而非显式契约；能力边界只到"文件变小"，没到"模块变深"。重访信号：新职责又开始以"字段 + 方法"的形式直接长在 `app.rs` 上。
- 深挖候选的实施顺序与验收标准不变，本拆分只是把落点提前准备好。

## Verification

- `wc -l src/bin/moe-frame.rs src/bin/moe-frame/*.rs`：入口 220 行，九个模块合计 1180 行（另有拆分前就独立的 `worker.rs` 138 行），无遗漏的 `fn`（原文件 23 个函数全部在新树中，仅新增 `pub(super)` 可见性）。
- `cargo test --bin MoeFrame`：2 个宿主用例（托盘左右键、右键菜单）通过；`cargo test --all-targets` 全绿。
- `bash scripts/check.sh` 通过；`cargo build --release --bin MoeFrame --locked` 产出 `target/release/MoeFrame.exe`。