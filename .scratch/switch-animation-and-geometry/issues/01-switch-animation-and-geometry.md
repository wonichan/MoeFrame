# 01: 切图交叉淡化、相框几何动画与设置动画选项

**What to build:** 按本专题 spec 落地：切图 240ms 交叉淡化；窗口尺寸与
位置同步平滑动画；以名义中心锚定，宽图被工作区钳住不改变中心；工作区
6px 边距；超宽临时缩高不落盘；设置窗新增「切换动画」两档；修复
Windows 原生拖动后窗口被拉回的回归。

**Blocked by:** None

**Status:** done

## Acceptance criteria

- [x] 切图使用 240ms 三次 ease-out 交叉淡化，旧图与新图按各自比例 fit，
      不拉伸；`animation = "none"` 立即完成；`slide`/`fade` 都按交叉淡化；
      首图与重扫首图不动画。
- [x] 窗口尺寸与位置共用同一时间轴插值，终点无中间尺寸落盘；位置逐帧
      按当前尺寸做工作区钳制。
- [x] 名义中心稳定：宽图被屏幕边缘钳住后，切回窄图仍回到原中心；
      多次横竖切换不漂移。
- [x] 工作区从 `GetMonitorInfoW` 的 `rcWork` 取，物理像素按 DPI 换算；
      越界边压到 6px 边距；取不到工作区时退回旧口径。
- [x] 超宽时临时降低显示高度，只改显示不写回配置；切回竖图或重启恢复
      名义高度。
- [x] 设置窗「切换动画」两档：无写回 `none`，交叉淡化写回 `fade`；
      旧 `slide` 显示为交叉淡化且不触发重写。
- [x] Windows 原生拖动/缩放期间丢失指针状态后，松手第一帧不回旧位置；
      用户拖动/缩放可立即打断几何动画并接管。
- [x] 自动化：`window_geom` 32 项、`settings_session` 27 项、`config`
      30 项、`window_validation` 13 项全过；`scripts/check.sh` 全绿。
- [x] 真机：用户逐项验收交叉淡化、窗口尺寸/位置、中心稳定、6px 边距、
      临时缩高、配置与落盘、刷新与资源、回归；两个后续回归修复后用户
      确认可用。

## Context

父规格：[切图动画与相框几何稳定规格](../spec.md)。

关联历史工单：
[V1 03 设置窗边界](../../v1-exe/issues/03-settings-boundary.md)、
[V1 14 滑动切换](../../v1-exe/issues/14-slide-transitions.md)。
V2 相框自动定宽见 [V2 规格](../../v2-frame/spec.md) 用户故事 9。

## Comments

- 2026-11-22 实现完成：宿主转场从「立即回送完成」改为 240ms 交叉
  淡化；`window_geom` 增加几何动画、名义中心与工作区钳制；新增
  `win32_monitor` 读取工作区；设置窗增加动画两档。决策笔记：
  `.agents/notes/implemented/feature/2026-11-22-switch-animation-and-geometry.md`。
- 2026-11-22 实机验收：用户确认八项全部通过，随后报告两个回归——
  「无法拖动窗口」与「设置窗口没有可供选择的动画效果」。根因是
  Windows 原生拖动模态循环期间 egui 收不到指针状态，松手第一帧按旧
  名义中心把窗口拉回；设置窗当时确实没有动画控件。
- 2026-11-22 回归修复：宿主增加 `native_gesture_active` 并在
  `StartDrag`/`BeginResize` 后用物理左键状态兜底；设置窗增加「无 /
  交叉淡化」两档并落盘。新增 `window_geom` 原生拖动丢事件测试、
  `settings_session` 动画点选测试、`config` 动画落盘测试。
- 2026-11-22 收口：`scripts/check.sh` 全绿（window_geom 32、
  settings_session 27、config 30、window_validation 13）；release
  `target/release/MoeFrame.exe` 重建成功，SHA-256
  `fe42b210b6094767c0a6908bab3f56f0e25449d5b6c27407b730415cd9de5a85`。
  用户确认可用，状态置 done。
## 2026-11-23 收编注记

本工单的专题已被 `.scratch/transition-animation` 专题收编取代（用户拍板以 PR #4 为准）：交叉淡化收编为「淡化」档保留；几何平滑动画与名义中心锚定撤除；回弹修复、名义高度/临时缩高不落盘、工作区观察通路保留。逐项清单见 transition-animation 工单 05。本工单保留为历史记录，状态不再单独推进。
