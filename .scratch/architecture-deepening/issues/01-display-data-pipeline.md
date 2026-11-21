# 01: 显示数据管线收成 display_data module

**What to build:** 「当前纹理与所显示图片的显示口径（适应/填充、物理目标）一致」这一不变量只由一个库 module 维护：它管理显示数据的记账、转正、过期判定与重载决策，宿主只上传纹理与绘制，全仓不再有第二处转正记账。

**Blocked by:** None (can start immediately).

**Status:** ready-for-agent

## Acceptance criteria

- [ ] 库层新增 egui-free module `display_data`（领域名「显示数据」，见 GLOSSARY），持有显示数据（解码图 + 模式 + 物理目标 + 路径）与全部记账；纹理句柄仍由宿主持有一份小表，按 module 的抽象槽位对应。
- [ ] 槽位只有两个：当前、传入；刷新重载的决策形态是「替换当前槽内容」，不设第三槽。
- [ ] `RELOAD_SEQ = u64::MAX` 哨兵及其借道彻底删除；worker 通道给「刷新当前图显示数据」独立的命令与回送类别；序号空间只留核心导航加载，序号的分配与回送校验归 display_data。
- [ ] 核心发出的全部加载流量（导航加载、预载、刷新）经 display_data 中转：在飞记录（路径、模式、目标）、回送匹配与过期判定都在其中；FrameCore 不改。
- [ ] worker 预载缓存的键口径（路径 + 物理目标 + 模式，即「同一份显示数据」的定义）由 display_data 给出，缓存存储仍在 worker。
- [ ] 物理目标变化的 250 ms 防抖归 display_data 计时：宿主每帧喂原始观察值与当前时刻，module 决定何时下重载决策。
- [ ] 刷新失败策略按定案明文实现：保持当前纹理不动、不重试、不提示，直到下一次口径变更或导航自然换图；该策略在 module 文档中成文。
- [ ] 滑动绘制数学（行程、ease-out、纹理绘制）留在宿主不动；module 只下决策（哪个槽出/入、何时转正）。
- [ ] tests/ 直接驱动 display_data：喂加载回送、模式变更、目标变更，断言转正 / 丢弃 / 重载 / 无动作决策，覆盖过期回送与刷新失败两条路径。
- [ ] 全量测试绿、Windows 目标 check/clippy 通过；宿主文件行数较实施前下降；双轴 code-review 通过并写实施笔记。

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 1（grilling Q1–Q11 定案）；父规格：[宿主层深挖规格](../spec.md)。

本票不改变任何用户可见行为；转场状态机仍在 FrameCore，纹理的实际绘制仍在宿主。
