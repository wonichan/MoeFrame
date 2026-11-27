# Agent Note: 窗口投影（二）——剥掉相框窗口的边框样式，根除窗口矩形暗层

Status: implemented

## Problem

工单 01 的 DWM 关闭调用实机回执 `[dwm-shadow] event=disable result=ok hresult=0x00000000`——调用成功落地，但用户复测（2026-11-27 宽幅截图）暗层依旧。像素量测：暗层为均匀 ×0.41 乘暗、硬边、几何与窗口矩形重合，与首报白底截图的 flat 105 灰带同源（255 × 0.41 ≈ 105）。

根因定位：winit 0.30 对无边框可缩放窗口**恒置** `WS_CAPTION | WS_BORDER | WS_SYSMENU` 并按可缩放加 `WS_SIZEBOX`、扩展样式带 `WS_EX_WINDOWEDGE`（vendored 源码 window_state.rs 的 `to_window_styles`，注释自陈为支持 aero snap 所需），无边框外观只靠 WM_NCCALCSIZE 把客户区铺满实现——DWM 因此仍按「带框窗口」给它画整圈框架影；工单 01 的 `DWMNCRP_DISABLED` 管的是非客户区渲染策略，管不到这一路。既有的 `set_undecorated_shadow(false)` 同理只决定 winit 要不要那条 1px 非客户区窄边，与框架影无关。

## Decision

- 装配后（吸附子类化挂好之后）对相框窗口读改写 GWL_STYLE / GWL_EXSTYLE，剥掉 `WS_CAPTION | WS_SIZEBOX | WS_BORDER` 与 `WS_EX_WINDOWEDGE`，以 SetWindowPos(SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) 生效；写回后现读现核，前后样式值与成败以 `[window-style]` 前缀记一行 moe_frame.log。纯判定（与 Win32 文档对齐的常量、剥离计算、日志口径）在库 `src/window_style.rs`，宿主 `src/bin/moe-frame/window_style.rs` 只接线。设置窗不碰；工单 01 的 DWM 调用与 `[dwm-shadow]` 日志原样保留作基线对照。
- 连带接管（剥样式的必然后果）：DefWindowProc 的移动/缩放模态环依赖这组样式，剥后失效——egui 的 StartDrag / BeginResize 经 winit 投递 WM_NCLBUTTONDOWN 后正落到该环。既有吸附子类化新增拦截：命中 HTCAPTION 与八个 HT 缩放码时吞掉消息、自研模态环接管，要点有三：
  - **生命周期与原生环同构**：进入/退出经子类链送 WM_ENTERSIZEMOVE / WM_EXITSIZEMOVE。吸附会话的组装/收尾走既有分支；winit 在 EXITSIZEMOVE 里复位其 dragging 旗标并合成 WM_LBUTTONUP——应用层的松手感知（input.rs 的 PointerUp、几何机的松手重校正与 GetAsyncKeyState 轮询）与原生路径同源，不另造通知。真实的 WM_LBUTTONUP 被环消费不分发，与 DefWindowProc 消费真实抬键同构（winit 正是因此才需要合成那一记）。
  - **环内推进**：SetCapture 后泵本线程消息；光标移动时以「起始矩形 + 起始光标 + 当前光标」算拟议矩形（库 `src/modal_drag.rs` 纯判定：移动整窗跟随、缩放被拖边跟随对边锚定、最小尺寸钳制），流经与 WM_MOVING 完全同一条吸附调整（从 handle_moving 抽出 `adjust_drag_frame` 共用一份实现：伙伴刷新、求解、逐帧卫生、关系记录、HDWP 整组搬运），SetWindowPos 落位。其余消息一律翻译 + 分发，设置窗与定时器/绘制消息照常流转；环不重入，捕获没拿到手直接放弃本次环。
  - **终止语义**：本窗 WM_LBUTTONUP 按现位结束；Esc 与 WM_CANCELMODE 恢复起始矩形后结束；WM_CAPTURECHANGED 按现位结束；WM_QUIT 重新投回队列让外层事件循环照常收尾。
- 拟议矩形与既有口径一致——只是提议：应用层的高宽派生/回正在后续帧照常收敛，环里不另写尺寸规则。钳制只补一项：最小尺寸按仓库既有常量 MIN_WINDOW_SIDE（逻辑 120px 按本窗 DPI 换算物理像素），这是 DefWindowProc 曾以最小跟踪尺寸代劳的；最大跟踪尺寸仓库无既有常量，未复刻。

## Alternatives considered

- 只剥样式、拖动改走应用层 egui 手势（弃用 StartDrag / BeginResize）：改动横跨输入/几何两层，且与多实例吸附的 WM_MOVING 语义脱钩，弃。
- 模态环不发 ENTER/EXITSIZEMOVE、直接调会话组装/收尾函数：winit 的 dragging 旗标只在 EXITSIZEMOVE 复位、其合成 LBUTTONUP 是应用层松手感知的唯一来源，绕开它要逐项复刻且易漏；发消息走既有分支是单一路径，弃直调。
- 剥样式集合扩大到 WS_SYSMENU：工单定案集合不含它（winit 注释称该组样式为 aero snap 所需，SYSMENU 与框架影无涉），不扩围。

## Consequences

- 相框窗口成为真正的无框窗口，DWM 不再按带框窗口画框架影，矩形暗层根除（实机清单复核）；两条日志（[window-style]、[dwm-shadow]）并存对账。
- 拖拽移动与八向缩放改由自研模态环驱动，吸附跟随/关系记录/整组搬运与剥样式前同源同路径；拦截与吸附开关解耦——吸附关时拖动/缩放照常（环不带会话、矩形不调整），与原生环时期的行为一致。
- DefWindowProc 模态环的附带行为（如拖到屏幕边缘的系统级 Snap Assist）不复刻：本窗几何由应用派生收敛、吸附有自家语义，未列入工单口径。
- Esc 恢复只恢复本窗矩形：环内已被 HDWP 搬走的伙伴不回退——与原生环时期 Esc 取消的既有行为一致（WM_MOVING 帧同样已搬运伙伴），不另立口径。

## Testing

- 库层纯判定单测 15 个：window_style 5 个（常量与 Win32 文档对齐、winit 典型样式的剥离结果、幂等、扩展样式剥离、日志格式以手算十六进制为独立真值）；modal_drag 10 个（九个命中码映射、无关码不拦截、移动跟随、三类边/角的跟随与锚定、三向最小尺寸钳制）。TDD 红→绿在 Linux 刮擦副本真跑（先 14 红、后全绿；样式常量测试因常量即声明直接绿）。
- 全量刮擦副本 `cargo test --all-targets` 455 个全绿（基线 440 + 新增 15）；Windows 目标在 worktree 直跑 `cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check` 全绿（scripts/check.sh 的 VM 代偿口径，宿主接线含 ui_regressions 在该目标下编译校验、运行在 CI）。
- 宿主的样式剥离与模态环是 Windows 运行时行为，本 VM 无法运行其实效；实机验收见 [real-machine-check-v2-frame.md](../../../../real-machine-check-v2-frame.md)「窗口投影」一节追加条目（工单 02 置 needs-info 待用户跑）。
