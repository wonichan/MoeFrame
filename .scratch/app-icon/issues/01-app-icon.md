# 01: 应用图标接入（exe / 窗口 / 托盘）

**What to build:** 把用户选定的 K6 图标母图接入三处：exe 资源图标、
主视口与设置视口的窗口图标、托盘图标（替换占位图）。

**Blocked by:** None

**Status:** needs-info

## Acceptance criteria

- [x] `assets/icon.png`（512×512）与 `assets/icon.ico`（9 个尺寸）入库
- [x] Windows 构建的 exe 带图标资源（build.rs + winresource，仅 Windows
      宿主生效，失败只警告）；非 Windows 宿主构建不受影响
- [x] 主视口与设置视口都设置了窗口图标
- [x] `assets/tray-icon.png` 为母图的 48×48 渲染，托盘加载代码不变
- [x] 全量测试与 Windows 目标 clippy（-D warnings）通过
- [ ] 真机确认：新构建 exe 在资源管理器显示新图标（待用户下轮构建）

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
