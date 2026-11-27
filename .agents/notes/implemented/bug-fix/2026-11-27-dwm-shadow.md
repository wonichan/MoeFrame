# Agent Note: 窗口投影——关掉 DWM 给相框窗口加的系统投影

Status: implemented

## Problem

用户实机反馈（2026-11-27 截图）：自定义哥特异形相框套用后，相框四周有一圈圆角矩形、比相框大一圈、微微下沉的灰影带。排查查实：相框图 PNG 本身干净（合成到品红/深绿底上外轮廓无光晕），应用也不给自定义模式画阴影（FR-315 口径）；影带是 Windows 11 的 DWM 给相框窗口本体加的系统投影——它跟窗口矩形走、不跟相框异形轮廓走，用矩形内置框时藏在框边上看不见，换透明异形框就露馅。

一处诊断补正：主会话排查时 grep 范围漏了 `src/bin/moe-frame.rs` 本体——创建闭包里自工单 10 起就有一行 winit 的 `set_undecorated_shadow(false)`，并非「宿主无任何 shadow 处理」。但该开关只让 winit 不主动索取投影（不把边框扩进客户区），挡不住 DWM 给可缩放无边框窗口画的投影；用户截图正是取自含该调用的构建，这也是本票改走 DWM 层面关闭的原因。

## Decision

- 宿主在装配处（创建闭包取得相框 HWND、挂完吸附子类化之后）对相框窗口调一次 `dwm_shadow::disable(hwnd)`：内部以 windows-sys 的 `DwmSetWindowAttribute(DWMWA_NCRENDERING_POLICY, DWMNCRP_DISABLED)` 关掉非客户区渲染，投影随之消失，落实 FR-313「不使用原生窗口阴影」。仅相框窗口、仅一次；设置窗是另一个 HWND，不在此处、不动。
- 纯判定放库层 `src/dwm_shadow.rs`（仿 level_guard 先例）：与 Win32 文档对齐的常量（属性号 2、策略值 1）与 HRESULT 的日志口径（`result=ok/failed hresult=0x…`），单测以文档值为独立真值钉住；宿主模块只接线，属性号直接传库常量，与 windows-sys 自带常量同值。
- 调用结果经新增的 `dwm_shadow_record` 向 moe_frame.log 写一行 `[dwm-shadow]`（与启动标记同文件同格式、独立前缀）：成功记 result=ok，失败记 result=failed 与十六进制码，句柄未就绪记 event=skipped。失败与跳过都只记日志，不影响启动与任何既有行为。
- windows-sys 依赖新增 `Win32_Graphics_Dwm` 特性（dwmapi 由 windows-sys 的链接声明直引，无新增依赖）。

## Alternatives considered

- 摘除 WS_THICKFRAME 等窗口样式位：可能影响 winit 的缩放/吸附语义，面更大；只作后备——若实机确认 DWMNCRP_DISABLED 不足以去掉投影（日志 result=ok 但影带仍在），另票评估，本票不预先实现（工单 Comments 已记）。
- 只靠既有 `set_undecorated_shadow(false)`：已被实机截图证伪（见 Problem 的诊断补正），保留该行不删——它与本票互补（一个管 winit 索取、一个管 DWM 绘制），删它无收益。
- 按外观模式条件关闭（只在自定义模式关）：投影是窗口级属性、窗口在三种模式间不重建，模式切换时反复开关 DWM 属性徒增状态；一律关闭与 FR-313 对内置框「不使用原生窗口阴影」的口径正好一致。

## Consequences

- 相框窗口在三种外观模式下都不再带 DWM 投影；内置画框的软阴影本就由绘制层落在框体内侧（FR-313），不依赖原生投影，观感口径不变。
- DWMNCRP_DISABLED 关的是非客户区渲染：相框窗口本就无边框无标题栏，没有可失去的非客户区元素；透明合成、8px 自绘缩放带（客户区内命中测试）、WM_MOVING 子类化、置顶守擂都是客户区/消息层机制，不受影响（既有测试全绿，实机清单复核）。
- 设置窗保留普通窗口的系统投影，外观如常。

## Testing

- 库层纯判定由 `src/dwm_shadow.rs` 内单测钉住（6 个）：属性号/策略值与 Win32 文档一致、S_OK 为成功、负 HRESULT 为失败、成功与失败的日志明细格式。TDD 红→绿在 Linux 刮擦副本真跑。
- 全量刮擦副本 `cargo test --all-targets` 440 个全绿（基线 434 + 新增 6）；Windows 目标在 worktree 直跑 `cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check` 全绿（scripts/check.sh 的 VM 代偿口径，宿主接线含 ui_regressions 在该目标下编译校验、运行在 CI）。
- 宿主的 Dwm 调用是 Windows 一次性接线，本 VM 无法运行其实效；实机验收见 [real-machine-check-v2-frame.md](../../../../real-machine-check-v2-frame.md) 新增「窗口投影」一节（工单 01 置 needs-info 待用户跑）。
