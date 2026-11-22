# 02: 宿主渲染·有方向组（卷页 + 抽卡）

**What to build:** 宿主按核心转场事件渲染两种有方向动画：卷页（curl，550ms，新图自导航方向一侧卷入、卷边带亮影、旧图轻微变暗）与抽卡（card，600ms，绕竖轴翻转、旧图翻走新图翻正）。方向来自核心事件，宿主不自判方向。

**Blocked by:** [01: 配置转正与核心转场状态机](01-config-and-core-transition.md)

**Status:** needs-info

## Acceptance criteria

- [x] 卷页：下一张自右侧卷入、上一张自左侧卷入；卷边有一道亮影；旧图在动画期间轻微变暗；结束时画面与无动画直落的最终状态一致。
- [x] 抽卡：绕竖轴翻转（非横轴）；下一张/上一张翻转方向相反；新图翻正落位后无残余变换。
- [x] 几何口径：动画开始帧窗口已变到新图比例（核心/几何既有路径负责），宿主在新舞台内绘制；旧图按自身比例居中、绝不拉伸；动画只动图片区域，框体/卡纸/相框图/异形轮廓不动。
- [x] 墙钟推进：按事件给定的时长以墙钟计算进度，掉帧跳进度不延长；结束帧回送转场完成回执（01 的接缝），不提前也不拖延一帧以上地漏发。
- [x] 双纹理共存仅限动画期间；完成回执后旧纹理随既有替换路径释放，不新增常驻纹理。
- [x] 动画期间宿主连续重绘，结束后立即回到静止零重绘（沿用宿主帧尾唤醒裁决，不另起计时器）。
- [x] Windows 目标 check/clippy 通过；实机目检项并入工单 04 的实机清单（本票状态置 needs-info 待回执）。

## Context

专题 spec：[切换动画](../spec.md) 第 2/4/7/8 节。视觉参考：同目录 `prototype.html` 的变体 A 与 D（原型数值已冻结进 spec，宿主实现以 spec 为准、不照搬 CSS 手法）。

## Comments

- 2026-11-22 实现完成，状态置 needs-info：宿主转场骨架落地（新模块 `src/bin/moe-frame/transition.rs`：`HostTransition` + `begin_transition`/`advance_transition`/`paint_stage`；`StartTransition` 不再秒回，转正从 `Reply::Loaded` 分支尾搬到完成时刻，旧纹理随转正释放；重扫/变空同步清宿主转场；`slide_active` 两处喂真值；`next_frame_delay` 新增转场唤醒臂 ≤16ms/到期 0ms）。卷页（clip 揭示 + 卷边高光线与渐淡影带 + 旧图变暗）与抽卡（单 Mesh 梯形、绕竖轴两半程换面、倾斜随方向反号）接入两处绘制点（frame_paint 内置、frame_ui 极简/自定义共用 `paint_stage`）；旧图按 `fit_rect` 居中不拉伸。工单 03 三类在分发 match 占位直落（时长/回执/转正走骨架）。验证：Windows 目标 check/clippy 全过、fmt 通过；Linux 副本 lib+集成 296/296 绿；宿主内联纯函数测试（progress/fit_rect/curl/card + next_frame_delay 转场用例）Windows 目标编译通过、本 VM 不能执行。实施笔记：`.agents/notes/implemented/feature/2026-11-22-transition-animation-host-directional.md`。needs-info 原因：实机目检（卷页/抽卡观感、宽高图互切、转正时序）并入工单 04 实机清单，待回执后转 resolved。
- 2026-11-22 工单 04 收口回填：实机清单已就绪——`real-machine-check-transition-animation.md`「工单 02 · 有方向组」节覆盖卷页/抽卡手动+自动、转正时序，并含**卷页近似目检**单独一条（clip 揭示 + 亮影近似）；宽↔高互切与三外观绘制在「几何与外观」节。本票继续 needs-info，待回执后转 resolved。
