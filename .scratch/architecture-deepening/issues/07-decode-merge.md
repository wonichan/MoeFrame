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

## Comments

- 2026-11-22 前提变化（提交 `7861e5f`，决定见 `.agents/notes/implemented/feature/2026-11-22-image-driven-frame.md`）：现行决定明示内部 `DisplayMode`/填充路径暂时保留、其删除不属本票原范围；候选 5 原定的「fit/fill 计算合一」在本票开工前须先与用户重新定界（合一，还是随填充路径删除而简化），再按定界后的范围执行。其余合流决定（产品侧承接、验证 adapter、等价测试转常驻）不变。
- 2026-11-22 票面输入（深挖工单 06 差异清单全文，与工单 06 Comments 同文）：
  - **差异清单全文**（本票实测结论；工单 07 的票面输入、工单 22/23 发布门的复测说明）：
  - **等价面（实测全等）**：五种格式 PNG/JPEG/BMP/WebP/GIF × 横/竖/方 × Fit/Fill × 多目标 × 两档 Limits——接受/拒绝、source_pixels、scaled_pixels、display_pixels、RGBA 字节全等；Fill 裁剪边界（源比例在目标两侧、恰等、刚过等号）一致；超限三类（源宽/高超 max、解码预留超 max_alloc、填充覆盖超 max_alloc）与损坏/截断/扩展名伪装，两侧同为拒绝且方向已钉死；零目标/零 Limits 守卫同为拒绝。
  - **已知差异 D1（行为）**：encoded 字节预检——验证接缝在解码前以 encoded_bytes > max_alloc 拒绝（原因原文 `encoded file exceeds experimental max_alloc`），产品无预检、按解码输出判定。当 max_alloc 介于显示覆盖缓冲与编码长度之间时两侧结果不同（验证拒绝、产品接受），形态已由测试 `encoded_precheck_is_the_known_difference` 钉住。合流处置（候选 5 Q8 已定）：工单 07 合流后验证 adapter 不保留预检，Limits 只按解码输出判定。
  - **已知差异 D2（文本）**：拒绝原因措辞——覆盖超限：验证 `coverage buffer exceeds experimental max_alloc` vs 产品 `coverage buffer exceeds max_alloc`（等价测试首轮失败输出实测）；正数守卫：验证 `physical target and experimental Limits must be positive` vs 产品 `physical target and Limits must be positive`。结果（同为拒绝）一致，等价断言不断言原因文本；合流后随实现统一自然消失。
  - **尺寸取整归类（三份实现）**：解码侧两份整数版逐项全等；绘制侧浮点版 `display::display_size` 与整数版对照（9 种源尺寸 × 4 种目标 × 双模式）——驱动边只差 f32 表示误差（<0.01px）；派生边 Fit 向下取整（浮点−整数 ∈ [0,1)；浮点 <1 时整数钳到 1），Fill 向上取整（整数−浮点 ∈ [0,1)）。无归类外差异。工单 07 合一时以此归类为取整政策的定义依据。
  - **未知差异：无。** 测试期间唯一失败是本票测试自身的 max_alloc 取值未避开覆盖缓冲门，属测试设计修正，非管线差异。
