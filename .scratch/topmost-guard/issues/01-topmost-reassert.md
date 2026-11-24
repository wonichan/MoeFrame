# 01 置顶守擂——层级命令重申与 TOPMOST 自检日志

**What to build:** 修复「置于顶层后过一段时间被其他窗口盖住」。根因（已查
实）：层级命令全仓库只有一处发送点（frame_ui.rs 菜单点击时发一次
`WindowLevel`），之后永不重申，任何 z-order 扰动（置顶带内后激活窗口排
前、标记被摘）都不可恢复。本票给非 Normal 层级加守擂：当相框窗口失去焦
点（带内排序变化的时刻）时重申一次当前层级命令；重申前自检本窗
WS_EX_TOPMOST 标记是否还在，并记一行日志区分「标记被摘」与「带内重
排」，仅在观察状态变化时记，避免刷屏。重申走既有 viewport 命令通路
（不激活窗口、不抢焦点）。Normal 层级不产生任何重申流量。

**Blocked by:** None（基线 main @ 417f32d，PR #11 吸附已合入；本票不改吸
附行为，只与之共存）。

**Status:** needs-info

## Acceptance criteria

- [x] 层级为 AlwaysOnTop 时，相框窗口失去焦点后层级命令被重申（回归测试钉住，沿用 ui_regressions 的 FullOutput 断言口径）
- [x] 层级为 AlwaysOnBottom 时同理重申；层级为 Normal 时任何情况下零重申（既有测试全绿不回归）
- [x] 重申有节流：持续失焦/反复唤醒不产生命令风暴（纯函数判定 + 测试断言）；判定逻辑放在可单测的纯函数里
- [x] 自检日志：重申前读本窗 WS_EX_TOPMOST，标记缺失记一行、标记在但发生重申记一行（或等价区分），仅状态变化时记录；日志走仓库既有日志口径
- [x] 不与吸附子类化（WM_MOVING）及几何机冲突：不新增 SetWindowPos 直调，吸附/拖动/切图既有测试全绿
- [x] `scripts/check.sh` 口径全绿（fmt / clippy -D warnings / test --all-targets；VM 代偿口径如实记入 Comments）
- [x] 实机清单新增「置顶守擂」一节：置顶后依次点击普通窗口、另一置顶窗口、最小化/恢复其他窗口、轮播切图数轮，相框始终在顶层；托盘隐藏/召回后置顶仍有效
- [x] 实现笔记落 `.agents/notes/implemented/` 并通过三道校验门

## Context

- 诊断来源：2026-11-24 用户实机截图与反馈（置顶一段时间后被左侧窗口覆盖）；根因定位见本票 What to build
- 既有先例：失焦模糊修复的 heal 节拍（next_frame_delay 纯函数）——低频守擂不引入新定时器，挂靠既有唤醒点（焦点事件、轮播滴答等自然重绘）
- 层级状态：宿主 app.rs 的 window_level 字段（会话级、不落盘），菜单切换逻辑在 frame_ui.rs

## Comments

- 2026-11-24 实现完成（分支 fix/topmost-guard，功能提交 7a5cc6c）：
  库层新增纯判定 `level_guard::LevelGuard`——非 Normal 层级在失焦
  边沿触发重申，节流下限 1000ms、被压住的重申挂起待到期补发（回
  焦或切回 Normal 作废）；宿主 frame_ui 在菜单层级应用后每帧采样
  焦点轮询，命中时先以 Win32 读 WS_EX_TOPMOST 自检、按分类变化经
  `[level-guard]` 前缀记一行日志（MarkMissing / BandReorder），再
  经既有 viewport 命令重申，不新增 SetWindowPos 直调。
- 验证（VM 代偿口径，如实记录）：`cargo fmt --check` 通过；
  Windows 目标 `cargo check --all-targets` 与 `cargo clippy
  --all-targets -- -D warnings` 通过；测试以 Linux 刮擦副本（剥
  rfd/tray-icon、启用 wayland/x11）跑 `cargo test --all-targets`：
  432 passed / 0 failed（含 level_guard 新增 8 个纯判定单测）。
  宿主 bin 测试（含本次新增 ui_regressions 守擂回归）受产品 bin
  Windows-only 限制，在本 VM 只能编译校验，实际运行在 CI
  （windows-latest 跑 scripts/check.sh）。
- 笔记校验门：tree 与 archived 通过；format 门本笔记无问题，但仓
  库整体有一处既有失败（implemented/feature/2026-11-24-wm-moving-
  snap.md，吸附线 af4d696 引入、非本票范围），未代为修改。
- Status 置 needs-info：待用户按实机清单新增「置顶守擂」一节实机
  验收后回填。
