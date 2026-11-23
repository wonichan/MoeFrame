# 01: 应用图标接入（exe / 窗口 / 托盘）

**What to build:** 把用户选定的 K6 图标母图接入三处：exe 资源图标、
主视口与设置视口的窗口图标、托盘图标（替换占位图）。

**Blocked by:** None

**Status:** resolved

## Acceptance criteria

- [x] `assets/icon.png`（512×512）与 `assets/icon.ico`（9 个尺寸）入库
- [x] `build.rs` 通过 Windows 宿主上的 `winresource` 请求嵌入图标资源（失败只警告），非 Windows 宿主构建不受影响。此项是接线，不证明实际发布产物带资源。
- [x] 主视口与设置视口都设置了窗口图标
- [x] `assets/tray-icon.png` 为母图的 48×48 渲染，托盘加载代码不变
- [x] 全量测试与 Windows 目标 clippy（-D warnings）通过
- [x] Windows x64 MSVC release 候选 exe 有完整图标资源，资源管理器显示 K6 图标。
- [x] [v0.1.4 正式发布 exe](https://github.com/wonichan/MoeFrame/releases/tag/v0.1.4) 用同一 Windows MSVC 流程构建，上传前图标复验通过；公开下载件与验收产物 SHA-256 及字节一致。`v0.1.3` 旧包保持原样。

## Context

- 图标选型过程（四轮候选）见主会话 2026-11-22；母图源文件为生成图
  K6（1600×1600），入库的是 512 母图与 ico 套件。
- 托盘图沿用工单 17 的文件名与解码路径，只换像素内容。

## Comments

实现（2026-11-22）：母图由生成图 K6（1600×1600）产出 512 母图与
9 尺寸 ico；build.rs 末尾追加 Windows-only 的 winresource 段
（失败只警告），Cargo.toml 加 `[target.'cfg(windows)'.build-dependencies]`；
`tray.rs` 新增 `app_icon_data()` 并更新托盘图注释，主视口与设置
视口各加 `.with_icon(app_icon_data())`；tray-icon.png 换成母图
48×48 渲染。Windows 目标 clippy -D warnings 干净，全量 230/230。
exe 资源图标在 Linux 交叉检查下按设计跳过，真机确认（下轮构建后
看资源管理器 exe 图标）未做，故 needs-info。笔记见
`.agents/notes/implemented/feature/2026-11-22-app-icon.md`。

- 原因已确认：[v0.1.3 发布说明](https://github.com/wonichan/MoeFrame/releases/tag/v0.1.3)注明产物由 Linux/MinGW-w64 交叉构建，而 `build.rs` 的嵌入步骤只在 Windows 宿主执行。对照 exe 的发布 SHA-256 一致、PE Resource Directory 地址和大小均为 0，Shell 返回通用图标。
- 候选已通过：Windows 11 x64，从干净的 `166de3e` 用 Windows x64 MSVC 执行 `cargo build --release --bin MoeFrame --locked`。候选 SHA-256 `4bcc9d5f47c38af4b12f04a36f1f5800dbd9d1044062671ebce4d9caee8be104`。PE 中有 1 个图标组、9 个图标帧；内嵌图标和 Shell 提取图标都与 `assets/icon.ico` 同尺寸渲染一致，资源管理器实际显示 K6，旧包仍显示通用图标。同一 exe 的缩放轮播复测两次通过。见[本轮记录](../../../real-machine-check-closeout.md)。
- 正式发布已验收：从干净的 `8a192dc` 按同一命令构建 Windows x64 MSVC `0.1.4`，资源管理器显示 K6，PE 含 1 个图标组和 9 帧；同一 exe 的缩放轮播复测两次通过。[公开下载](https://github.com/wonichan/MoeFrame/releases/download/v0.1.4/MoeFrame.exe)的 SHA-256 `864cf503eadf184e9ddd880943de60bf2923b0660d782918345eedf007d5ee73` 和上传前验收文件一致。关闭本票。
