# Agent Note: 窗口投影（三）——样式自愈 + 实况日志，终结矩形暗层的归因猜测

Status: implemented

## Problem

工单 02 合并（PR #14）后用户实机复测：暗层依旧。且工单 02 的装配日志回执 `[window-style] event=strip style=0x06CF0000->0x060B0000 exstyle=0x00040110->0x00040010 result=ok` 证明装配时刻剥离确实成功（定案掩码位已清）——装配剥离没问题，问题在剥完之后发生了什么。本轮宽幅截图（1202×869）的像素量测又推翻一层归因：暗层在原生分辨率下是 1 像素硬边（x=65 壁纸 R=251 → x=66 R=103，无渐变），不透明度在白/橙/奶油三张壁纸五处量测恒为 black @ ≈150/255（白底 flat 105 = 255×(1−150/255) 精确吻合）——DWM 投影是模糊生成的，做不出 1px 硬边，它不是 DWM 投影本体。

未决嫌疑剩两个。（甲）样式被 winit 写回：winit 0.30 的十余个 setter（set_always_on_top、set_visible、set_resizable 等）都经 set_window_flags 以 to_window_styles() 整词重写 GWL_STYLE——置顶守擂每次焦点丢失重申层级都可能把剥掉的 CAPTION/SIZEBOX 挂回去；装配日志只证明剥成功过，不证明之后没被写回。（乙）窗口与相框图几何错位：暗层矩形比相框图外廓大一圈，若窗口矩形本身就比绘制矩形大，那一圈是窗口自身表面在渲染。本票不猜：装自愈 + 实况探针，一次把两个嫌疑钉死。

## Decision

- **样式自愈**：既有吸附子类化新增 WM_STYLECHANGED 处理（子类链先于 winit 自有过程看到消息；处理与吸附开关无关，处理完照常转发）。按 wParam 区分 GWL_STYLE / GWL_EXSTYLE 两路，读 STYLESTRUCT.styleNew，库 `src/window_style.rs` 新增纯判定 `style_restrip_needed` / `exstyle_restrip_needed`（复用工单 02 的剥离掩码）：目标位重现就当场 SetWindowLongPtrW 写回剥后值 + SetWindowPos(SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) 生效，写回后现读现核，前后值与成败以 `[window-style] event=restrip` 记一行（口径函数 `restrip_log_details` 与工单 02 的 log_details 一致）。位已清时零动作零日志——再剥自身同步触发的二次 STYLECHANGED 在判定处自然收敛，不设递归计数。宿主接线在 `src/bin/moe-frame/snap_subclass.rs` 的 `handle_style_changed`。
- **实况快照探针**：库 `src/window_probe.rs` 出纯判定（快照状态机 + 排版），宿主 `src/bin/moe-frame/window_probe.rs` 只实读落行。两个点位：①首次焦点丢失——与置顶守擂自检同一次焦点采样、同一帧判定（frame_ui 里共用 `focused_now`），每进程只记一次；②外观模式切换生效的那一帧——只分极简/内置/自定义三模式键（内置参数变化不算切换），每进程至多 3 次，首报只建基线。每行 `[window-probe]` 四项齐全：当前 STYLE/EXSTYLE 十六进制、GetWindowRect 窗口矩形、绘制矩形换算到屏幕物理坐标后的矩形。换算口径：相框窗口无边框、客户区与窗口矩形重合，客户区原点的屏幕物理坐标即 GetWindowRect 左上，故物理绘制矩形 = 窗口左上 + 逻辑矩形 × pixels_per_point（逐边换算后四舍五入）；绘制矩形取应用侧当帧 CentralPanel 的绘制矩形（根 Ui 的 max_rect，面板以 Frame::NONE 无边距铺满、与之同值）。读取失败（无句柄/GetWindowRect 失败）不写假数据：无句柄记一行 skipped 留痕，后者直接不写行。设置窗不监听、不探针。

## Alternatives considered

- 自愈改走定时轮询样式（守擂已有 1s 节流的轮询面）：轮询有窗口期、且要在宿主再养一份状态；WM_STYLECHANGED 是样式变更的系统事件本身，事件驱动零窗口期，弃轮询。
- restrip 时整词重算（把当前样式再跑一遍 stripped_*）而非只针对被改索引：现状正是按索引一路判定一路写回，另一路不动——winit 写回哪路就修哪路，动静最小。
- probe 的绘制矩形取九宫格实际绘制区而非面板矩形：工单口径是「应用侧当帧的相框绘制矩形」，面板矩形即整窗绘制面，与 win_rect 的同坐标系对账正为看整窗几何是否错位；取子区域反而引入第二套口径，弃。

## Consequences

- 若嫌疑（甲）成立：样式被写回的当下即被再剥，暗层不再随焦点/层级操作复活，且日志里 restrip 行与写回事件一一对账；若（甲）不成立（全程无 restrip 行而暗层仍在），probe 两行的 style 位与 win_rect/draw_rect 对账直接指向（乙）或排除两者——下一嫌疑（eframe 合成路径的表面 alpha）另票再议，本票不预先动合成层。
- 自愈挂在子类化消息路径上，与工单 02 的模态环/吸附共存：STYLECHANGED 不在模态环拦截集合内，环进行中样式被写回同样会被再剥，互不干扰。
- probe 是会话级诊断状态（与 level_guard 同口径，不落盘），上限与去重保证日志不被刷屏；本票所有新增日志在归因结案后可随工单收尾一并评估去留。

## Testing

- 库层纯判定单测新增 12 个：window_style 4 个（winit 整词重写的典型样式命中再剥、剥后样式零动作收敛、集合外位不触发、扩展样式只认 WINDOWEDGE 而 WS_EX_TOPMOST 在场不触发、restrip 日志格式以手算十六进制为独立真值）；window_probe 8 个（首次失焦只记一次、失焦起步只建基线、模式切换触发与 3 次上限、内置参数变化不算切换、绘制矩形换算的缩放/平移/整数倍三例、快照排版格式）。本票由接管审计完成：前任实现者无报告离场，红绿过程无留存记录、本轮未复跑红相；全部断言已经审计逐条核对真值（掩码位、十六进制期望、换算结果均为手算独立值），绿相数字见下。
- 全量刮擦副本 `cargo test --all-targets` 469 个全绿（基线 457 + 新增 12）；Windows 目标在 worktree 直跑 `cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check` 全绿（scripts/check.sh 的 VM 代偿口径，宿主接线含 ui_regressions 在该目标下编译校验、运行在 CI）。接管审计中另修一处前任遗漏：宿主 window_probe.rs 用 `egui::Rect` 未引入 egui 导致 Windows 目标 E0433，补 `use eframe::egui;` 一行后三项全绿。
- 自愈与探针是 Windows 运行时行为，本 VM 无法运行其实效；实机验收见 [real-machine-check-v2-frame.md](../../../../real-machine-check-v2-frame.md)「窗口投影」一节工单 03 追加条目（工单 03 置 needs-info 待用户跑，并回传 [window-style] 与 [window-probe] 全部相关行）。
