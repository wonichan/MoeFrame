# 03: 导航决策收拢到 Playlist

**What to build:** 「下一张从哪来」只有一个地方能回答：候选计算、落位方式与落位记账整段归 Playlist，FrameCore 只保留计时、在飞/排队、转场与视图状态，理解导航不再需要在两个文件间跳。

**Blocked by:** None (can start immediately).

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Playlist 拿走整段导航决策：core 交「方向 + 触发源」、拿回「要加载的候选集 + 落位标记」；加载成功后 core 把成功路径交回 Playlist 完成落位记账。
- [ ] RandomSource 的所有权移入 Playlist；FrameCore 不再持有或借出随机源，FrameCore::new 入参由 6 个降为 5 个（只减不打包，打包归工单 04）。
- [ ] 备袋内化为 Playlist 落位记账时的自检规则；preload_target 只读地产出预载目标，不再背「备袋」副作用。
- [ ] 落位标记的类型定义归 Playlist；core 的在飞记录只原样携带、不解释，core 不再有自己的 Landing 枚举。
- [ ] 恢复（restored）、重扫（rescan）、当前图缺失时的替代候选一并下沉为 Playlist 的方法，产出与导航同种结果（候选集 + 落位标记）；core 的 ScanCompleted 臂只做分流与时序；自由函数 candidates_when_current_missing 删除。
- [ ] 续播数据改由 Playlist 产出一份只读快照（当前图、本轮进度、历史）供 core 拼 SaveResumeState；core 不再直接读 Playlist 的 seen/history/游标字段，Playlist 不为此开只读字段口子。
- [ ] Intent/Effect 不新增变体；宿主无感知（仍只与 core 打交道）。
- [ ] 测试随决策搬家：app_core 测试中断言候选/落位结果的用例搬到 Playlist 测试面，core 测试只留时序断言；导航决策只有一个测试面。
- [ ] 全量测试绿、Windows 目标 check/clippy 通过；双轴 code-review 通过并写实施笔记。

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 3（grilling Q1–Q8 定案）；父规格：[宿主层深挖规格](../spec.md)。

ADR-0002 的两份状态（本轮进度与历史）规则不变，本票是向它靠拢：规则只在一处被解释。
