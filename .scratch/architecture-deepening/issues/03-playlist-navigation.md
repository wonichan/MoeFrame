# 03: 导航决策收拢到 Playlist

**What to build:** 「下一张从哪来」只有一个地方能回答：候选计算、落位方式与落位记账整段归 Playlist，FrameCore 只保留计时、在飞/排队、转场与视图状态，理解导航不再需要在两个文件间跳。

**Blocked by:** None (can start immediately).

**Status:** needs-info

## Acceptance criteria

- [x] Playlist 拿走整段导航决策：core 交「方向 + 触发源」、拿回「要加载的候选集 + 落位标记」；加载成功后 core 把成功路径交回 Playlist 完成落位记账。（plan_navigation 产出 NavPlan；core 的 navigate 只登记在飞并发 Load，候选 match 与落位分派已整段删除；触发源 auto/手动仍由 core 的在飞记录区分）
- [x] RandomSource 的所有权移入 Playlist；FrameCore 不再持有或借出随机源，FrameCore::new 入参由 6 个降为 5 个（只减不打包，打包归工单 04）。（new/restored 收 Box<dyn RandomSource>；生产 SystemRandom 移入 playlist.rs，宿主同名实现删除；FrameCore::new 6 → 5 参）
- [x] 备袋内化为 Playlist 落位记账时的自检规则；preload_target 只读地产出预载目标，不再背「备袋」副作用。（ensure_bag/bag_candidates 已私有；preload_target 签名无 rng、对外不记账——其内部仍可备袋，与旧实现一致，备袋不等于记账）
- [x] 落位标记的类型定义归 Playlist；core 的在飞记录只原样携带、不解释，core 不再有自己的 Landing 枚举。（Landing 不透明结构体，kind 私有；core 的 Pending/Transition 只存取不解释，core 自有 Landing 枚举已删）
- [x] 恢复（restored）、重扫（rescan）、当前图缺失时的替代候选一并下沉为 Playlist 的方法，产出与导航同种结果（候选集 + 落位标记）；core 的 ScanCompleted 臂只做分流与时序；自由函数 candidates_when_current_missing 删除。（plan_resume_load、rescan -> RescanPlan 三态、私有 missing_candidates 共用；「随机+缺失→清进度」搬进 restored；initial_landing 按胜出者判定 Restored 且只消费一次；自由函数已删）
- [x] 续播数据改由 Playlist 产出一份只读快照（当前图、本轮进度、历史）供 core 拼 SaveResumeState；core 不再直接读 Playlist 的 seen/history/游标字段，Playlist 不为此开只读字段口子。（resume_snapshot 为唯一出口：current + seen 自然序 + history 显示序；seen_paths/history/history_cursor/items 等公开访问器全删）
- [x] Intent/Effect 不新增变体；宿主无感知（仍只与 core 打交道）。（枚举未动；宿主 diff 仅删 SystemRandom 与构造实参）
- [x] 测试随决策搬家：app_core 测试中断言候选/落位结果的用例搬到 Playlist 测试面，core 测试只留时序断言；导航决策只有一个测试面。（app_core 55 → 53，两个候选用例迁至 tests/playlist.rs 改打 plan interface；playlist 25 → 31；core 其余随机用例改顺序无关断言，只留时序与状态）
- [x] 全量测试绿、Windows 目标 check/clippy 通过；双轴 code-review 通过并写实施笔记。（全量 202/202；Windows check 通过、clippy 产品侧零警告；核心 717 → 623 行；双轴审查见 Comments；实施笔记见下）

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 3（grilling Q1–Q8 定案）；父规格：[宿主层深挖规格](../spec.md)。

ADR-0002 的两份状态（本轮进度与历史）规则不变，本票是向它靠拢：规则只在一处被解释。

## Comments

- 2026-11-21 实现完成，状态置 needs-info：九条验收标准在本侧全满足。本票与工单 01/02 不同，没有宿主接线变化（宿主只删了随机源与一个构造实参）、没有新增实机检查项——导航行为在真实窗口的表现与既有清单（工单 10–21 与深挖 01/02 两份）同源，随深挖实机清单同轮确认后转 resolved。实施笔记：`.agents/notes/implemented/feature/2026-11-21-playlist-navigation.md`。
- 标准 3 的口径在此写明（Spec 审查附注）：备袋内化指 ensure_bag/bag_candidates 成为播放序列私有规则；preload_target 对外只读（不记账、不动进度与历史），其内部备袋行为与旧实现一致保留。
- 双轴审查（基线 8cc54e6）：Standards 无硬违规，判断项五条全采纳（模块头「触发源」措辞、悬空交叉引用、陈旧括注、两个测试名、孤儿 mod common）；Spec 九条全满足、行为等价 (a)–(g) 全成立、无范围蔓延。Spec 另记一处不可达差异：恢复标记旧代码在首图加载失败时也消费，新代码只在成功提交时消费——失败后该播放序列再无 Initial 提交路径（重扫先清标记、改文件夹整表替换），无可观察后果。
