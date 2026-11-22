# 01: 配置转正与核心转场状态机

**What to build:** 把只保存不驱动的 `animation` 配置键转正为 7 档切换动画（无/卷页/百叶帘/墨晕/抽卡/像素故障/随机，默认卷页）；核心层恢复并泛化转场状态机——动画种类与方向在导航意图产生时定死、随机抽法（等概率、禁连续重复、只存内存）、提交点在转场完成、动画期间只留最后一个意图、隐藏时直落。宿主渲染不在本票（02/03），本票的转场以「宿主按墙钟时长回送完成」为接缝。

**Blocked by:** None (can start immediately)

**Status:** needs-info

## Acceptance criteria

- [x] `Animation` 枚举改为 7 档，存盘键名 `none/curl/blinds/ink/card/pixel/random`；旧值迁移：`none` 保留，`slide/fade`/未知值落默认卷页；配置往返与迁移有测试。
- [x] 每档动画的时长在核心层定死：curl 550ms / blinds 650ms / ink 750ms / card 600ms / pixel 550ms；`none` 无转场、即时提交（现行行为不变）。
- [x] 转场开始事件携带动画种类、方向与时长：方向由导航意图定死，不因加载完成顺序反转；随机档在转场开始时抽定具体动画（等概率、与上一次实际播放不同，只存内存不落盘），随机结果对宿主是具体动画而非「随机」。
- [x] 提交点在转场完成：记账、历史、落盘、重计轮播间隔只在完成回执后发生；转场期间 Tick 挂起；转场期间暂停仍完成并提交但不计时（继承工单 14 行为测试口径）。
- [x] 动画/加载期间的新意图只保留最后一个，不并行、不累积；失败/失效目标不启动转场、不提交。
- [x] 首图与重扫首图不启动转场、直接提交；相框隐藏时切图不启动转场、直接提交。
- [x] 目标未就绪时不启动转场、保留当前图（不闪白的接缝在核心层成立：转场只在加载成功后产生）。
- [x] 通过应用行为入口控制时间与加载结果做测试：五种动画的种类/方向/时长、随机不连续重复、连点只留最后意图、慢图、失败、暂停、隐藏直落、首图直落；不绑定 GUI 帧数或线程调度。
- [x] 全量测试绿；fmt/clippy/test 以 Windows 目标与 Linux 测试副本代偿验证（check.sh 本体因 VM 无 Linux GUI 条件未直跑，详见实施笔记）；实施笔记落盘。

## Context

专题 spec：[切换动画](../spec.md)（用户 2026-11-22 原型评审 + 三轮 grill 共识）。历史基线：工单 14 的转场骨架（提交点后移、单意图队列）与 2026-11-22 即时提交对齐（`7861e5f`）——本票是在即时提交之上把转场接缝泛化到 5 种动画，不是恢复旧滑动。

## Comments

- 2026-11-22 实现完成，状态置 needs-info：配置转正（7 档、默认 curl、none 保留 / slide·fade·未知落 curl 写回、set_animation + save_animation + 启动翻译进 CoreArgs）、核心转场泛化（StartTransition 扩 animation/duration_ms、五档冻结时长、AnimationSelected/SaveAnimation 同值守卫、隐藏与无档汇合 commit_display 直落）与随机接缝（复用 playlist 的 RandomSource trait，`FrameCore::new` 委托 `with_animation_random`，生产 SystemRandom、测试脚本源；池排除 last_animation，等概率，last 在转场完成时才记、只存内存）全部落地。宿主仅加 SaveAnimation 落盘臂，秒回完成保持（02/03 再延迟）。验证：全量 297/297 绿（新增 8 项：config +3、app_core +6、既有隐藏轮播测试按新直落口径改断言 1 项），fmt 通过，clippy（lib+集成测试）干净，Windows 目标 check/clippy 全过；check.sh 在本 VM 的 Linux 副本止步于工单 09 验证程序的既有警告（未触其文件），与基线一致。实施笔记：`.agents/notes/implemented/feature/2026-11-22-transition-animation-core.md`。needs-info 原因：本票无独立实机项，宿主真实渲染与观感验收在 02/03/04，待专题收口后随 04 回执转 resolved。
- 2026-11-22 工单 04 收口回填：实机清单已就绪——仓库根目录 `real-machine-check-transition-animation.md`「工单 01 · 核心口径」节覆盖本票行为项（随机连切 10 张、连点只留最后意图、慢图不闪白、隐藏直落、暂停播完不续计时、改档下一条生效、首图/重扫直落），资源复测在清单末节。本票继续 needs-info，待用户跑完清单回执后转 resolved。
- 2026-11-22 评审修复·随机口径明示（spec 第 3 节「与上一次实际播放的动画不同」的实现口径）：`last_animation` 只在转场完成时才记——被重扫中途取消的转场即使画面已部分播放也不记。理由：取消的转场没有完成提交，把它计入会让下一次随机平白排除一种动画；且与「提交点在转场完成时」的全专题口径一致。此口径实施时已记于本票实施笔记，双轴评审建议在工单里再明示，即本条。
