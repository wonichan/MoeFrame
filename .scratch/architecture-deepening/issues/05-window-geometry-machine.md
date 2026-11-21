# 05: 窗口几何生命周期状态机

**What to build:** 窗口几何的整台生命周期（启动钳制、宽高比锁定修正、落盘防抖、恢复默认、首次放置）收进一个库层状态机，状态迁移第一次能脱离真实窗口被测试驱动；宿主每帧只喂观察值、执行它返回的抽象命令。

**Blocked by:** [02: 设置窗会话机 settings_session](02-settings-session.md)（锁定开关经会话发动作）、[04: 持久化镜像与启动装配 config_sync](04-config-sync.md)（落盘与启动初值归它）。

**Status:** ready-for-agent

## Acceptance criteria

- [ ] window_geom 就地演进为 egui-free 状态机（文件名不变）：宿主每帧喂观察值（窗口矩形、显示器尺寸、指针状态、锁定开关事件、恢复默认请求），它返回抽象命令（设位置、设尺寸、待保存几何值），宿主翻译成 ViewportCommand 执行。
- [ ] window_geom 的纯算术（bottom_right_position、aspect_locked_size、offscreen_position）被状态机吸收为内部实现，不再是公开面；tests/window_geom.rs 的用例改打状态机 interface，不保留双测试面。
- [ ] 状态机只产出「要保存的几何值」，落盘与退出收尾归 config_sync（工单 04）；启动几何初值从工单 04 的启动包来；状态机不直接碰 config。
- [ ] 宽高比锁定的比例捕获归状态机：锁定开启时用最近观察值捕获比例，不再由设置窗代码捕获；开关动作由 settings_session 发出、宿主转发。
- [ ] 与 display_data 不横向通信：尺寸/DPI 观察由宿主从几何状态机转喂 display_data。
- [ ] 多显示器已知限制只搬家不修复：副屏误钳的口径与警告注释迁入状态机文档，行为一字不变；实机清单中工单 20 的已知限制条目照旧。
- [ ] 工单 20 的全部口径保持：停稳后修正（非拖动时约束）、0.6 秒防抖落盘、退出冲刷（经 config_sync 收尾）、恢复默认的隐藏时显示语义。
- [ ] 行为测试覆盖生命周期迁移：首次放置、离屏钳制、锁定修正、防抖落盘产出、恢复默认；全量测试绿、Windows 目标 check/clippy 通过；双轴 code-review 通过并写实施笔记。

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 6（grilling Q1–Q7 定案）；父规格：[宿主层深挖规格](../spec.md)。

本票是六个候选里唯一可无损推迟到 V1 之后的一项；排期上在工单 04 之后、实机清单之前。
