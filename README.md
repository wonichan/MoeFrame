# MoeFrame

当前只实现了 [08 无边框窗口最小验证](.scratch/v1-exe/issues/08-window-validation.md)的独立工具，**不是正式相框产品**。轮播、图库扫描、托盘、设置窗、配置持久化都还没有实现。

## 运行验证工具

目标为 Windows 10/11 x64；需要 Rust ≥ 1.95。

```powershell
cargo build --release --bin window-validation --locked
.\target\release\window-validation.exe
```

- 内容区拖动移动，四边/四角缩放，最小 120×120 逻辑尺寸。
- `F9` 记快照，`F10` 最大化/还原，`Esc` 退出。
- 日志在 `%APPDATA%\MoeFrame\window-validation\`，每次启动独立保存。

操作清单与实测记录见 [窗口验证清单](docs/window-validation.md)。用户已在本机（Windows 11 x64 Build 26200、150% 缩放）操作确认通过；Windows 10、Build 26100、100% 缩放与混合 DPI 双屏仍未验证。编译、日志或截图都不代表人工验收。

## 目录

```text
src/lib.rs                      egui 输入 → 视口命令（拖动、八方向缩放、最大化守卫）
src/diagnostics.rs              真实文件日志与窗口观测
src/bin/window-validation.rs    验证程序入口
src/bin/window-validation/      Win32 观测（系统版本、前台窗口、客户区、DPI）
tests/                          输入入口与日志的自动测试
docs/window-validation.md       操作清单、实测记录与结果回传格式
docs/adr/                       已接受的架构决定
.scratch/v1-exe/                V1 规格、路线图与本轮实施票
.agents/notes/                  工程决定笔记
```
