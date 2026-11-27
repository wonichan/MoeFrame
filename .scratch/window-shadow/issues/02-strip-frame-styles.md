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

**Status:** needs-info

## Acceptance criteria

- [x] 装配后相框窗口边框样式被剥离（GetWindowLongPtr 前后值以 `[window-style]` 前缀记一行日志：剥前/剥后十六进制 + 成败），设置窗样式不被触碰
- [x] 拖拽移动与八向边缘缩放经子类化自研模态环工作：环内矩形流经既有吸附矩形调整（多实例吸附跟随/吸附语义与剥样式前一致），Esc 恢复原矩形，松手落位
- [x] 既有机制不回归：置顶守擂自检与重申、托盘隐藏/召回、转场动画、8px 缩放带命中判定（input.rs 方向判定不变）、工单 01 的 [dwm-shadow] 日志仍在
- [x] 纯判定可测部分（样式位掩码计算、模态环的矩形跟随/锚定/吸附调整）有单测；`scripts/check.sh` 口径全绿（fmt / clippy -D warnings / test --all-targets；VM 代偿口径如实记入 Comments）
- [x] 实机清单「窗口投影」一节追加：亮色壁纸宽幅截图下窗口矩形暗层消失；拖拽移动、四边四角缩放、多实例吸附跟随如常；重启后仍无暗层
- [x] 实现笔记落 `.agents/notes/implemented/` 并通过三道校验门；工单 01 的 Comments 补记本票为后备路线的正式承接

## Context

- 证据链：工单 01 Comments 的诊断补正 + 2026-11-27 实机日志行（result=ok）+ 宽幅截图量测（×0.41 均匀乘暗、硬边、矩形几何）
- winit 0.30.13 vendored 源码：window_state.rs `to_window_styles`（样式恒置）、event_loop.rs WM_NCCALCSIZE（MARKER_UNDECORATED_SHADOW 关时客户区铺满）、window.rs `drag_window`/`drag_resize_window` → `handle_os_dragging`（WM_NCLBUTTONDOWN 路径）
- 既有子类化与矩形调整：吸附专题的 WndProc 子类（WM_MOVING leader 跟随）与 level_guard 自检的 HWND 取得路径；样式剥离与模态环都挂在同一装配点附近
- 本票不动工单 01 的 DWM 调用（已证明无害且日志可复用作基线对照）

## Comments

- 2026-11-27 实现完成（分支 fix/window-shadow-2）：装配处（吸附
  子类化挂好之后）新增 `window_style::strip(hwnd)`——读改写
  GWL_STYLE / GWL_EXSTYLE 剥 `WS_CAPTION | WS_SIZEBOX |
  WS_BORDER` 与 `WS_EX_WINDOWEDGE`，SetWindowPos
  (SWP_FRAMECHANGED) 生效，写回后现读现核，前后值与成败以
  `[window-style]` 前缀记一行 moe_frame.log；纯判定（常量/剥
  离计算/日志口径）在库 `src/window_style.rs`（5 个单测）。
  设置窗未碰；工单 01 的 DWM 调用与 `[dwm-shadow]` 日志原
  样保留。
- 模态环接管：snap_subclass 新增 WM_NCLBUTTONDOWN 拦截
  （HTCAPTION + 八个 HT 缩放码，库 `src/modal_drag.rs` 出
  命中分类与矩形纯判定，10 个单测）。进入/退出经子类链送
  WM_ENTERSIZEMOVE / WM_EXITSIZEMOVE，与原生环同构：吸
  附会话组装/收尾走既有分支，winit 的 dragging 旗标复位与
  合成 WM_LBUTTONUP 原样发生（真实抬键被环消费，与
  DefWindowProc 同构）；环内 SetCapture 泵消息、光标移动
  算拟议矩形、流经从 handle_moving 抽出的 `adjust_drag_frame`
  （与 WM_MOVING 共用一份吸附调整）、SetWindowPos 落位；
  Esc / WM_CANCELMODE 恢复起始矩形，WM_CAPTURECHANGED
  按现位结束，环不重入、捕获没拿到手放弃本次环。拦截与吸
  附开关解耦：吸附关时环照常工作、矩形不调整。
- 尺寸衔接：拟议矩形只是提议，应用层高宽派生/回正在后续帧
  照常收敛，环里不另写尺寸规则。最小尺寸钳制按仓库既有常
  量 MIN_WINDOW_SIDE（逻辑 120px 按本窗 DPI 换算物理）补
  齐（DefWindowProc 曾以最小跟踪尺寸代劳）；最大跟踪尺寸
  仓库无既有常量，未复刻，如实记录。DefWindowProc 附带的
  系统级 Snap Assist 不复刻（本窗几何由应用派生、吸附有自
  家语义，未列入本票口径）。
- 验证（VM 代偿口径，如实记录）：库层单测 15 个 TDD 红→
  绿在 Linux 刮擦副本真跑（先 14 红后全绿）；刮擦副本
  `cargo test --all-targets` 455 个全绿（基线 440 + 新增
  15）；Windows 目标在 worktree 直跑 `cargo check
  --all-targets`、`cargo clippy --all-targets -- -D warnings`、
  `cargo fmt --check` 全绿。宿主样式剥离与模态环是
  Windows 运行时行为，本 VM 无法运行其实效，由实机清单
  「窗口投影」一节追加条目验收，本票置 needs-info。
- 实现笔记 `.agents/notes/implemented/bug-fix/2026-11-27-
  window-style-strip.md` 三道校验门：tree 通过、archived 通
  过、format 门失败名单恰为仓库既有 4 篇（crossfade-merge、
  multi-instance-snap-review-fixes、leader-driven-group、
  wm-moving-snap），本笔记不在名单、未代改。
