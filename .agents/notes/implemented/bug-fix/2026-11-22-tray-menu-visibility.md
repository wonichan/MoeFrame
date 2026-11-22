# Agent Note: 托盘菜单「关闭相框」点不动——菜单项不再参与焦点三分支

Status: implemented

## Problem

用户手工验收（票 23）报告：托盘右键 → 「关闭相框」，窗口不关。

[托盘-only 化](../feature/2026-11-22-tray-only.md) 的决定是「托盘左键、托盘菜单项、几何机
ShowFrame 三处共用同一意图与焦点采样口径」，宿主在菜单事件处理帧调用
`frame_focused(ctx)` 喂给 `ToggleVisibilityRequested { focused }`。问题在于：
**托盘菜单弹出时，相框一定不是前台窗口**，所以这条路径采到的 `focused`
永远是 false，于是核心走「可见但无焦点 → 只唤回聚焦」，`HideFrame` 从不发出。
菜单项标签写着「关闭相框」，行为却是抬一下，等于点了没反应。

左键不是这个形状：点托盘图标时相框可能仍持有焦点，三分支能正常收敛；用户也
明确要求保留左键三分支（`m01796` 问答回执）。

## Decision

只拆菜单项这一处，左键三分支原样保留：

- 新增 `Intent::SetFrameVisibleRequested { visible: bool }`：按给定终态执行——
  要可见且当前隐藏 → `ShowFrame`；要隐藏且当前可见 → `HideFrame`；已可见再
  要"显示" → `RaiseFrame`；已隐藏再要"关闭" → 无动作。
- 宿主菜单项分支改成 `SetFrameVisibleRequested { visible: !core.frame_visible() }`，
  不再采样焦点；菜单文案（关闭/显示相框）与提交的终态由同一份
  `frame_visible()` 推导，天然一致。
- `.scratch/tray-only/spec.md` 与工单 01 同步改写：三分支只约束左键，菜单项按
  标签终态执行。

## Alternatives considered

- 菜单事件里先 `Focus` 再下一帧发 `ToggleVisibilityRequested`：要跨帧等待焦点
  真的落到相框上，多一帧状态机、还要处理"焦点始终没来"的分支，不取。
- 菜单项也传 `focused: true`（假定想要隐藏）：等于让菜单项无条件隐藏，但
  隐藏态的菜单项标签是「显示相框」，同一意图两种语义，正是这次的病根，不取。
- 把左键三分支一起改成纯开关：用户明确要求保持现状（`m01796`），且三分支的
  「别误藏」在左键路径上确实成立（点图标时窗口可能还有焦点），不取。

## Testing

- `tests/app_core.rs` 新增 `the_tray_menu_item_sets_the_visibility_named_by_its_label`：
  覆盖四条终态（关闭→隐藏、显示→显示并聚焦、已可见再显示→只唤到最前、
  已隐藏再关闭→无动作）。
- `scripts/check.sh` 全绿（fmt + clippy -D warnings + 全量测试）。
- 未验证：真实菜单点击（合成点击只能打到托盘窗口，打不到 muda 的菜单窗口），
  交由用户在实机复测。

## Consequences

- 「三处共用同一意图」这条口径被部分推翻：左键与几何机 ShowFrame 仍走焦点
  三分支，托盘菜单项走终态意图。今后任何「菜单/按钮上写着动作名」的入口都
  应给终态，而不是复用依赖焦点的切换意图。
- 两条托盘相关笔记现在描述的是不同入口，读的时候别把三分支套到菜单项上。