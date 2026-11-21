# Agent Note: 持久化镜像与启动装配收拢到 config_sync（深挖工单 04）

Status: implemented

## Problem

「哪个值以谁为准、何时写回」散在六处：run() 逐字段翻译配置（文件夹、间隔、模式、续播三字段装配、窗口派生、自启对账回写）；核心 Save 系列效果在 dispatch 里逐个直呼 ConfigStore setter；宿主直写又散在显示方式、几何防抖、恢复默认、自启镜像、设置窗宽高比锁定五处；退出冲刷在 Exit 臂里自拼保存调用。写回失败口径还不一致：多数 `let _ =` 静默吞掉，自启镜像的保存失败靠注释解释「下次启动自愈」。新增一个持久化设置要改六处（父决策见 proposed 笔记 2026-11-21-host-deepening-plan 候选 4）。

## Decision

- 库层新增 module `config_sync`，两头收：(a) `ConfigSync::startup(config_path, log_path, &mut dyn AutostartStore) -> (ConfigSync, StartupPackage)` 做全部启动翻译——加载/修复配置、自启注册表↔镜像对账装配、产出启动包（`CoreArgs` + `HostInit` + `AutostartSync`），run() 只分发；(b) 全部镜像写回经它的 save_* 方法落盘，宿主不再直呼 ConfigStore 任何 setter。ConfigStore 不合并、不改字段级修复语义，降为其背后的文件 adapter。
- FrameCore 入参打包在此兑现（工单 03 只减不打包）：`CoreArgs` 定义在 app_core（消费方持有包类型，生产方 config_sync 负责装配），`FrameCore::new(args, thresholds)`；全仓 11 处构造点统一为命名字段字面量写法。
- 三种镜像模式在 module 头显式成文：核心状态镜像（核心为准，Save 系列写穿）、宿主状态镜像（宿主为准，记录当前值）、注册表反向对齐（注册表为准，配置 autostart 只是镜像）。开机自启只收两环：镜像写回改经 save_autostart_mirror、启动对账装配归 startup；AutostartSync 决策与 RegAutostartStore 不搬家。
- 写回失败统一口径（三种镜像一致，module 头成文）：不阻塞行为、不弹窗、不重试，失败经唯一的 note_write_result 记一行 `config write failed (<name>): <error>` 到现有日志文件；偏差靠下一次启动的对账/字段修复收敛。此前自启镜像「保存失败静默自愈」的个别口径就此并入统一口径（多了一行日志，其余不变）。
- 退出冲刷归收尾职责：`shutdown(observed, saved) -> GeometryRecord`——观察值与落盘记录不同才写、返回结清后的记录，宿主 Exit 臂只发收尾请求并回填基准（工单 20 语义不变）。几何记录类型立名 `GeometryRecord` 归本 module，宿主原 SavedGeom 别名删除改导入。
- 工单 02 的改道在此落地：settings_session 的落盘类动作在宿主执行分发处改走 config_sync（文件夹/间隔/模式经核心 Save 效果，其余经同步的宿主镜像方法），settings_session 本身未改。两套秒数规则（parse_interval_draft 与配置修复）未动、未合并。

## Alternatives considered

- CoreArgs 定义在 config_sync 里由 app_core 反向引用：同 crate 虽可编译，但包类型归消费方更顺，生产方只装配——未采纳反向引用。
- 把 ConfigStore 合并进 config_sync：字段级修复与手改保留是已验证的文件层行为，合并徒增风险——父评审已否决，降为 adapter。
- 写回失败保留个别口径（自启静默、其余吞掉）：统一口径正是本票的目的——未采纳。

## Testing

- 新增 tests/config_sync.rs 8 项，全在真实临时配置/文件上：启动翻译逐字段（含自启对账装配）、对账回写落盘、注册表读失败退镜像、空配置无 resume/无位置、全部镜像写回可重读 + 续播清除、写回失败口径（写目标被目录占住时两次失败恰记两行、调用照常返回、目标恢复后下一次写回落盘收敛）、shutdown 三态、非 UTF-8 当前图整份跳过且不记失败日志。
- 全量测试 210 项全绿；Windows 目标 check 通过，clippy 产品侧零警告；cargo fmt 已应用。
- 双轴 code-review（基线 2ed5882）：Standards 1 处硬违规已修（删除宿主 paths_to_strings 时其文档注释残留到 app_directory 头上并留下重复 cfg——本仓库复发模式的删除变体；注释已随函数迁入 config_sync 一侧），判断项三条全采纳（GeometryRecord 单一名、HostInit 字段文档补齐、write 改名 note_write_result）；Spec 八条全满足，行为等价 (a)–(g) 逐项成立（启动翻译逐字段、自启对账、setter 改道全清、UTF-8 口径、退出冲刷、失败口径无遗漏、秒数规则未合并、自启决策未搬家），无范围蔓延。

## Consequences

- 新增一个持久化设置的改动面降到「config_sync + UI」两处；宿主 FrameApp 的 store 字段整体消失，换成 sync。
- 写回失败从此在日志里可见（此前静默）：日志可能多出 `config write failed` 行，这是口径统一的预期结果，不是新故障。
- 本票动了全部持久化路径与启动装配，宿主接线无自动化覆盖：实机清单（real-machine-check-deepening.md）新增本票小节，重启恢复/自启对账/几何恢复/显示方式持久化须在真窗口确认后转 resolved。
- 工单 05 的入口已就位：geometry 状态机只发保存意图、经 config_sync 落盘；启动几何值从 HostInit 领取，本票未预留其他接口。
