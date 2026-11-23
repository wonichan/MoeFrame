# 01: 显示数据管线收成 display_data module

**What to build:** 「当前纹理与所显示图片的显示口径（适应/填充、物理目标）一致」这一不变量只由一个库 module 维护：它管理显示数据的记账、转正、过期判定与重载决策，宿主只上传纹理与绘制，全仓不再有第二处转正记账。

**Blocked by:** None (can start immediately).

**Status:** resolved

## Acceptance criteria

- [x] 库层新增 egui-free module `display_data`（领域名「显示数据」，见 GLOSSARY），持有显示数据（解码图 + 模式 + 物理目标 + 路径）与全部记账；纹理句柄仍由宿主持有一份小表，按 module 的抽象槽位对应。（DisplayPipeline 持 current/incoming 两槽 DisplayKey 与全部记账；仅依赖 decode_validation 的 DisplayMode 与 std）
- [x] 槽位只有两个：当前、传入；刷新重载的决策形态是「替换当前槽内容」，不设第三槽。（刷新在 refresh_reply 内直接写 current，无第三槽）
- [x] `RELOAD_SEQ = u64::MAX` 哨兵及其借道彻底删除；worker 通道给「刷新当前图显示数据」独立的命令与回送类别；序号空间只留核心导航加载，序号的分配与回送校验归 display_data。（全仓 grep 零命中；worker 新增 Command::Refresh/Reply::Refreshed 独立类别；begin_load 分配、nav_reply 校验）
- [x] 核心发出的全部加载流量（导航加载、预载、刷新）经 display_data 中转：在飞记录（路径、模式、目标）、回送匹配与过期判定都在其中；FrameCore 不改。（导航经 begin_load/nav_reply、预载由宿主以 DisplayKey 组键发送、刷新经 tick/refresh_reply；app_core.rs 不在本票改动内）
- [x] worker 预载缓存的键口径（路径 + 物理目标 + 模式，即「同一份显示数据」的定义）由 display_data 给出，缓存存储仍在 worker。（DisplayKey 定义与文档在 display_data；Cached 改持整键比较，Load 命中与 Refresh 的 obtain 共用）
- [x] 物理目标变化的 250 ms 防抖归 display_data 计时：宿主每帧喂原始观察值与当前时刻，module 决定何时下重载决策。（TARGET_SETTLE_SECS 与候选计时在 tick 内；宿主 refresh_display_data 每帧组装 Observation 喂入，调用点与旧实现同为 apply_fit_mode 与 drain_replies）
- [x] 刷新失败策略按定案明文实现：保持当前纹理不动、不重试、不提示，直到下一次口径变更或导航自然换图；该策略在 module 文档中成文。（模块头成文；refresh_failed_for 实现同键压制，转正/Swap 清除；与旧实现的帧驱动重试差异如实记于实施笔记）
- [x] 滑动绘制数学（行程、ease-out、纹理绘制）留在宿主不动；module 只下决策（哪个槽出/入、何时转正）。（绘制代码未动；转场结算仅换记账调用）
- [x] tests/ 直接驱动 display_data：喂加载回送、模式变更、目标变更，断言转正 / 丢弃 / 重载 / 无动作决策，覆盖过期回送与刷新失败两条路径。（12 项：转正、过期丢弃、失败清在飞、模式刷新 + Swap、目标防抖停稳/抖动、失败压制与再触发、画面切走丢弃、转场中丢弃、invalidate 丢待决、键身份、在飞豁免）
- [x] 全量测试绿、Windows 目标 check/clippy 通过；宿主文件行数较实施前下降；双轴 code-review 通过并写实施笔记。（全量 188/188；Windows check 通过，clippy 产品侧零警告；宿主 1656 → 1623 行；双轴审查见 Comments；实施笔记见下）

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 1（grilling Q1–Q11 定案）；父规格：[宿主层深挖规格](../spec.md)。

本票不改变任何用户可见行为；转场状态机仍在 FrameCore，纹理的实际绘制仍在宿主。

## Comments

- 2026-11-21 实现完成，状态置 needs-info：十条验收标准在本侧全满足（全量 188 项测试、Windows check/clippy、双轴审查），但宿主纹理搬运（上传/转正/Swap）无自动化覆盖且从未在真实 Windows 上运行——显示刷新路径的实机确认并入仓库根目录 `real-machine-check-deepening.md` 的工单 01 一节，按专题顺序在工单 01–05 后、发布门前与 10–21 清单同轮执行，确认后转 resolved。实施笔记：`.agents/notes/implemented/feature/2026-11-21-display-data-pipeline.md`。
- 结构决定：管线只记账与裁决，纹理句柄按当前/传入槽位归宿主；刷新失败同键压制是与旧实现唯一的有意行为变化（旧为帧驱动重试，可见结果相同），已在模块头与笔记中明文。
- 双轴审查（基线 8a2ead2）：Standards 无硬违规，判断项三条全采纳（重载/刷新用词统一、删 DisplayKey::new、字段注释对齐术语界线，另修正 Debounce 过期注释）；Spec 十条全满足，行为等价 (a)–(e) 成立，(f) invalidate 连带清理的边角时序差异已记入笔记 Consequences。
- 2026-11-22 对齐（提交 `7861e5f`，决定见 `.agents/notes/implemented/feature/2026-11-22-image-driven-frame.md`）：显示口径已固定为完整显示，适应/填充切换不再存在；管线与内部 `DisplayMode` 暂时保留，填充路径处于闲置，删除与否另行决定（不属深挖 07 原范围）。深挖实机清单工单 01 节涉及模式切换的两项已作废。

- Windows 补测：`v0.1.3` 放大后松手能自行变清晰；混合 DPI（125% → 100%）停稳清晰，坏图刷新旧图保留、无弹窗且仍可切下一张（[W1b、W1d、W2a](../../../real-machine-check-closeout.md)）。
- [W1c](../../../real-machine-check-closeout.md) 未通过：同一 exe 在原生缩放中连续两次复现 2:1 宽图被完整画进约 200×550 的窗口，松手才恢复正确比例。无动画路径在宿主 `drain_replies` 转正新纹理；`WindowGeom::observe` 在原生手势中暂不改尺寸，`paint_stage` 无动画时满幅绘制。窗口几何与画面切换不同步，不能用刷新重试修正。

- 用户选定松手再切；修复等待新尺寸后再显示自动加载的新图，不改播放间隔。`scripts/check.sh` 全过（342 项）；Windows 上拖角切图两次通过，坏图刷新和不拖动切图对照通过。见[补测 W1c/W1d](../../../real-machine-check-closeout.md)和[实施笔记](../../../.agents/notes/implemented/bug-fix/2026-11-23-autoplay-during-resize.md)。关闭本票；修复未发新包。
