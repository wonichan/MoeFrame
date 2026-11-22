# 06: 双解码交叉等价测试

**What to build:** 在合流之前，先用一组交叉等价测试把两条解码管线（产品 loading 与工单 09 验证 decode_validation）的行为差异钉死：哪些是等价面、哪些是已知差异，白纸黑字由测试守住，合流（工单 07）与发布门（v1-exe 工单 22/23）都站在这组测试上。

**Blocked by:** None (can start immediately)，但须在 v1-exe 工单 22/23 发布门之前完成。

**Status:** resolved

## Acceptance criteria

- [x] 新增交叉等价测试：同一组样本集（覆盖各支持格式、超限尺寸、填充裁剪边界、损坏文件）在多种 Limits 下，分别经产品管线与验证管线加载，断言输出尺寸与拒绝/接受结果一致。
- [x] fit/fill 尺寸计算的三份实现（两处解码侧整数版 + display 绘制侧浮点版）在测试中互相校验，取整差异如有，逐项列明归类。
- [x] 已知差异单独成列、不算等价失败：首项为 encoded 字节预检（验证接缝按编码长度预检拒绝、产品按解码输出判定），测试以「差异存在且形态如此」的方式记录它，而不是断言一致。
- [x] 本票只加测试与差异清单，不改任何一侧实现；若测试暴露清单之外的未知差异，停下记录并上报，不在本票顺手修。
- [x] 差异清单落到工单 07 的票面输入与发布门的复测说明里（在本票 Comments 中给出清单全文）。
- [x] 全量测试绿（含新增交叉测试）；双轴 code-review 通过；本票不写实施笔记（纯测试增补，如实测出新差异则按笔记规则另记）。

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 5（grilling Q4/Q8 定案）；父规格：[宿主层深挖规格](../spec.md)。

工单 09 的冻结处置在本票期间继续有效：验证接缝的实现一字不动，本票只从外部观察它的行为。
## Comments

- 2026-11-22 实现完成，状态置 resolved：新增 `tests/decode_equivalence.rs`（6 项），全量 227 项全绿，Windows 目标 check 通过，clippy 仅既有 3 条警告。本票无实机项、无实施笔记（标准 6）；验证接缝一字未动。
- 双轴审查（基线 `f48ec36`）：Standards 无硬违规；判断项采纳 4 条（文件头政策表述收口到等价断言、DecodeError 改走导入、驱动边判定简化并注释同比例情形、截断样本加注），1 条记为有意选择（`save_sample` 与既有样本助手并存——渐变内容是 RGBA 字节全等断言的前提）。Spec 标准 1–4 满足；其指出的部分项已修复：拒绝类用例原先只断言两侧一致、未钉住拒绝方向，现改用 `assert_both_reject` 钉死两侧同为拒绝（原因文本仍不断言相等）；填充覆盖用例按模式分流（Fill 拒绝、Fit 接受，两侧一致）。
- **差异清单全文**（本票实测结论；工单 07 的票面输入、工单 22/23 发布门的复测说明）：
  - **等价面（实测全等）**：五种格式 PNG/JPEG/BMP/WebP/GIF × 横/竖/方 × Fit/Fill × 多目标 × 两档 Limits——接受/拒绝、source_pixels、scaled_pixels、display_pixels、RGBA 字节全等；Fill 裁剪边界（源比例在目标两侧、恰等、刚过等号）一致；超限三类（源宽/高超 max、解码预留超 max_alloc、填充覆盖超 max_alloc）与损坏/截断/扩展名伪装，两侧同为拒绝且方向已钉死；零目标/零 Limits 守卫同为拒绝。
  - **已知差异 D1（行为）**：encoded 字节预检——验证接缝在解码前以 encoded_bytes > max_alloc 拒绝（原因原文 `encoded file exceeds experimental max_alloc`），产品无预检、按解码输出判定。当 max_alloc 介于显示覆盖缓冲与编码长度之间时两侧结果不同（验证拒绝、产品接受），形态已由测试 `encoded_precheck_is_the_known_difference` 钉住。合流处置（候选 5 Q8 已定）：工单 07 合流后验证 adapter 不保留预检，Limits 只按解码输出判定。
  - **已知差异 D2（文本）**：拒绝原因措辞——覆盖超限：验证 `coverage buffer exceeds experimental max_alloc` vs 产品 `coverage buffer exceeds max_alloc`（等价测试首轮失败输出实测）；正数守卫：验证 `physical target and experimental Limits must be positive` vs 产品 `physical target and Limits must be positive`。结果（同为拒绝）一致，等价断言不断言原因文本；合流后随实现统一自然消失。
  - **尺寸取整归类（三份实现）**：解码侧两份整数版逐项全等；绘制侧浮点版 `display::display_size` 与整数版对照（9 种源尺寸 × 4 种目标 × 双模式）——驱动边只差 f32 表示误差（<0.01px）；派生边 Fit 向下取整（浮点−整数 ∈ [0,1)；浮点 <1 时整数钳到 1），Fill 向上取整（整数−浮点 ∈ [0,1)）。无归类外差异。工单 07 合一时以此归类为取整政策的定义依据。
  - **未知差异：无。** 测试期间唯一失败是本票测试自身的 max_alloc 取值未避开覆盖缓冲门，属测试设计修正，非管线差异。
