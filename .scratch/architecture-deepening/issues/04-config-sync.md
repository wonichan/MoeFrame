# 04: 持久化镜像与启动装配 config_sync

**What to build:** 「哪个值以谁为准、何时写回」只有一个出处：启动翻译产出一个启动包，全部镜像写回经一个 module 落盘；新增一个持久化设置的改动面从六处降到「config_sync + UI」两处。

**Blocked by:** [02: 设置窗会话机 settings_session](02-settings-session.md)（其落盘类动作在本票改道）。

**Status:** ready-for-agent

## Acceptance criteria

- [ ] 库层新增 module `config_sync`，两头收：(a) 启动翻译：配置 → 一个启动包（FrameCore 构造入参打包 + 宿主初值打包），run() 只做分发，不再逐字段翻译；(b) 全部镜像写回：core 的 Save 系列 Effect 由它执行，宿主直写的显示方式、窗口几何、宽高比锁定、开机自启镜像也经它落盘。
- [ ] 宿主不再直接调用 ConfigStore 的 setter；ConfigStore 不合并、不改其字段级修复语义，降为 config_sync 背后的文件 adapter；三种镜像模式（核心状态镜像、宿主状态镜像、注册表反向对齐）在 config_sync 内显式成文。
- [ ] 开机自启只收两环：AutostartSync 的镜像写回改经 config_sync 落配置，启动时注册表↔镜像对账的装配归启动翻译；AutostartSync 的决策与 RegAutostartStore 不搬家。
- [ ] 写回失败统一口径（三种镜像一致，明文成文）：不阻塞行为、不弹窗、不重试，记一行到现有日志文件；镜像偏差靠下一次启动的对账/字段修复收敛。
- [ ] 退出冲刷归 config_sync 的收尾职责：宿主退出时把最后观察到的待落盘值交给它一次结清，core 的 Exit 臂只发收尾请求、不再自己拼保存调用（工单 20 的退出冲刷语义不变）。
- [ ] 工单 02 的改道在此落地：settings_session 发出的落盘类动作（文件夹、间隔、模式、显示方式、自启、几何）在宿主的动作执行分发处改走 config_sync；settings_session 本身不改。
- [ ] 两套秒数规则（parse_interval_draft 与配置修复）在收拢时不得合并，ADR-0003 的区分保持。
- [ ] 测试：启动翻译（含自启对账装配）在真实临时配置上验证；写回失败口径有行为测试（失败不阻塞、记日志、镜像待收敛）；全量测试绿、Windows 目标 check/clippy 通过；双轴 code-review 通过并写实施笔记。

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 4（grilling Q1–Q8 定案）；父规格：[宿主层深挖规格](../spec.md)。

FrameCore 入参打包在本票兑现（工单 03 只减不打包）；ADR-0002/0003 规定的保存内容不变。
