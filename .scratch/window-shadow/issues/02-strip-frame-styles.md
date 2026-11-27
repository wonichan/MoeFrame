# 02 剥掉相框窗口的边框样式，根除窗口矩形暗层

**What to build:** 工单 01 的 DWM 关闭调用实机验证不达预期：日志
`[dwm-shadow] event=disable result=ok hresult=0x00000000`——调用成功
落地，但暗层依旧（用户 2026-11-27 宽幅截图复测，像素量测：暗层为均
匀 ×0.41 乘暗、硬边、几何与窗口矩形重合，与首报白底截图的 flat 105
灰带同源）。根因定位：winit 0.30 对无边框可缩放窗口**恒置**
`WS_CAPTION | WS_BORDER | WS_SYSMENU` 并按可缩放加 `WS_SIZEBOX`
（vendored 源码 window_state.rs `to_window_styles`，注释自陈为支
持 aero snap 所需），无边框外观只靠 WM_NCCALCSIZE 把客户区铺满实
现——DWM 因此仍按「带框窗口」给它画整圈框架影，NCRENDERING_POLICY
管不到这一路。修法：相框窗口装配后剥掉这组边框样式
（`WS_CAPTION | WS_SIZEBOX | WS_BORDER` 与扩展样式
`WS_EX_WINDOWEDGE`），SetWindowPos(SWP_FRAMECHANGED) 生效。
**连带后果必须一并接管**：egui BeginDrag/BeginResize 经 winit 落到
`WM_NCLBUTTONDOWN(HTCAPTION/HT*)` → DefWindowProc 的移动/缩放模
态环，该环依赖这组样式，剥后失效——在既有吸附子类化里拦截这批
HT 码，自行跑模态环（SetCapture + 跟随光标 + SetWindowPos），并让
环内矩形继续流经既有吸附的矩形调整逻辑（WM_MOVING/WM_SIZING 既有
语义不丢）。Esc 取消并恢复原矩形（DefWindowProc 语义）。

**Blocked by:** 工单 01（已合并 PR #13，其 DWM 调用保留不动作基线）。

**Status:** ready-for-agent

## Acceptance criteria

- [ ] 装配后相框窗口边框样式被剥离（GetWindowLongPtr 前后值以 `[window-style]` 前缀记一行日志：剥前/剥后十六进制 + 成败），设置窗样式不被触碰
- [ ] 拖拽移动与八向边缘缩放经子类化自研模态环工作：环内矩形流经既有吸附矩形调整（多实例吸附跟随/吸附语义与剥样式前一致），Esc 恢复原矩形，松手落位
- [ ] 既有机制不回归：置顶守擂自检与重申、托盘隐藏/召回、转场动画、8px 缩放带命中判定（input.rs 方向判定不变）、工单 01 的 [dwm-shadow] 日志仍在
- [ ] 纯判定可测部分（样式位掩码计算、模态环的矩形跟随/锚定/吸附调整）有单测；`scripts/check.sh` 口径全绿（fmt / clippy -D warnings / test --all-targets；VM 代偿口径如实记入 Comments）
- [ ] 实机清单「窗口投影」一节追加：亮色壁纸宽幅截图下窗口矩形暗层消失；拖拽移动、四边四角缩放、多实例吸附跟随如常；重启后仍无暗层
- [ ] 实现笔记落 `.agents/notes/implemented/` 并通过三道校验门；工单 01 的 Comments 补记本票为后备路线的正式承接

## Context

- 证据链：工单 01 Comments 的诊断补正 + 2026-11-27 实机日志行（result=ok）+ 宽幅截图量测（×0.41 均匀乘暗、硬边、矩形几何）
- winit 0.30.13 vendored 源码：window_state.rs `to_window_styles`（样式恒置）、event_loop.rs WM_NCCALCSIZE（MARKER_UNDECORATED_SHADOW 关时客户区铺满）、window.rs `drag_window`/`drag_resize_window` → `handle_os_dragging`（WM_NCLBUTTONDOWN 路径）
- 既有子类化与矩形调整：吸附专题的 WndProc 子类（WM_MOVING leader 跟随）与 level_guard 自检的 HWND 取得路径；样式剥离与模态环都挂在同一装配点附近
- 本票不动工单 01 的 DWM 调用（已证明无害且日志可复用作基线对照）

## Comments

（暂无）
