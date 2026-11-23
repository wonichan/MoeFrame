# Agent Note: 实例编号身份与配置实例段隔离（多实例吸附专题工单 01）

Status: implemented

## Problem

多开现状（spec 第 1 节）：无单实例限制、也无多实例支持——配置全局一份，每个实例启动时整表读入内存、任一 setter 凭内存副本整文件覆写，双开时几何、图库与全局设置互相回滚（整文件级 last-writer-wins）；进程没有身份，两扇窗初始精确重叠。本票是专题地基：给实例编号身份、把图库与几何按实例段隔离、立写入纪律、给无落盘几何的新实例级联初始位置。不含任何吸附行为。

## Decision

- 新增库 module `instance`（纯逻辑可测）：编号认领与级联裁决都在这里，宿主只装配。
  - 锁文件口径：app 目录下 `instance-<编号>.lock`，内容为持有者 pid。认领从 1 起用 `create_new` 原子占位，撞号时读持有者 pid 做存活判断：存活跳下一号；已死或内容非法视为陈旧锁，删后重试同一编号。`InstanceClaim` 的 `release`/Drop 删锁释放编号，守卫在宿主 `run()` 里活到进程退出。
  - 存活判断经 `process_alive` 接缝：Unix 查 `/proc/<pid>`，Windows 用 OpenProcess + GetExitCodeProcess 看 STILL_ACTIVE，其余平台保守按存活。工单 04 的自启「在运行」标记恢复若需更强口径，复用此接缝，不改认领流程。
  - 陈旧判定带短暂重读（自旋约 200ms）：create_new 与写 pid 之间有内容为空的窗口，立即判陈旧会抢活人的锁——并发测试实测复现过一次重号，靠重读修复；超时仍读不出合法 pid 才判陈旧（写一半崩溃的残锁）。
- `config` 新增 `[[instances]]` 实例段：每段含 `number`、`image_folder` 与 `[instances.window]` 子表（x/y/width/height 沿现字段语义）；全局段（recursive/playback/display/general）继续存共享设置，语义不变。`Config` 视图的 `image_folder`/`window` 永远取本实例段，另带 `instance_number` 与全部段快照 `instances`（只读用途，如找上一扇位置）。`ConfigStore::load` 保持 1 号口径，新增 `load_for_instance`。
  - 迁移只发生一次：文档无 `instances` 数组时，把顶层 `image_folder` 与 `[window]` 按既有字段修复口径读出、写成 1 号段并移除顶层原字段；文档自此有实例段，永不再走迁移分支。段级修复：编号缺失/非整数/<1/重复的段整段丢弃，图库非字符串删键，window 走与顶层同一套修复（含删 `lock_aspect`）；本实例段缺失时补默认段。
  - 续播状态（current_image/round_seen/history）仍在全局 `[general]`：本票只隔离图库与几何，全局设置的跨实例实时同步归工单 04，本票不预支。
- 写入纪律收拢到唯一通道 `ConfigStore::update`：重读磁盘当前文档 → 在新文档上 `parse_document` 重解析（含修复与迁移） → 只施加本次改动（实例段经 `instance_table_mut` 按编号定位，全局 setter 只碰自己改的键） → 刷新实例视图 → 写回。几何落盘、设置落盘、退出冲刷（经 ConfigSync::shutdown → save_window_geometry）与 load 的修复写回全部走这条或 load 路径，无第二条整文件覆写。磁盘读不出/不可解析时退回内存快照再写，避免可读性问题变数据丢失；写失败口径不变（ConfigSync 记一行日志）。
- `config_sync` 新增 `startup_for_instance(.., instance_number)`，`startup` 委托为 1 号。启动包 `HostInit` 新增 `instance_number`；核心入参的图库与宿主几何都按本实例段取数。级联：本实例段无落盘位置时，取「编号小于本实例且有落盘位置的最大编号段」的位置 +24/+24 作为初始位置（`instance::initial_position` 裁决，saved 优先）；级联结果只进 `saved_position`/`saved_geom` 做初始放置，不直接落盘——本实例段的几何仍等其自身几何变化经既有防抖/退出冲刷写回；钳制走几何机既有工作区口径（启动离屏钳制 + 落位钳制），本票不新增定位规则。
- 宿主 `run()`：先 `claim_instance(app 目录)` 再按编号 `startup_for_instance`，并打一行 `instance-claimed instance=N` 启动标记；认领失败按启动失败处理（写 startup-error.txt 的既有路径）。

## Alternatives considered

- 实例段用 `[instance1]` 式命名表而非 `[[instances]]` 数组：spec 明文 `[[instances]]`，且数组便于按编号遍历与迁移判定——从 spec。
- 锁用目录占位（create_dir 原子且可把 pid 编进名内、无空窗）：能免掉重读自旋，但 spec 口径是「编号锁文件」，且文件内容 pid 对工单 04 的存活判断更直接——保留文件 + 重读防空窗。
- 级联位置在启动时直接写进本实例段落盘：会把「从未被用户确认的位置」变成落盘几何，之后每次启动都按它开而非重新级联；且多写一次盘——不采纳，只作初始放置。
- 把续播状态也拆进实例段：spec 第 6 节只点名图库与几何，全局同步归工单 04——不预支。

## Testing

- TDD：先写测试观察失败（新 API 不存在，13 处编译错误），再实现转绿。
- 新增 16 项：tests/instance.rs 9 项（最小空缺顺序认领、释放后复用最小空缺、Drop 释放、死持有者残留锁回收、损坏锁回收、活持有者锁不被抢、8 线程并发认领不重号、process_alive 真值、级联裁决三态）；tests/config.rs 4 项（迁移一次且不丢数据、二号不继承一号、A/B 交错写最终两处都在、A 写回不改 B 段字段级比对）；tests/config_sync.rs 3 项（双实例经 ConfigSync 交错写不回滚且核心入参按实例取数、级联 +24/+24、无落盘不偏移对照的有落盘严格按落盘）。
- 全量（Linux 测试副本，沿用既有替代口径）：350 passed / 0 failed（本票新增 16 项，其余为集成分支基线）。Windows 目标：`cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all -- --check` 三项均通过。`scripts/check.sh` 本体未运行（Windows 专用，本 VM 跑不了），未冒充。
- 边界检查（safe_code_edits_and_commits）：最终 diff 专项复读——无劈裂/孤儿文档注释、无重复属性；GLOSSARY Avoid 词（窗口号、进程号、磁吸、窗口组、编组、贴边停靠）对 diff 与新文件 grep 零命中。

## Consequences

- 双开时图库与几何各自落盘互不回滚，全局键交错写也不再互滚；单实例行为与旧口径一致（load = 1 号）。
- 遗留风险：(1) 两个实例在同一瞬间各自 load 触发修复/迁移写回时，load 路径仍是整文档写回，理论上有极窄的互覆窗口——setter 路径已收拢，load 写回只在文档需要修复/迁移时发生，接受为已知口径，工单 04 做全局同步轮询时可再收。(2) 陈旧锁重读自旋最长约 200ms/个残锁，只在启动认领路径发生。(3) 级联的「上一扇」以落盘段为准而非实时窗口位置（窗口枚举归工单 02），上一扇开着但从未落盘新位置时，级联基点是它的落盘位置——与 spec「相对上一扇已开相框」的偏差仅限于此，工单 02 落地实例发现后可复核。(4) pid 复用理论上可让残锁误判存活（跳号，无数据风险）。
- 实机两项（双开重启各自恢复、老配置升级首启一致）未验，汇总进工单 04 实机清单。
