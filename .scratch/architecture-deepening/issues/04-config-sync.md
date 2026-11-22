# 04: 持久化镜像与启动装配 config_sync

**What to build:** 「哪个值以谁为准、何时写回」只有一个出处：启动翻译产出一个启动包，全部镜像写回经一个 module 落盘；新增一个持久化设置的改动面从六处降到「config_sync + UI」两处。

**Blocked by:** [02: 设置窗会话机 settings_session](02-settings-session.md)（其落盘类动作在本票改道）。

**Status:** needs-info

## Acceptance criteria

- [ ] 库层新增 module `config_sync`，两头收：(a) 启动翻译：配置 → 一个启动包（FrameCore 构造入参打包 + 宿主初值打包），run() 只做分发，不再逐字段翻译；(b) 全部镜像写回：core 的 Save 系列 Effect 由它执行，宿主直写的显示方式、窗口几何、宽高比锁定、开机自启镜像也经它落盘。
- [x] 宿主不再直接调用 ConfigStore 的 setter；ConfigStore 不合并、不改其字段级修复语义，降为 config_sync 背后的文件 adapter；三种镜像模式（核心状态镜像、宿主状态镜像、注册表反向对齐）在 config_sync 内显式成文。（ConfigSync::startup 产出 StartupPackage{core: CoreArgs, host: HostInit, autostart}，run() 只分发；save_* 覆盖核心 Save 系列与宿主直写四项）
- [x] 开机自启只收两环：AutostartSync 的镜像写回改经 config_sync 落配置，启动时注册表↔镜像对账的装配归启动翻译；AutostartSync 的决策与 RegAutostartStore 不搬家。（全仓 grep 核过宿主 setter 直呼为零、FrameApp.store 字段整体移除；config.rs 未改；三种镜像模式在模块头成文并按三组分区组织）
- [x] 写回失败统一口径（三种镜像一致，明文成文）：不阻塞行为、不弹窗、不重试，记一行到现有日志文件；镜像偏差靠下一次启动的对账/字段修复收敛。（save_autostart_mirror 改道 + startup 内对账装配；src/autostart.rs 与宿主 RegAutostartStore 均未动）
- [x] 退出冲刷归 config_sync 的收尾职责：宿主退出时把最后观察到的待落盘值交给它一次结清，core 的 Exit 臂只发收尾请求、不再自己拼保存调用（工单 20 的退出冲刷语义不变）。（模块头成文；每个 save_* 经唯一的 note_write_result 汇合，失败记一行 config write failed 到 moe_frame.log；行为测试见 tests/config_sync.rs 失败口径用例）
- [x] 工单 02 的改道在此落地：settings_session 发出的落盘类动作（文件夹、间隔、模式、显示方式、自启、几何）在宿主的动作执行分发处改走 config_sync；settings_session 本身不改。（shutdown(observed, saved) 返回结清后的记录，Exit 臂只发请求并回填基准；三态有测试）
- [x] 两套秒数规则（parse_interval_draft 与配置修复）在收拢时不得合并，ADR-0003 的区分保持。（execute_session_actions 文档与各臂已改道：文件夹/间隔/模式经核心 Save 效果、显示方式/自启/锁定/几何经同步方法；settings_session 本身未改）
- [x] 测试：启动翻译（含自启对账装配）在真实临时配置上验证；写回失败口径有行为测试（失败不阻塞、记日志、镜像待收敛）；全量测试绿、Windows 目标 check/clippy 通过；双轴 code-review 通过并写实施笔记。（config.rs 零改动，parse_interval_draft 与修复逻辑各自独立）

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 4（grilling Q1–Q8 定案）；父规格：[宿主层深挖规格](../spec.md)。

FrameCore 入参打包在本票兑现（工单 03 只减不打包）；ADR-0002/0003 规定的保存内容不变。

## Comments

- 2026-11-21 实现完成，状态置 needs-info：八条验收标准在本侧全满足；本票动了全部持久化路径与启动装配，宿主接线无自动化覆盖，实机项已追加到 `real-machine-check-deepening.md` 的工单 04 小节（重启恢复、自启对账、几何恢复、锁定持久化、写回失败可见），确认后转 resolved。实施笔记：`.agents/notes/implemented/feature/2026-11-21-config-sync.md`。
- 口径变化一处（标准 4 的既定结果，非漂移）：开机自启镜像的保存失败此前静默吞掉，现并入统一口径——多记一行日志，行为不变。
- 双轴审查（基线 2ed5882）：Standards 1 处硬违规已修（删除宿主 paths_to_strings 时文档注释残留到 app_directory 头上并留下重复 cfg 属性——复发模式的删除变体；注释随函数迁入 config_sync），判断项三条全采纳（几何记录统一用 config_sync 的 GeometryRecord 名、宿主 SavedGeom 别名删除；HostInit 字段文档补齐；write 改名 note_write_result）；Spec 八条全满足、行为等价 (a)–(g) 全成立、无范围蔓延。
- 2026-11-22 对齐（提交 `7861e5f`，决定见 `.agents/notes/implemented/feature/2026-11-22-image-driven-frame.md`）：`save_fit_mode`、`save_lock_aspect` 写回已随功能移除而删除；旧配置的 `display.fit_mode`、`window.lock_aspect` 由文件适配层在读取时清除。深挖实机清单工单 04 节的「宽高比锁定持久化」项已作废、「重启恢复」项已去掉显示方式。
