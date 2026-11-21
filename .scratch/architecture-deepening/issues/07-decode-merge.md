# 07: 双解码管线合流

**What to build:** 全仓只剩一份解码实现：产品类型与实现回到产品侧，工单 09 的验证接缝变成骑在同一实现上的薄 adapter；fit/fill 计算只剩一份口径；encoded 预检的历史分歧在合流时裁决关闭，不再留给下一个人。

**Blocked by:** [06: 双解码交叉等价测试](06-decode-equivalence-tests.md)、v1-exe 工单 22/23 发布门完成（发布门须用工单 09 原接缝完成最后一次复测、闭合其实测证据链）。

**Status:** ready-for-agent

## Acceptance criteria

- [ ] 产品解码实现与产品类型（DisplayMode、DisplayImage、DecodeError、DecodeLimits）迁到产品侧；loading.rs 就地演进为产品解码 module 承接（不新起 module、不改名），模块头文档写清职责。
- [ ] decode_validation 只剩验证装配：命令行、样本生成、内存观测，加一个薄 adapter 把「可独立传 Limits 的实验入口」接到产品实现上；decode-validation 二进制保留且可运行。
- [ ] fit/fill 尺寸计算三份合一：解码与绘制共用一份，取整政策只定义一次；与 display_data 的分工不重叠（它管记账与决策，尺寸是纯算术）。
- [ ] encoded-size 悬案裁决落地：验证 adapter 不再自带编码字节预检，Limits 只按解码输出判定（与产品、与父规格统一）；工单 06 记录的已知差异在本票消除，交叉等价测试相应改为断言一致。
- [ ] 工单 06 的交叉等价测试改打统一实现并长期保留、保持全绿；display_data 等消费方对迁动类型的导入随迁移动（纯机械，不重开工单 01 的任何定案）。
- [ ] 文档同步：新写一份 implemented 笔记取代工单 09 冻结处置（说明接缝已合流、原实测对应合流前实现、发布门已用原接缝闭合）并与旧笔记交叉链接，旧笔记原文不改写；docs/decode-validation.md 同步为 adapter 形态。
- [ ] 全量测试绿、Windows 目标 check/clippy 通过；产品代码中解码实现只剩一份（可由仓库搜索核验）；双轴 code-review 通过。

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 5（grilling Q1–Q8 定案）；父规格：[宿主层深挖规格](../spec.md)。

本票是本专题最后一票，排在 V1 发布门之后，不挡 V1 交付。
