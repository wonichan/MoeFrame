# Agent Note: 切换动画宿主转场骨架与有方向组（卷页 / 抽卡）

Status: implemented

## Problem

[切换动画专题工单 02](../../../../.scratch/transition-animation/issues/02-host-directional-curl-card.md) 要让宿主真正播放核心定死的转场：工单 01 落地后宿主仍在 `dispatch` 里秒回 `TransitionCompleted`，且新图纹理在 `Reply::Loaded` 分支尾同帧转正——只延迟回送而不搬转正，旧图当场就丢，动画无从画起。本票须同时解决：转正时机后移、双纹理跨帧共存、墙钟推进与帧尾唤醒、两处绘制点（内置画框路径与极简/自定义路径）统一接入，以及卷页/抽卡两种有方向动画的绘制。

## Decision

- 新增宿主模块 `src/bin/moe-frame/transition.rs` 集中骨架与分发：`HostTransition { kind, direction, started_ms, duration_ms }` 只存核心事件给的值；`FrameApp` 新增同名 `transition` 字段。`Effect::StartTransition` 分发臂改调 `begin_transition` 置位状态（纹理缺失时退回秒回完成，不卡死）。
- 转正搬家：`Reply::Loaded` 分支尾的转正加守卫 `transition.is_none()`；新增 `advance_transition`（每帧 Tick 后、绘制前调用）——到期才清状态、回送 `TransitionCompleted`，随后才把 `incoming` 转正并 `promote_incoming`，旧纹理随赋值 drop 释放。提交点后移由此在宿主侧闭合（核心记账本来就等回送）。
- 取消路径：`Effect::Scan` 与图库变空的收尾同步清宿主转场状态（与核心清转场同口径）；`slide_active` 两处喂真值（`refresh_display_data` 的 Observation 与 `Reply::Refreshed` 的裁决参数都改 `transition.is_some()`），转场期间管线不发刷新、回送丢弃，旧纹理不被中途替换。
- 帧尾唤醒：`next_frame_delay` 新增 `transition_deadline_ms` 参数，优先级在加载轮询之下、治愈保活之上：距截止取 min(剩余， 16ms)，已过取 0——动画期间连续出帧、到期帧不迟到；转场结束后参数为 None，自动回到静止零重绘。既有 6 个用例补参数，新增 1 个用例钉住三档返回值与优先级。
- 舞台共用绘制：`paint_stage(painter, image_rect) -> bool` 供两处绘制点共用（frame_paint 内置路径替换原 image 落笔、frame_ui 极简/自定义路径替换原 image 落笔，空状态提示仍由调用方按返回 false 绘制）。无转场时行为与旧代码逐项一致（当前纹理满幅）。
- 纯函数接缝（先写测试后实现）：`progress`（墙钟、钳制、零时长即完成）、`fit_rect`（旧图按自身比例居中最大内接、绝不拉伸）、`curl_frame`（ease-out 揭示区/前沿/旧图亮度 1.0→0.75）、`card_frame`（两半程的在场面、cos/sin 横向收拢、倾斜随方向反号）。测试照 `next_frame_delay` 先例放宿主内联 mod。
- 卷页绘制：旧图以 fit 矩形 + 灰 tint 变暗垫底；新图以 `with_clip_rect(揭示区)` 在 fit 矩形上满幅；卷边 = 前沿 2px 高光线 + 揭示侧 4 条渐淡影带（借 frame_paint 分段渐变手法），与原型/CSS 参考同为 clip+影带近似，不做真卷曲几何（探路结论）。
- 抽卡绘制：在场面以单 Mesh 梯形贴图（横向收拢 + 顶/底边反向倾斜模拟绕竖轴透视，亮度随 |cos| 压暗）；中点换面只是绘制层行为，纹理槽与核心提交仍等完成时刻。
- 工单 03 的三类（Blinds/Ink/Pixel）在分发 match 里先占位：新图直落绘制，但时长、回执与转正仍走骨架；03 只需替换该 match 臂，不动骨架。

## Alternatives considered

- **转正留在 Loaded 分支、仅延迟回送** — 探路已指出旧图同帧即丢，动画第一帧就没有旧图可画，拒绝。
- **在 Loaded 分支里按「是否刚起转场」决定转正、完成回送时不再转正** — 转正与回送分散两处，重扫/失败等收尾路径容易漏；集中到 `advance_transition` 一处更稳，取后者。
- **卷页做真卷曲 Mesh（弯曲面+背面镜像）** — egui Mesh 是仿射插值、无透视校正，弯曲面贴图会滑动；spec 只要求卷入+亮影，取 clip 近似（与原型一致）。

## Testing

- TDD：先写 transition.rs 内联测试（progress/fit_rect/curl_frame/card_frame 共 6 个用例）与 next_frame_delay 转场用例，红相位为 Windows 目标 check 报未解析导入（函数不存在）；实现后转绿（编译层面）。
- Windows 目标（x86_64-pc-windows-gnu）：`cargo check --all-targets` 通过、`cargo clippy --all-targets -D warnings` 干净、`cargo fmt --check` 通过（覆盖宿主 bin 与全部内联测试的编译）。
- 全量：Linux 测试副本（winit 补 x11 feature、删 bin 目标，与工单 01 副本同口径）lib + 18 个集成测试目标 **296 passed / 0 failed**；本票未触库与集成测试文件。
- 环境注记：本 VM 的 Linux 上 `scripts/check.sh` 原样仍跑不了（产品 bin 为 Windows-only、winit 无 Linux 平台），与基线一致；宿主内联测试（transition 纯函数 + next_frame_delay）只能在 Windows 目标下编译、无法在本 VM 执行，运行验证归工单 04 实机清单与 Windows CI。
- 未验证：真实窗口里的动画观感、转正时序的实机表现（本票因此置 needs-info，目检项并入工单 04 清单）。

## Consequences

- 03 接百叶帘/墨晕/像素时：只替换 `paint_transition` 的占位 match 臂，骨架（begin/advance/paint_stage、唤醒、slide_active）不要重写；三种动画需要的额外转场期状态（如像素块序）加在 `HostTransition` 或其旁的新字段里，并在 `begin_transition` 初始化。
- 转场期间核心若再发 `StartTransition`（排队导航在完成回送后才起新一轮 Load，正常时序不会同帧叠加）；`begin_transition` 直接覆盖置位，最坏情况是旧转场被新转场取代，incoming 仍是最新目标，不丢图。
- `advance_transition` 在 Tick 之后调用：转场到期帧上，核心先处理 Tick（转场中 Tick 不触发轮播）再收完成回执，从完成时刻重计间隔的口径不变。
