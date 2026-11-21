# Agent Note: 导航决策收拢到播放序列（深挖工单 03）

Status: implemented

## Problem

导航决策散在两处：核心的 navigate 里按（模式 × 方向）算候选并定落位、commit_display 里按落位分派记账、ScanCompleted 三条分支各自拼候选，还有一个核心与播放序列共用的自由函数 candidates_when_current_missing；播放序列只提供积木（袋、历史候选、轮转、记账原语），每条新规则都要改两个 module。随机源住在核心、逐方法借给播放序列，宿主还要为此备一份 SystemRandom（父决策见 proposed 笔记 2026-11-21-host-deepening-plan 候选 3）。

## Decision

- 播放序列成为唯一导航决策面：`plan_navigation(direction) -> Option<NavPlan>`（候选集 + 落位标记）、`plan_first_load`、`plan_resume_load`、`initial_landing(shown)`、`rescan(new_items) -> RescanPlan{EmptyLibrary, Survived, Missing{candidates}}`、`land(landing, path)`、`resume_snapshot()`、`preload_target()`。落位标记 `Landing` 是本模块定义的不透明类型，核心在 Pending/Transition 里只原样携带、提交时交回 land，不读其内容；核心自有的 Landing 枚举删除。
- 随机源所有权移入播放序列：构造（new/restored）接收 `Box<dyn RandomSource>`；生产实现 SystemRandom 随之移入本模块，核心在构造播放序列时注入，宿主的 SystemRandom 删除。FrameCore::new 入参 6 → 5（只减 rng，不打包——打包归工单 04）。
- 备袋内化：ensure_bag/bag_candidates 成为私有规则。preload_target 的签名不再带 rng，但其内部仍可备袋（与旧实现一致：备袋不是记账，计划与预加载都不提前提交本轮进度）；「只读」的含义是对外不记账、不动进度与历史，不是内部零状态变化。
- 恢复语义下沉：restored 内部处理「随机 + 当前图缺失 → 清空旧本轮进度」（顺序模式保留过滤后的进度）；恢复当前图由 restored 记入 restore_current，initial_landing 按首图胜出者判定 Restored/Fresh 并只消费一次；缺失替代规则收为私有 missing_candidates，恢复与重扫共用；重扫另在入口清恢复标记。
- 记账原语全部私有化（record_fresh、land_history、land_restored、set_current、rotation_candidates、history_*_candidates、next_path），seen/history/游标不再开只读口子，落盘内容只经 resume_snapshot 导出（seen 自然序、history 显示序、上限仍在记账时执行）。
- 测试随决策搬家：tests/playlist.rs 全部改走新 interface（31 项，含自核心搬来的随机全程与首图袋序两条、新增恢复计划与 initial_landing 用例）；tests/app_core.rs 删去这两个断言候选用例，其余随机模式用例改顺序无关断言（集合比对，与既有重扫用例同例），核心只断言时序与状态。

## Alternatives considered

- 候选计算留在核心、只把落位记账下沉：决策仍是两处，父评审已否决。
- 落位标记做成裸枚举让核心 match：核心会重新解释落位语义，标记必须不透明——父评审已否决。
- 顺手把 FrameCore::new 的其余入参打包成构造包：归工单 04，本票只减不打包——父评审已定。

## Testing

- 全量测试 202 项全绿（app_core 53 + playlist 31 + 其余不变）；Windows 目标 check 通过，clippy 产品侧零警告；cargo fmt 已应用。
- 双轴 code-review（基线 8cc54e6）：Standards 无硬违规，判断项五条全采纳（模块头删去与实现不符的「触发源」措辞、declared_current 悬空交叉引用改直述、set_current 陈旧括注、两个测试名改指新 interface、app_core 孤儿 mod common 删除）；Spec 九条全满足，行为等价 (a)–(g) 逐项成立（候选与落位判定、恢复三分支组成与清空规则、重扫三态、快照内容、备袋与新一轮提交时机、随机源归属），无范围蔓延。

## Consequences

- 核心 717 → 623 行，只剩时序、在飞/排队、转场、手势与可见性；导航规则今后只改播放序列一个 module、只测一个测试面。
- 核心测试不再能注入确定性随机源：这是所有权搬家的既定代价，随机模式在核心层面的断言降为顺序无关（候选集合、失败重试同一份候选），顺序敏感的断言全在播放序列测试面。
- 本票状态置 needs-info 的理由与工单 01/02 不同：本票无宿主接线变化（宿主只删了随机源与一个构造实参），无新的实机检查项；它随工单 01–05 的深挖实机清单同轮被间接覆盖（导航行为在真实窗口里的表现与工单 10–21 清单同源），清单确认后一并转 resolved。
- 工单 04 的入口已就位：FrameCore::new 的 5 个平铺入参与三处 Playlist 构造（new/restored 的 SystemRandom 注入）就是 config_sync 启动装配要收走的点，本票未预留任何接口。
