# 05: 窗口几何生命周期状态机

**What to build:** 窗口几何的整台生命周期（启动钳制、宽高比锁定修正、落盘防抖、恢复默认、首次放置）收进一个库层状态机，状态迁移第一次能脱离真实窗口被测试驱动；宿主每帧只喂观察值、执行它返回的抽象命令。

**Blocked by:** [02: 设置窗会话机 settings_session](02-settings-session.md)（锁定开关经会话发动作）、[04: 持久化镜像与启动装配 config_sync](04-config-sync.md)（落盘与启动初值归它）。

**Status:** needs-info

## Acceptance criteria

- [x] window_geom 就地演进为 egui-free 状态机（文件名不变）：宿主每帧喂观察值（窗口矩形、显示器尺寸、指针状态、锁定开关事件、恢复默认请求），它返回抽象命令（设位置、设尺寸、待保存几何值），宿主翻译成 ViewportCommand 执行。（GeometryMachine/GeomObservation/GeomCommand 落地；宿主 update_window_geometry 只采样+执行，execute_geometry_commands 统一翻译）
- [x] window_geom 的纯算术（bottom_right_position、aspect_locked_size、offscreen_position）被状态机吸收为内部实现，不再是公开面；tests/window_geom.rs 的用例改打状态机 interface，不保留双测试面。（三函数已私有、函数体逐行不变；tests/window_geom.rs 18 项全走状态机 interface，无双面）
- [x] 状态机只产出「要保存的几何值」，落盘与退出收尾归 config_sync（工单 04）；启动几何初值从工单 04 的启动包来；状态机不直接碰 config。（SaveGeometry/SaveLockAspect 只产出值，宿主转交 config_sync；exit_settle 交接 shutdown；初值取自 HostInit；window_geom 对 config 仅引用 GeometryRecord 类型）
- [x] 宽高比锁定的比例捕获归状态机：锁定开启时用最近观察值捕获比例，不再由设置窗代码捕获；开关动作由 settings_session 发出、宿主转发。（set_lock 按机器 last_inner 捕获、兜底默认比例逐字保留；宿主 ApplyAspectLock 臂只转发；勾选读数走 is_locked）
- [x] 与 display_data 不横向通信：尺寸/DPI 观察由宿主从几何状态机转喂 display_data。（口径标注：不横向通信已达成——两 module 无互引；「由宿主从几何状态机转喂」字面未实施——physical_target 仍由宿主独立采样，与几何观察同帧同视口同值、行为等价；父决策候选 6 未点名转喂细节，按父决策口径满足、按票面字面为部分，详见实施笔记 Alternatives）
- [x] 多显示器已知限制只搬家不修复：副屏误钳的口径与警告注释迁入状态机文档，行为一字不变；实机清单中工单 20 的已知限制条目照旧。（警告口径迁入 window_geom 模块头，offscreen_position 判定逐字不变；工单 20 清单条目照旧，深挖清单另加本票小节复核）
- [x] 工单 20 的全部口径保持：停稳后修正（非拖动时约束）、0.6 秒防抖落盘、退出冲刷（经 config_sync 收尾）、恢复默认的隐藏时显示语义。（帧序与门控逐支等价，见实施笔记与 Spec 审查 (a)(c)(e)；首次放置折进 observe 后尺寸来源由 viewport_rect 改为同帧 inner_rect、inner 缺失帧延后一帧，终态等价）
- [x] 行为测试覆盖生命周期迁移：首次放置、离屏钳制、锁定修正、防抖落盘产出、恢复默认；全量测试绿、Windows 目标 check/clippy 通过；双轴 code-review 通过并写实施笔记。（18 项覆盖五类迁移；全量 221/221；Windows check 通过、clippy 产品侧零警告；双轴审查见 Comments；实施笔记见下）

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 6（grilling Q1–Q7 定案）；父规格：[宿主层深挖规格](../spec.md)。

本票是六个候选里唯一可无损推迟到 V1 之后的一项；排期上在工单 04 之后、实机清单之前。

## Comments

- 2026-11-21 实现完成，状态置 needs-info：八条验收标准在本侧满足（标准 5 带口径标注，见该条勾选注与实施笔记）。实机项已追加到 `real-machine-check-deepening.md` 的工单 05 小节（锁定手感、几何记忆与退出冲刷、恢复默认、副屏已知限制），且工单 20 在 10–21 清单里的条目在新代码上照旧执行；确认后转 resolved。实施笔记：`.agents/notes/implemented/feature/2026-11-21-geometry-machine.md`。
- 双轴审查（基线 6ba9ec2）：Standards 1 处硬违规已修（新注释三处误用 GLOSSARY Avoid 词「锁定比例」，改「宽高比锁定的比例」），判断项两条全采纳（aspect_locked_size 陈旧调用方表述改述、GeomCommand 两变体补文档）；删除残留专项核查无问题（Debounce/常量/旧方法/六字段删除点干净）。Spec 行为等价 (a)(c)(d)(e)(f) 全成立、(b) 终态等价，无范围蔓延；另记基线边角一处被原样保留：恢复默认后 observed_geom 仍是旧观察值（仅恢复后同帧立刻退出可触及，两版相同）。
