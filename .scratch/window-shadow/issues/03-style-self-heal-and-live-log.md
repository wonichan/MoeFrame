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

**Status:** ready-for-agent

## Acceptance criteria

- [ ] WM_STYLECHANGED 自愈：目标位重现时再剥 + SetWindowPos(FRAMECHANGED)，一行 restrip 日志；位已清时零动作零日志（收敛性有单测或等价纯判定覆盖）
- [ ] [window-probe] 快照在首次焦点丢失与外观切换两点位各记一行（同点位不重复刷屏）：STYLE/EXSTYLE、窗口矩形、绘制矩形四项齐全
- [ ] 既有行为不回归：工单 02 的模态环/吸附/样式剥离、工单 01 的 DWM 日志、置顶守擂全绿；设置窗不被监听不被剥
- [ ] `scripts/check.sh` 口径全绿（fmt / clippy -D warnings / test --all-targets；VM 代偿口径如实记入 Comments）
- [ ] 实机清单「窗口投影」追加：启动后切走焦点再切回、切换一次外观模式，各截宽图看暗层；并回传 [window-style] 与 [window-probe] 全部相关行
- [ ] 实现笔记落 `.agents/notes/implemented/` 并通过三道校验门

## Context

- 量测原始数据：2026-11-27 晚截图 image-e5b73877.png（1202×869）：左 transect y=430 步进点 x=66；上 x=600 步进点 y=15；下 x=600 步进点 y≈810；暗层矩形约 x∈[66,1103]、y∈[15,810]
- winit 写回路径：vendored winit-0.30.13 window.rs 十余处 set_window_flags 调用（set_always_on_top、set_visible、set_resizable 等皆经此路整词重算样式）
- 守擂自检已有运行期 HWND 取得与样式读取先例（level_guard 的 GetWindowLongPtrW 自检），probe 点位与之同处最省接线
- 若本票实测排除（甲）（乙），下一嫌疑是 eframe 合成路径的表面 alpha，另票再议；本票不预先动合成层

## Comments

（暂无）
