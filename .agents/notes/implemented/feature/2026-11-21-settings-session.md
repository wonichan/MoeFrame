# Agent Note: 设置窗会话机 settings_session（深挖工单 02）

Status: implemented

## Problem

工单 19 把设置窗的草稿时序（失焦延迟一帧结算、重扫否决、关闭丢弃、快照作废、视口失焦合成）实现在宿主 show_settings 里：决策与 egui 采样交织在一个约 200 行的函数中，靠逐帧人肉推演验证，零自动化覆盖；唯一可测的部分是抽出的两条纯结算函数（settings_draft），而真正的 bug 面在调用顺序里（父决策见 proposed 笔记 2026-11-21-host-deepening-plan 候选 2）。

## Decision

- 新增库层 egui-free module `settings_session`，类型 `SettingsSession`，形状与手势机一致：宿主每帧帧首递交生效值快照（`begin_frame(EffectiveValues)`：文件夹文字、间隔、播放模式），渲染中借它的草稿文字与焦点记忆并记录回车/失焦（`note_enter`/`note_blur`），帧尾递交点选（`end_frame(Picks, viewport_focused)`）换回本帧动作列表（`Vec<SessionAction>`），宿主在 `execute_session_actions` 里按序执行。
- 会话覆盖整个设置窗：`open_requested` 管打开播种与已开只聚焦（返回裁决 `OpenDecision`）；关闭在 end_frame 内落定（open 标志归会话，宿主的 settings_open 只镜像窗口存在性）；设置窗定位居中留宿主。
- 延迟一帧内化为帧尾规则：失焦当帧只记 blurred，帧尾转 pending，下一帧帧尾结算；关闭/重扫分支在结算前清 pending 并返回——否决与丢弃是 module 内部的事件顺序，不再靠宿主 if/else 的排列。
- 外部改值无通知通道：begin_frame 逐字段比对基线与快照，不一致即作废重播种并取消该字段的 pending。帧尾结算用的生效值在本模块内随点选推进（间隔预设在 end_frame 内先推进 interval_now，待结算秒数按新基线比对作废），与旧宿主在 commit 时现读生效值等价。
- 视口失焦合成在 end_frame 开头：焦点 true→false、无既有回车/失焦记录、字段持有焦点记忆时合成该字段失焦，走同一条延迟路径。
- settings_draft 被吸收：两条结算函数成为 `SettingsSession::settle` 的内部实现（语义逐行对应），module 与其测试删除；旧 7 用例改走会话 interface（5 个直接对应、2 个补加：无效/空白路径逐字提交、秒数同值回车无操作）。
- 落盘类动作本票仍由宿主直执（核心意图、apply_fit_mode、set_lock_aspect、apply_autostart），公开面不预留任何持久化抽象；工单 04 落地时只改 execute_session_actions 的分发。

## Alternatives considered

- 只把结算时机搬进 module、点选应用顺序留宿主：bug 面恰在顺序，半搬等于没搬——父评审已否决。
- 会话发通知通道让宿主推送外部改值：多一条双向耦合，快照比对已足够且与手势机的喂入形状一致——父评审已否决。
- 给工单 04 预留动作执行接口：违反「不为假想接缝付费」，改道点已定位在宿主分发一处——父评审已否决。

## Testing

- tests/settings_session.rs 17 项按帧驱动：打开播种/二次请求只聚焦、打字不提交、失焦次帧帧尾结算、回车同帧结算、重扫否决（跨帧待结算 + 同帧失焦两种形态）、关闭丢弃 + 重开重播种、快照作废（含 pending 一并取消）、预设推进生效值作废待结算、解析失败保值回显、越界钳制回显、超大数字串钳制、同值路径无操作、无效/空白路径逐字提交、秒数同值无操作、点选顺序与同模式守卫、视口失焦合成。
- 全量测试 198 项全绿（196 + 补加 2）；Windows 目标 check 通过，clippy 产品侧零警告；cargo fmt 已应用。
- 双轴 code-review（基线 1bcc653）：Standards 无硬违规，判断项三条全采纳（OpenRequest 改名 OpenDecision 对齐裁决枚举惯例、end_frame 旁白注释改陈述式、track_draft_field 文档标点）；Spec 九条标准全满足，行为等价 (a)–(g) 逐项成立（动作顺序、延迟/否决/丢弃、快照作废与点选推进、三处守卫、两套秒数规则未合并），无范围蔓延。

## Consequences

- 宿主文件 1623 → 1529 行；8 个草稿相关宿主字段收成 1 个 session 字段，show_settings 只剩定位、采样、渲染与动作执行。
- 宿主接线（egui 焦点跟踪、TextEdit 借文字、视口采样）仍无自动化覆盖：本票置 needs-info，设置窗交互的实机确认并入 real-machine-check-deepening.md 的工单 02 一节，与工单 01–05 其余各节同轮执行。
- 工单 04 的改道点已明确：execute_session_actions 中 CommitFolder/CommitInterval/ApplyMode 的核心落盘 Effect 与 ApplyFitMode/ApplyAspectLock/ApplyAutostart 的直写，届时改走 config_sync；settings_session 本身不动。
- 工单 05 的宽高比锁定开关经本会话的 ApplyAspectLock 动作发出、宿主转发几何状态机——接线点就是 execute_session_actions 的该分支。
