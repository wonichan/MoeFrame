# 03 样式自愈 + 实况日志：终结矩形暗层的归因猜测

**What to build:** 工单 02 合并（PR #14）后用户实机复测：暗层依旧
（2026-11-27 晚宽幅截图，横屏）。像素量测把归因再推翻一层：暗层在
原生分辨率下是 **1 像素硬边**（x=65 壁纸 R=251 → x=66 R=103，无渐
变），且不透明度在五处独立量测（白/橙/奶油三张壁纸、上下左右）恒
为 **black @ ≈150/255**（白底 flat 105 = 255×(1−150/255) 精确吻
合）——DWM 投影是模糊生成的，做不出 1px 硬边，它不是 DWM 投影本
体。当前两个未决嫌疑，本票一次钉死：
（甲）**样式被 winit 写回**：winit 0.30 的十余个 setter（set_always_
on_top 等）都经 set_window_flags 以 to_window_styles() **整词重写**
GWL_STYLE——置顶守擂每次焦点丢失重申层级都可能把工单 02 剥掉的
CAPTION/SIZEBOX 挂回去；工单 02 的 [window-style] 日志只证明装配
时刻剥成功，不证明之后没被写回。
（乙）**窗口与相框图几何错位**：暗层矩形比相框图外廓大一圈，若实
况日志显示窗口矩形就比绘制矩形大，那一圈是窗口自身表面在渲染。
做法：①子类化监听 WM_STYLECHANGED，目标位（工单 02 的剥离集合）
一旦出现即再剥并以 [window-style] event=restrip 记一行（自触发的二
次 STYLECHANGED 因位已清而自然收敛，防递归/防刷屏）；②实况快照
日志：在首次焦点丢失（与守擂自检同点位）与外观模式切换时，以
[window-probe] 记一行——当前 STYLE/EXSTYLE 十六进制、GetWindowRect
窗口矩形、应用侧当帧的相框绘制矩形（物理像素，同坐标系）。

**Blocked by:** 工单 02（已合并 PR #14）。

**Status:** needs-info

## Acceptance criteria

- [x] WM_STYLECHANGED 自愈：目标位重现时再剥 + SetWindowPos(FRAMECHANGED)，一行 restrip 日志；位已清时零动作零日志（收敛性有单测或等价纯判定覆盖）
- [x] [window-probe] 快照在首次焦点丢失与外观切换两点位各记一行（同点位不重复刷屏）：STYLE/EXSTYLE、窗口矩形、绘制矩形四项齐全
- [x] 既有行为不回归：工单 02 的模态环/吸附/样式剥离、工单 01 的 DWM 日志、置顶守擂全绿；设置窗不被监听不被剥
- [x] `scripts/check.sh` 口径全绿（fmt / clippy -D warnings / test --all-targets；VM 代偿口径如实记入 Comments）
- [x] 实机清单「窗口投影」追加：启动后切走焦点再切回、切换一次外观模式，各截宽图看暗层；并回传 [window-style] 与 [window-probe] 全部相关行
- [x] 实现笔记落 `.agents/notes/implemented/` 并通过三道校验门

## Context

- 量测原始数据：2026-11-27 晚截图 image-e5b73877.png（1202×869）：左 transect y=430 步进点 x=66；上 x=600 步进点 y=15；下 x=600 步进点 y≈810；暗层矩形约 x∈[66,1103]、y∈[15,810]
- winit 写回路径：vendored winit-0.30.13 window.rs 十余处 set_window_flags 调用（set_always_on_top、set_visible、set_resizable 等皆经此路整词重算样式）
- 守擂自检已有运行期 HWND 取得与样式读取先例（level_guard 的 GetWindowLongPtrW 自检），probe 点位与之同处最省接线
- 若本票实测排除（甲）（乙），下一嫌疑是 eframe 合成路径的表面 alpha，另票再议；本票不预先动合成层

## Comments

- 2026-11-27 本侧完工（接管审计补完）：前任实现者中途离场留下未提交工作树，接管者逐项审计 diff 后认收，并修一处编译缺陷——宿主 `src/bin/moe-frame/window_probe.rs` 用 `egui::Rect` 未引入 egui，Windows 目标 E0433，补 `use eframe::egui;` 一行后全绿。自愈接线：子类化在吸附开关之前处理 WM_STYLECHANGED（与吸附解耦），STYLE/EXSTYLE 两路按 STYLESTRUCT.styleNew 判定，再剥写回后现读现核、仅动作时记一行 restrip；probe 两点位与守擂共用同一次焦点采样，外观首报只建基线、切换至多记 3 次。
- 验证（VM 代偿口径，scripts/check.sh 不能在 VM 原样跑）：刮擦副本（自本 worktree 刷新，剥 rfd/tray-icon、启用 wayland/x11）`cargo test --all-targets` **469 passed / 0 failed**（基线 457 + 新增 12：window_style 自愈判定 4 + window_probe 8）；Windows 目标在 worktree 直跑 `cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check` 全绿。宿主自愈/probe 接线在 Windows 目标下编译校验、运行在 CI；其实效（restrip 是否出现、probe 两行对账）待实机清单回传判定，Status 置 needs-info。
- 实现笔记 `.agents/notes/implemented/bug-fix/2026-11-27-style-self-heal-probe.md`：tree 门与 archived 门通过；format 门失败名单恰为既有 4 篇（crossfade-merge、multi-instance-snap-review-fixes、leader-driven-group、wm-moving-snap），本笔记不在名单、未代改。
- 实机清单「窗口投影」一节已追加 3 条（焦点切换、外观切换、日志回传），既有条目未动。
- 2026-11-28 探针定案（用户回传 pid=23908 运行）：`[window-probe] event=focus-loss style=0x160B0000 exstyle=0x00040010 win_rect=(2591,134,3629,926) draw_rect=(2591,134,3629,926)`。样式在探针时刻干净（CAPTION/SIZEBOX 位不在、剥离存活；WINDOWEDGE 不在），全程无 restrip 行——嫌疑（甲）样式写回排除；win_rect 与 draw_rect 逐像素相等（1038×792，与同轮截图量得的暗带矩形严丝合缝）——嫌疑（乙）几何错位排除。结论：暗带不是样式问题、不是几何错位，而是窗口表面自身在透明区呈现的像素，归因转工单 04（eframe 默认清屏色）。本票 Status 保持 needs-info：清单中与本轮探针无关的确认项不在工单 04 范围。
