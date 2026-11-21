# 02: 设置窗会话机 settings_session

**What to build:** 设置窗的全部会话语义（打开/关闭/重开、两个草稿字段的时序、即时控件的点选收集与应用顺序）由一个库层状态机管理，工单 19 靠人肉逐帧推演验证的时序规则第一次有行为测试；宿主的 egui 闭包只剩采样与渲染。

**Blocked by:** None (can start immediately).

**Status:** needs-info

## Acceptance criteria

- [x] 库层新增 egui-free module `settings_session`：宿主每帧把采样到的事件（打开请求、字段编辑、焦点变化、回车、按钮点选、视口失焦、关闭请求）喂给它，它返回本帧要执行的动作；宿主负责采样、渲染与执行动作。（仅依赖 config 与 decode_validation 的类型；begin_frame/note_enter/note_blur/end_frame 喂入面完整，execute_session_actions 只执行）
- [x] 覆盖整个设置窗会话：打开/关闭/重开的种子值、路径与秒数两草稿的全部时序、即时控件（播放模式、间隔预设、显示方式、窗口几何项、开机自启）的点选收集与应用顺序；即时控件的实际应用由宿主执行 module 发出的动作。（open_requested 管播种，end_frame 按定序产出五类即时控件动作，执行全在宿主）
- [x] 设置窗的定位居中计算留在宿主，不进本 module。（ViewportBuilder/SETTINGS_SIZE/with_position 未动，module 内无定位概念）
- [x] settings_draft 被吸收为 settings_session 的内部实现，不再是公开面；tests/settings_draft.rs 的用例改走新会话 interface 表达，不保留双测试面。（settle 逐行对应旧两函数；旧 7 用例 5 个直接对应、2 个补加：无效/空白路径逐字提交、秒数同值回车无操作）
- [x] 「延迟一帧结算」内化为 module 规则：宿主每帧渲染后喂「帧结束」事件，待结算在帧结束时结算；同帧到达的关闭/重扫事件先取消待结算（重扫否决、关闭丢弃都是 module 内部的事件顺序，不再靠宿主调用顺序）。（blurred 帧尾转 pending、次帧帧尾结算；close/rescan 分支在结算前清 pending 并返回）
- [x] 外部改值不设通知通道：宿主每帧随事件递交当前生效值快照，module 与基线比对并作废对应字段的草稿。（begin_frame 逐字段比对并取消该字段 pending；宿主 effective_values 每帧现读）
- [x] 行为测试覆盖四条时序规则：帧结束结算（含回车后焦点保留的语义等价）、重扫否决、关闭丢弃、快照作废；外加打开种子与重开显示生效值。（17 项会话测试全绿：帧尾结算、重扫两形态、关闭丢弃、快照作废，外加种子/重播种、视口失焦合成、预设推进作废、点选顺序与守卫）
- [x] 本票的落盘类动作仍由宿主直接执行（调核心、写配置照旧），不预留工单 04 的接口；工单 04 落地时改道。（execute_session_actions 全部直执，公开面无持久化抽象；改道点已记入实施笔记 Consequences）
- [x] 全量测试绿、Windows 目标 check/clippy 通过；show_settings 闭包显著变薄；双轴 code-review 通过并写实施笔记。（全量 198/198；Windows check 通过、clippy 产品侧零警告；宿主 1623 → 1529 行；双轴审查见 Comments；实施笔记见下）

## Context

父决策：`.agents/notes/proposed/architecture/2026-11-21-host-deepening-plan.md` 候选 2（grilling Q1–Q7 定案）；父规格：[宿主层深挖规格](../spec.md)。

语义边界沿用 ADR-0003 与工单 19 定案，本票只搬家不改语义；三个 P1 项不出现在设置窗的约束不变。

## Comments

- 2026-11-21 实现完成，状态置 needs-info：九条验收标准在本侧全满足，但宿主接线（egui 焦点跟踪、视口焦点采样、TextEdit 借文字渲染）无自动化覆盖且未在真实 Windows 上运行——设置窗交互的实机确认并入仓库根目录 `real-machine-check-deepening.md` 的工单 02 一节，按专题顺序在工单 01–05 后、发布门前同轮执行，确认后转 resolved。实施笔记：`.agents/notes/implemented/feature/2026-11-21-settings-session.md`。
- 结构决定：会话状态（open、两草稿、pending、焦点记忆、视口焦点记忆、模式快照）全归 SettingsSession；宿主 settings_open 只镜像窗口存在性。帧尾结算的生效值在 module 内随点选推进，与旧宿主 commit 时现读等价（双轴审查 (c) 项核对）。
- 双轴审查（基线 1bcc653）：Standards 无硬违规，判断项三条全采纳（OpenRequest 改名 OpenDecision、旁白注释改陈述式、文档标点）；Spec 九条全满足、行为等价 (a)–(g) 全成立、无范围蔓延；审查中补加 2 项旧用例对应测试（无效/空白路径、秒数同值）。
