# 相框窗口层级任务图

## Notes

- 父规格：[spec.md](spec.md)
- 当前实现票：[01-window-layer-menu](issues/01-window-layer-menu.md)
- 单票覆盖右键菜单层级操作、会话状态与已确认的宿主 UI 测试接缝；没有依赖票。
- Windows-only UI test fixtures must use isolated temporary paths with filename-safe, per-test identifiers.

## Decisions-so-far

- 置顶请求在当前运行期间保持；重复选择当前层级恢复普通层级，直接选择另一层级则切换。
- 置底只表示操作时移到当前 Z 序底端，后续窗口仍可覆盖；层级选择不跨重启保存。
- 菜单只显示本进程最后请求的层级；自动化测试观察真实宿主菜单发出的根视口命令，Windows 原生 Z 序已另行通过实机确认。
- 实现记录：[实现决策](../../.agents/notes/implemented/feature/2026-11-22-window-layer-controls.md)；[已解决实现票](issues/01-window-layer-menu.md)。自动检查通过；Windows 原生 Z 序和重启恢复行为已由用户实机验证。

## Fog

- Frontier: no unresolved implementation tickets.
- 无未确认事项；实现工单和 Windows 实机验收均已完成。
