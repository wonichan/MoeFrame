# 01: 实例身份与配置隔离（编号认领 / 实例段迁移 / 写入纪律 / 级联初始位置）

**What to build:** 给每个 MoeFrame 进程一个实例编号作为身份，并把配置从「全局一份、整文件覆写」改成「一份文件、实例段隔离」：`config.toml` 新增 `[[instances]]` 段存各实例的图库与几何，全局段继续存共享设置；写入改为写前重读、只改自己那段，修掉双开互相回滚。无落盘几何的新实例按级联偏移开窗。本票是专题地基，不含任何吸附行为。

**Blocked by:** 无（地基票）

**Status:** resolved

## Acceptance criteria

代码项：

- [x] 编号认领：实例启动时认领最小空缺编号（app 目录下编号锁文件占位），进程退出释放；两个实例同时启动不认领同一编号，第二个实例退出后第三个实例能复用其编号。有对应自动化测试或可复现的并发验证记录。
- [x] 实例段读写：`[[instances]]` 每段含编号、`image_folder`、`window` 几何；实例只读写自己那段的图库与几何，A 实例切图库/移位落盘不改 B 实例段的任何字节级内容（以解析后字段比对为准）。
- [x] 迁移：老配置（无实例段）首次启动时，1 号实例继承现有 `image_folder` 与 `window`，位置与图库与升级前一致；迁移只发生一次，重复启动不重复生成实例段、不丢数据。
- [x] 写入纪律：所有写回路径（几何落盘、设置落盘、退出冲刷）统一走「重读磁盘当前文档 → 只改自己实例段与全局段中自己改的键 → 写回」；构造「A 改图库、B 改几何、交错写回」的测试场景，最终文件两处改动都在，无一被回滚。
- [x] 级联初始位置：认领编号无落盘几何时，新窗相对上一扇已开相框偏移 +24px 打开（横纵各 +24，仍受既有工作区钳制约束）；有落盘几何时严格按落盘位置开，不偏移。
- [x] 单实例行为不回归：只开一扇时，配置读写、几何落盘、退出冲刷与现行行为一致；既有 config/config_sync/window_geom 测试全绿（因结构变更必须改的断言逐条在 Comments 注明理由）。
- [x] 检查：`scripts/check.sh` 本体未运行（Windows 专用，本 VM 跑不了，不冒充）；替代口径已跑并全绿——Linux 测试副本 `cargo test --all-targets` 350 passed / 0 failed，Windows 目标 `cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all -- --check` 三项通过（详见 Comments）。

实机项（汇总进工单 04 的实机清单，本票置 needs-info 时回填勾选）：

- [ ] 实机：双开 exe，两扇位置/图库各自独立调整后分别重启，各自恢复自己的位置与图库，无串扰。
- [ ] 实机：老配置升级首启，第一扇位置与图库与升级前一致。

## Context

专题 spec：[多实例吸附](../spec.md) 第 1、6 节。现状事实（2026-11-23 探查）：配置为单一全局结构 `Config{image_folder, window, playback, display, general}`，`ConfigStore` 启动整表读入内存、setter 后整文件 `fs::write` 覆写、写前不重读；几何落盘经 `ConfigSync::save_window_geometry`，退出再冲刷一次；全仓无单实例限制、无编号概念。写入纪律是本票的核心，不是顺手重构——所有现有写回路径都要收拢到同一条「重读—局部改—写回」通道。

## Comments

- 2026-11-23 grill 定案后建票，待实现。

- 2026-11-23 实现完成（分支 impl/multi-instance-snap-01，代码提交 6fb1cce），Status 置 needs-info：代码项全勾；两条实机项不勾、保持待工单 04 汇总验收，故不置 resolved。
- 实现要点：新增库 module `instance` 管编号认领与级联裁决；`config` 新增 `[[instances]]` 实例段（编号、image_folder、window 子表），`Config` 视图的图库/几何取本实例段、`load` 保持 1 号口径并新增 `load_for_instance`；写入纪律收拢到 `ConfigStore::update` 唯一通道（重读磁盘 → 重解析 → 只改本实例段与本次改的全局键 → 写回），几何/设置/退出冲刷/迁移写回全经此路；`config_sync` 新增 `startup_for_instance` 贯通编号，宿主 run() 先认领编号再装配、守卫活到退出释放。续播状态仍在全局 [general]（全局同步归工单 04，本票不预支）。实现笔记：.agents/notes/implemented/feature/2026-11-23-multi-instance-identity-config.md。
- 关键语义决定（spec/工单未定处，按 spec 精神取最小方案）：(1) 锁文件名为 app 目录下 `instance-<编号>.lock`，内容为持有者 pid；create_new 原子占位。(2) 陈旧锁判定：持有者 pid 经存活判断（`process_alive` 接缝：Unix 查 /proc、Windows OpenProcess+GetExitCodeProcess、其他平台保守存活）已死，或内容经约 200ms 短暂重读仍非合法 pid，才算陈旧——重读是防 create_new 与写 pid 之间的空窗误抢（并发测试实测复现过一次重号并已修复）；残锁删后重试同一编号，不永久占用。与工单 04 的衔接：自启恢复的存活口径复用同一 `process_alive` 接缝。(3) 级联的「上一扇」取落盘段口径：编号小于本实例且有落盘位置的最大编号段（实时窗口枚举归工单 02）；级联位置只作初始放置不直接落盘，本实例段几何仍由其自身防抖/退出冲刷写回，钳制走几何机既有工作区口径。(4) 迁移后顶层 image_folder 与 [window] 移除（数据完整搬进 1 号段，不双写）。
- 测试数字：新增 16 项（instance 9：最小空缺/释放复用/Drop 释放/残留锁回收/损坏锁回收/活锁不抢/8 线程并发不重号/process_alive/级联裁决；config 4：迁移一次/二号不继承/交错写两处都在/A 写回不改 B 段字段；config_sync 3：双实例交错经 ConfigSync 不回滚/级联 +24/有落盘不偏移）。替代验证口径（scripts/check.sh 本体未运行）：Linux 测试副本（~/workspace/tmp/refresh-moeframe-test-snap01.sh 刷新）cargo test --all-targets = 350 passed / 0 failed；Windows 目标 cargo check --all-targets、cargo clippy --all-targets -- -D warnings、cargo fmt --all -- --check 三项通过。TDD 记录：先写测试时新 API 不存在、13 处编译错误（红），实现后转绿。
- 既有测试断言变更：无。既有 config/config_sync/window_geom 等测试未改断言即全绿（load 的 1 号口径与旧行为等价，迁移对旧用例透明）。
- 边界检查（safe_code_edits_and_commits）：最终 diff 专项复读无劈裂/孤儿文档注释、无重复属性；GLOSSARY Avoid 词（窗口号/进程号/磁吸/窗口组/编组/贴边停靠）对 diff 与新文件 grep 零命中。代码与文档分两笔提交，暂存清单逐笔核对一致。
- 遗留风险：load 修复/迁移写回仍是整文档写回，两实例同瞬间 load 有极窄互覆窗口（setter 路径已收拢，发生条件苛刻，工单 04 可再收）；pid 复用理论上可让残锁误判存活（只跳号、无数据风险）；级联基点为上一扇的落盘位置而非其实时位置（工单 02 实例发现落地后复核）。
- 双轴审查修复（2026-11-24，分支 impl/multi-instance-snap-review-fixes）：S2——load 的修复/迁移写回已收拢到与 setter 相同的 update 唯一通道（空改动调用，借其重读-重解析把修复落到磁盘当前文档上；修复与迁移幂等），不再凭首次读入文档整表覆写；新增测试 load_repair_write_back_preserves_other_instances_current_data 锁住「修复写回不回滚其他实例当前数据」。残余窗口与 setter 路径同级（重读→写回之间无文件锁，是本专题既定设计口径，不再单列风险）。S4——级联基点改实时枚举：宿主启动瞬间枚举活伙伴、按锁文件 pid→编号定身份，纯规则 cascade_base_position（编号最大者）定基点，落盘段口径降为无活伙伴/枚举失败时的兜底（startup_for_instance_with_cascade）；本票原「级联基点为落盘位置」的遗留风险据此关闭。
- 2026-11-24 发布收官：用户已用新路线验收包（集成分支 c95b17d）在 Windows 真机整体验收通过（原话「可以了」），Status 置 resolved；随 v0.2.0 发布。
