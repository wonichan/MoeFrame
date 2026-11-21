# Agent Note: 窗口几何生命周期收拢为状态机（深挖工单 05）

Status: implemented

## Problem

窗口几何的生命周期散在宿主：update_window_geometry 里挤着启动钳制、宽高比锁定修正、防抖落盘三段时序，首次放置在另一个方法里，恢复默认又是一份独立拼装，锁定比例的捕获在设置窗动作臂里——状态迁移只能在真实窗口里观察，库层只有三个纯算术函数可测，调用点的编排（哪帧查钳制、防抖何时结算、捕获用哪一帧的尺寸）无自动化覆盖（父决策见 proposed 笔记 2026-11-21-host-deepening-plan 候选 6）。

## Decision

- `window_geom` 就地演进为 egui-free 状态机（文件名不变）：`GeometryMachine` 持有宽高比锁定的比例、上一帧内尺寸、落盘基准、最近观察几何、防抖候选与一次性的钳制/放置标记；宿主每帧喂 `GeomObservation`（外矩形位置/尺寸、内尺寸、显示器、指针、可见性、时钟），执行返回的 `GeomCommand`（SetOuterPosition/SetInnerSize 翻译成 ViewportCommand；SaveGeometry/SaveLockAspect 经 config_sync 落盘；ShowFrame 经行为核心）。迁移顺序与工单 20 的宿主帧序一致：钳制 → 修正 → 落盘观察 → 首次放置。
- 三个纯算术（bottom_right_position、aspect_locked_size、offscreen_position）收为内部实现，函数体逐行不变；防抖计时内化（SAVE_SETTLE_SECS = 0.6），宿主的 Debounce 结构随之删除；DEFAULT_FRAME_SIZE 迁入本 module 为公开常量，宿主同名常量删除。
- 比例捕获归状态机：`set_lock` 用机器最近观察的内尺寸捕获（无观察时初值即启动包的存档尺寸），兜底退回默认尺寸比例（工单 20 口径逐字保留）；设置窗开关经 settings_session 动作由宿主转发，勾选读数改走 is_locked。
- 恢复默认归状态机：`restore_default(monitor, frame_visible)` 产出命令组并同步推进落盘基准与候选；退出结算经 `exit_settle()` 把（观察值、落盘基准）交给 config_sync.shutdown（工单 04 交接形状不变，shutdown 返回值不再回填——退出后无读者）。
- 状态机只产出待保存值、不碰 config；启动初值从工单 04 的启动包（HostInit）领取。多显示器已知限制只搬家不修复：副屏误钳的口径与警告迁入模块头，判定逻辑逐字不变。
- 测试改打状态机 interface：tests/window_geom.rs 7 → 18 项（首次放置、钳制只查一次、部分可见不钳、锁定修正五支、防抖产出/变值重启/隐藏不观察、比例捕获、恢复默认四支、退出结算），不保留双测试面。

## Alternatives considered

- 把 display_data 的物理目标观察也改从状态机转喂：票面标准 5 字面如此要求，但父决策候选 6 只定了「module 不横向通信、一切经宿主转交」的共同形状，未点名转喂细节；宿主 physical_target 独立采样与几何观察同帧同视口同值，改道无行为收益且要给 GeomObservation 加 DPI 字段——未做，口径出入在工单票面如实标注。
- 首次放置继续独立于 observe：会留两个喂入口，生命周期又散回两处——未采纳；折进 observe 后放置的尺寸来源由 viewport_rect 改为同帧的 inner_rect（egui 中同值），inner 缺失的帧延后一帧放置，终态等价。

## Testing

- 全量测试 221 项全绿；Windows 目标 check 通过，clippy 产品侧零警告；cargo fmt 已应用。
- 双轴 code-review（基线 6ba9ec2）：Standards 1 处硬违规已修（新注释三处误用 GLOSSARY Avoid 词「锁定比例」，工单 20 已判过同词违规——改「宽高比锁定的比例」），判断项两条全采纳（aspect_locked_size 陈旧调用方表述、GeomCommand 两个变体补文档）；Spec 八条中六条全满足，行为等价 (a)(c)(d)(e)(f) 逐项成立，(b) 首次放置终态等价（尺寸来源变化见 Alternatives）；标准 5 为口径性部分满足（不横向通信达成、转喂字面未达、行为等价），已在票面标注。

## Consequences

- 几何状态迁移第一次能脱离真实窗口被测试驱动；宿主删去六个几何字段、Debounce 与一份恢复默认拼装，update_window_geometry 只剩采样与执行。
- 本票是六个候选里唯一可无损推迟的一项，按总顺序在实机清单之前落地；实机清单的工单 20 条目（锁定手感、恢复默认、副屏已知限制等）在新代码上照旧执行，深挖清单另加本票小节。
- 恢复默认后 observed_geom 仍是旧观察值的基线边角被原样保留（只有恢复默认后同一帧内立刻退出才可能触及，两版行为相同），未在本票处理。
