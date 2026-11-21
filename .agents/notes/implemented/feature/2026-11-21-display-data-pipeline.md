# Agent Note: 显示数据管线收成 display_data module（深挖工单 01）

Status: implemented

## Problem

「当前纹理与所显示图片的显示口径（适应/填充、物理目标）一致」这一不变量此前靠宿主三处手抄维系：核心加载转正、转场完成转正、刷新替换各抄一份 texture_mode/texture_target 记账；刷新借道核心加载命令 + RELOAD_SEQ 哨兵序号，与导航回送挤在同一个 Loaded 回送里靠序号区分；目标防抖、过期判定、在飞豁免散在宿主字段间，零自动化覆盖（父决策见 proposed 笔记 2026-11-21-host-deepening-plan 候选 1）。

## Decision

- 新增库层 egui-free module `display_data`，类型 `DisplayPipeline`：持有当前/传入两槽的显示数据身份（`DisplayKey` = 路径 + 物理目标 + 显示方式）、在飞导航记录（序号 + 发出口径）、待决刷新、防抖候选与失败记忆。接缝为事件进/裁决出：`begin_load`（分配序号）、`nav_reply`（Accept/Failed/Stale）、`promote_incoming`（直接提交与转场完成共用一条转正记账）、`drop_incoming`/`clear_current`、`invalidate`（扫描作废）、`tick`（每帧喂 Observation，返回要刷新的期望键）、`refresh_reply`（Swap/Discard）、`load_in_flight`（宿主重绘轮询节奏用）。
- 纹理句柄留宿主，按当前/传入槽位对应持有；刷新在 module 里的决策形态是「替换当前槽」，没有第三槽。滑动绘制数学不动。
- RELOAD_SEQ 哨兵删除：worker 新增独立的 `Command::Refresh { key }` / `Reply::Refreshed` 类别；导航序号空间只服务核心加载，分配与校验归管线。worker 预载缓存改持 DisplayKey（键口径归 display_data，存储归 worker）；Refresh 与 Load 共用「缓存命中直交、否则现解码、失败记一条跳过日志」的 obtain 路径，与旧借道 Load 时的证据行为一致。
- 刷新失败策略按定案明文实现于模块头：保旧纹理、不重试、不提示；失败记下期望键，同键压制至口径变化或导航转正。**这是与旧实现唯一的有意行为变化**：旧实现刷新失败后由帧驱动持续重试（结果上旧图也一直保留），新实现同键只试一次；用户可见结果相同，浪费的重复解码消失。
- `invalidate` 连带清待决刷新与防抖候选（旧实现不清）：扫描后到达的旧刷新回送由「按守卫评估」变为「直接丢弃、需要时由 tick 重新触发」，可能多一次解码；扫描期间 50ms 重绘轮询回落到核心截止期分支。结果收敛一致，无可见差异（双轴审查 Spec 轴 (f) 项核对）。

## Alternatives considered

- 宿主内持有者（pipeline 放宿主、直接拿纹理句柄）：测不到，等于把热点换个房间——父评审已否决。
- 哨兵只换枚举标签、保留单一 Load 命令：借道核心序号空间的根不除——父评审已否决。
- DisplayKey 保留 `new` 构造器：三字段全公开的纯数据键，构造器无附加值——双轴审查 Standards 轴提出后删除，统一用结构体字面量。

## Testing

- tests/display_data.rs 12 项行为测试直接驱动管线 interface：转正记账、过期序号丢弃、失败回送清在飞保当前、模式刷新立即触发 + Swap、目标防抖停稳/抖动重启计时、失败保旧图 + 同键压制 + 新键再触发、画面切走丢弃、转场中回送丢弃且不记失败、invalidate 丢待决刷新、键身份四维、在飞同口径豁免。
- 全量测试 188 项全绿（176 + 12）；Windows 目标 check 通过，clippy 产品侧零警告（仅余工单 09 验证程序的 3 个既有警告）；cargo fmt 已应用。
- 双轴 code-review（基线 8a2ead2）：Standards 无硬违规，判断项三条全采纳（「重载/刷新」用词统一到刷新、删 DisplayKey::new、宿主字段注释与术语界线对齐，另顺手修正 Debounce 注释的过期描述）；Spec 十条标准全满足（标准 10 的笔记项即本笔记），行为等价点 (a)–(e) 成立、(f) 边角时序差异如上记录。

## Consequences

- 宿主文件 1656 → 1623 行，删去 8 个记账字段；宿主的 Debounce 只剩窗口几何落盘一处使用（目标防抖已归管线）。
- 宿主接线（上传、转正搬运、Swap 搬运）仍无自动化覆盖，靠结构与实机清单：本票置 needs-info，显示刷新路径的实机确认并入深挖专题清单（仓库根目录 real-machine-check-deepening.md），按专题顺序在工单 01–05 后、发布门前执行。
- 后续工单与本 module 的关系：工单 05 的几何状态机不与它横向通信，尺寸/DPI 观察由宿主转喂；工单 07 合流时 DisplayImage 等类型的导入随类型迁动机械调整，本 module 的决策不重开。
