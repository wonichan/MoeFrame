# 01: 任务栏无图标与托盘三分支召回

**What to build:** 按本专题 spec 落地：相框主窗与设置窗定死不进任务栏；托盘召回从纯切换改为按焦点三分支（显示/唤回/隐藏），核心层判定、宿主采样焦点。

**Blocked by:** None (can start immediately)

**Status:** needs-info

## Acceptance criteria

- [x] 相框主窗与设置窗的视口构建器均带 `with_taskbar(false)`，任务栏不出现 MoeFrame 图标；构建期定死，无运行时开关、无设置项。
- [x] 核心 `ToggleVisibilityRequested` 携带宿主采样的焦点标志，三分支：隐藏 → `ShowFrame`；显示且有焦点 → `HideFrame`；显示且无焦点 → 新增效果 `RaiseFrame`（宿主只发聚焦命令，不改可见性状态）。核心测试钉住三分支与每步的可见性状态。
- [x] 托盘左键、托盘菜单「显示/关闭相框」、几何状态机的 ShowFrame 路径共用同一意图与焦点采样口径；焦点未知按无焦点处理（只唤回不隐藏）。
- [x] Alt+Tab 行为不变（相框显示时在列）——本票明确不处理（spec 定案）。
- [x] 全量测试绿、Windows 目标 check 通过；双轴 code-review 通过。
- [x] 实施笔记按规则落盘；工单 17 补对齐评论；实机清单工单 17 一节改三态口径并加任务栏复测项。

## Context

专题 spec：[托盘-only 化](../spec.md)（用户 2026-11-22 grill-with-docs 共识）。基线提交 `2987e32`。事实依据：egui 0.37 `ViewportBuilder::with_taskbar` 经 egui-winit 映射为 winit `skip_taskbar`，Windows 实现为 `ITaskbarList::DeleteTab`——只删任务栏标签，不动窗口样式与 Alt+Tab；该选项仅在窗口创建时生效，主窗经 NativeOptions、设置窗经其构建器在首次创建时带上即可。

## Comments

- 2026-11-22 交付（Muse 侧）：两窗构建器定死 with_taskbar(false)；核心 ToggleVisibilityRequested { focused } 三分支与新效果 RaiseFrame 落地，三处调用点（托盘左键映射、托盘菜单项、几何机 ShowFrame）共用 frame_focused 采样（None 按无焦点）。核心测试改写为三分支契约，全量 230/230 绿，Windows check 干净。双轴审查（基线 2987e32）：Spec 轴功能标准全达标；Standards 轴硬问题（托盘分发处注释仍述纯切换）已修，「召回」用词统一；内部字段 show_hide 沿用旧配对为有意保留（仅内部命名）。实施笔记：.agents/notes/implemented/feature/2026-11-22-tray-only.md（三道校验门通过）。工单 17 已补对齐评论（含父规格用户故事 53 取代记录），实机清单工单 17 节改三分支口径并新增托盘-only 复测节。状态 needs-info：待实机复测回执后转 resolved。
- 2026-11-22 手工验收回执（票 23）：托盘右键菜单「关闭相框」点了不生效。根因：菜单弹出时相框必然失焦，菜单项沿用 `ToggleVisibilityRequested { focused: false }` 被判成「只唤回聚焦」。修复：新增 `Intent::SetFrameVisibleRequested { visible }`，菜单项按标签终态执行（可见→隐藏、隐藏→显示并聚焦），左键三分支保持不变（用户明确要求保留）。`tests/app_core.rs` 新增 `the_tray_menu_item_sets_the_visibility_named_by_its_label`，check.sh 全绿。提交 `d971227` 之后。
