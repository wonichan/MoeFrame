# 06: 领队驱动整组（方向关系属性树 / HDWP 子树搬运 / 拆组清属性）

**What to build:** 让贴合的相框真正按 PotPlayer 语义联动：吸附关系带方向（从属/基座，记为从属窗上的窗口属性），拖动开始时被拖进程算出自己的从属子树，拖动中由领队用 HDWP 把子树按同一增量原子搬走——拖基座带整树、拖从属先脱离再带自己的子树；Shift 独走与菜单脱离改为清属性。同时整段拆除路线 (b) 的轮询跟随引擎与抑制标记，接触派生降为存量接触临时成树与锚边邻居判定的辅助。

**Blocked by:** [05: WM_MOVING 实时磁吸](05-wm-moving-snap.md)

**Status:** needs-info

## Acceptance criteria

代码项：

- [x] 方向关系存储：从属窗以 Win32 窗口属性（SetProp/GetProp/RemoveProp）记基座 HWND，属性名定死并避免与工单 04「退出请求标记」等既有属性冲突；关系读写/清除收成宿主薄层 + 可测的纯判定（属性值解析、基座存活校验）。一窗至多一个基座；设属性只发生在工单 05 磁吸命中贴合时（对齐命中不成关系）与本票的存量接触临时成树。
- [x] 子树计算（纯逻辑可测）：给定全量窗口的「从属→基座」关系表，算出任一窗口的从属子树（传递闭包）；环路防御（关系数据异常成环时不死循环、按断环处理），用例覆盖单链、三层树、一基座多从属、无关系空集、环。
- [x] 整组搬运：WM_ENTERSIZEMOVE 时被拖进程读关系表算子树；WM_MOVING 每帧以被拖窗位移增量（与上帧拟拖/实际矩形之差，口径定死）用 BeginDeferWindowPos/DeferWindowPos/EndDeferWindowPos（SWP_NOACTIVATE | SWP_NOSIZE）搬子树，单帧原子提交；拖动结束（WM_EXITSIZEMOVE）收尾清状态。搬运只发生在被拖进程，伙伴进程拖动中不跑跟随逻辑。
- [x] PotPlayer 分离语义：拖从属时，它在拖动开始先清自己的基座属性（脱离），只带自己的子树走，原基座及其余从属不动；拖基座带整棵子树。拖动中吸上新基座：被拖窗设属性成为其从属（原有子树并入，由后续拖基座时按新树联动）。三种情形的判定逻辑纯函数化并有测试（以关系表 + 被拖窗输入、期望的清/设属性与搬运集合输出）。
- [x] 存量接触临时成树：启动后从未拖动过的已贴合窗口，拖动开始时按接触派生（工单 02 纯逻辑）把直接接触者临时视为被拖窗的从属参与本次搬运；临时关系在本次拖动内有效，拖动中若磁吸命中则按上一条转正为属性关系；临时成树的判定有测试（贴合者入树、对齐者不入、跨屏不入）。
- [x] 拆组清属性：Shift+拖动——本次拖动不带子树且被拖窗清基座属性（拖动开始判定，采样 Shift 的时机沿工单 03 已落地的手势开始采样）；右键菜单「脱离吸附组」改为清本窗基座属性（位置不变；菜单项出现条件改为「本窗有基座属性或有直接接触」，定死并注明）；工单 03 的抑制标记机制整段拆除，其测试逐条交代去向。
- [x] 卫生规则：拖动开始与磁吸求解前校验本窗基座属性——基座窗口已不存在（枚举不到/进程已退出）或两者已不接触（相对边间隙 > 吸附阈值）则清属性；校验为纯判定 + 薄宿主执行，有测试（基座消失、间隙超限、同屏变跨屏三种失效各一）。
- [x] 拆除轮询跟随：库 snap_follow 的 FollowEngine/PollSchedule 与宿主轮询挂接整段拆除，静止零重绘口径回归到「无吸附专题轮询」的基线（帧尾唤醒裁决不再并入任何吸附轮询间隔）；被拆路径的旧测试逐条交代去向（删除/改写及理由记 Comments），测试总数对账不低于基线减去已交代的拆除数。
- [x] 锚边变尺寸接关系树：工单 03 的锚边计算保留，邻居来源改按「属性关系（基座/直接从属）+ 直接接触」判定，不再按无方向连通分量；工单 03 锚边既有测试按新口径复核（改写处逐条记 Comments），与工单 06（切图溢出钳制）叠加顺序不变。
- [x] 伙伴被外力移动的吸收：审计 geometry 观察路径——伙伴被 HDWP 搬走后，其进程的几何机不得把这次纯位置变化误判为用户手势（不触发比例校正、不抢写落盘几何）；按「位置变了、尺寸没变」自然吸收进观察与防抖落盘（沿落盘以实际观察为准的口径）。发现打架点在本票修掉并加回归测试；无打架点也要在 Comments 写明审计结论（观察了哪条路径、为何不打架）。
- [x] 检查：`scripts/check.sh` 本体未运行（Windows 专用，本 VM 跑不了，不冒充）；替代口径全绿——Linux 测试副本 `cargo test --all-targets` 通过（增删逐项对账），Windows 目标 `cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all -- --check` 三项通过。

实机项（汇总进专题实机清单，返工收尾时重写对应条目）：

- [ ] 实机：三扇相框依次拖吸成链/树，拖基座（最先被吸靠的那扇）整组实时同移、贴合不散、无延迟追赶感。
- [ ] 实机：拖链中间/末端的从属窗，它先脱离、只带走它自己（及其从属），原组其余留在原地仍可整组拖动。
- [ ] 实机：Shift 拖动组内一扇，独自离开，其余成员纹丝不动；右键「脱离吸附组」后拖任意一边互不带动。
- [ ] 实机：重启程序后（关系不落盘）各窗回到落盘位置，直接拖已贴合的一扇，整组仍能一起动（存量接触临时成树）。
- [ ] 实机：把组员拖向另一块屏再拖回来，关系卫生规则不残留（不该联动的不会被陈旧关系拉走）。
- [ ] 实机：组内一扇开自动轮播切图变尺寸，共享边不动、邻居不被推走（锚边在新关系口径下仍成立）。

## Context

专题 spec 第 15 节与 ADR-0005 为本票口径来源。关键结构：协调权只在被拖进程手里——伙伴进程在别人拖动时不运行任何吸附逻辑，这是与旧路线 (b) 的根本区别，也是 Dormant 不对称、回声吞移、组图过期三类旧硬伤整类消失的原因。关系载体选窗口属性而非文件/注册表：跨进程可读、随窗口生灭、无落盘一致性问题；工单 04 已有「退出请求标记」属性的先例，命名与用法不要撞车。锚边变尺寸（工单 03）与级联/自启/同步（工单 01/04）的成果不在本票推翻范围。

## Comments

- 2026-11-24 用户拍板换路线 (a) 后建票（路线修订组第二票），待工单 05 完成后开工。

- 2026-11-24 工单 06 实现完成（impl/multi-instance-snap-06，基于 af4d696）：路线 (a) 后半落地——方向关系属性树 + 领队 HDWP 子树搬运 + 拆组清属性，路线 (b) 轮询跟随整段拆除。实机项不勾、Status 置 needs-info 待实机验收。
  - **关系时机（定死）**：属性名 `MoeFrame.SnapBaseHwnd`（与工单 04 的 `MoeFrame.ExitAllRequested` 错开，宿主 snap_enum 薄层 read/write/clear_base + 库 snap_group::parse_base_prop 纯解析）。WM_MOVING 每帧求解后，hits 非空且 choose_base（命中列表第一个，一窗至多一个基座，多 hit 取首个贴合命中）与会话已记录者不同才 SetProp；hits 为空不清属性（拖离后的陈旧关系由下次拖动开始的卫生规则清）。WM_EXITSIZEMOVE 只清会话，关系以最后写入为准留在窗上。
  - **拖动开始顺序（定死）**：WM_ENTERSIZEMOVE 枚举伙伴组装本地关系表（各伙伴基座属性 + 本窗属性）→ 卫生校验（基座存活/同屏/间隙 ≤ 物理阈值，失效清本窗属性）→ plan_drag_start 裁决：Shift solo（采样时机沿工单 03 手势开始采样，宿主 PointerDown 采样 solo_drag 并经 snap_subclass::set_solo_drag 镜像，未用 ENTERSIZEMOVE 时 GetAsyncKeyState）清属性且搬运集合为空；本窗有基座时清属性（拖从属先脱离）只带自身子树；否则带整棵子树；存量接触临时从属并入（不写属性；拖动中吸附命中贴合时按关系时机转正为属性关系）。搬运集合在拖动开始定死：拖动中吸上新基座只写属性成为其从属，本次仍由被拖窗领队搬运原集合、新基座不动，下次拖基座才按新树联动（snap_group 测试 mid_drag_new_base_does_not_switch_leader 钉住）。
  - **增量口径（定死）**：每帧增量 = 最终落位左上（求解改写后的拟拖矩形左上）− 本窗当前实际左上（帧首以自家 HWND 的 GetWindowRect 取得，物理像素）；吸附跳变包含在差内，子树随跳变同行；增量为零不调 HDWP。搬运以 BeginDeferWindowPos/DeferWindowPos/EndDeferWindowPos 单帧原子提交，标志 SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER。
  - **子树纯函数**：snap_group::descendant_subtree 广度优先传递闭包，已访问集合断环；五类用例齐全（单链/三层树/一基座多从属/无关系空集/环）。
  - **存量接触临时成树**：temporary_contact_ids 用 pair_in_contact（物理矩形喂入，贴合间隙为零天然满足容差）+ 同屏 + 与本窗无任何方向关系三条件；对齐不入树、跨屏不入树，有测试。
  - **卫生规则**：base_relation_valid 纯判定（存活 + 同屏 + separation ≤ 阈值），基座消失/间隙超限/同屏变跨屏三种失效各一测试；宿主在 ENTERSIZEMOVE 执行清属性。
  - **菜单脱离**：出现条件定死为 snap_detach_available = 本窗有基座属性或有直接接触伙伴（geometry 已唤醒帧上的锚边刷新维护）；点击经 snap_enum::clear_base 清本窗基座属性，位置不变；菜单其余项与顺序不动。锚边不再在此清（下一帧按新关系 + 直接接触重派生）。
  - **锚边接关系树**：GeomObservation.snap_anchor 接缝保留；宿主组装来源从旧跟随引擎改为 relation_partners 枚举 + snap_group::anchor_neighbour_ids（基座 + 直接从属 + 直接接触）→ anchor_edges；window_geom 内 anchored_resize_position 与「先锚边、后工单 06 钳制」顺序不变。
  - **拆除清单**：库 snap_follow 的 FollowEngine / PollSchedule / PollOutcome / PollPhase / 抑制标记（detach/detach_ids/is_suppressed）整段删除，snap_follow 只剩 AnchorEdge + anchored_resize_position；库 snap 的 MIN_MOVE 常量随之删除（无其他使用者）；宿主 FrameApp.follow / poll_schedule / snap_contacts 字段、geometry 轮询块与跟随命令落地、frame_ui::next_frame_delay 的吸附轮询参数、input 的 poll_schedule.wake、app::apply_window_snap_value 的引擎 reset/调度 sleep/wake 全部拆除。帧尾唤醒裁决不再含吸附轮询间隔，geometry 改为已唤醒帧上的一次关系枚举刷新锚边（不新增唤醒）。
  - **旧测试逐条去向（tests/snap_follow.rs 共 29 条，删 25、留/改 4）**：跟随 15 条（first_poll_only_establishes_history、follows_a_single_moved_contact_partner、quiet_partners_produce_no_command、a_partner_outside_the_group_is_not_followed、chain_group_middle_member_follows_either_side、chain_group_end_member_follows_through_the_middle、two_neighbours_moving_together_are_followed、two_neighbours_moving_apart_are_not_followed、a_vanished_partner_produces_no_command_and_no_phantom_history、own_external_move_is_never_followed_back、follow_echo_does_not_bounce_back、no_follow_while_own_window_is_being_dragged、disabled_switch_stops_the_engine_and_clears_history、follow_target_is_clamped_into_own_work_area、cross_monitor_partner_is_not_in_the_group）随 FollowEngine 删除——其语义（整组同移/跨屏脱组等）改由领队计划纯函数（snap_group 的 plan/subtree 用例）与实机项承接；抑制 5 条（detach_suppresses_follow_until_real_separation、suppression_survives_small_gaps_within_the_threshold、detach_without_contact_marks_nothing、direct_contacts_exclude_suppressed_partners、detached_partner_still_anchors_resize_but_never_pulls）随抑制标记删除——脱离语义改由 plan_drag_start 清属性与 menu_detach_available 用例承接；轮询调度 4 条（schedule_starts_probing_then_sleeps_without_partners、schedule_wake_rearms_a_dormant_schedule、schedule_heartbeat_is_low_frequency_and_active_is_frame_paced、schedule_active_settles_back_to_heartbeat_after_quiet_polls）随 PollSchedule 删除——静止零唤醒改由 frame_ui 改写的 no_snap_poll_wake_remains_after_follow_removal 承接；锚边派生 1 条（anchor_edges_report_the_shared_sides）改写移至 tests/snap_group.rs（anchor_edges_come_from_neighbour_contacts_only）；保留 4 条在本文件（right_anchor_grows_to_the_left、left_anchor_grows_to_the_right、bottom_anchor_wins_over_top_and_right_over_left 不动；shrinking_can_detach_the_contact_naturally 改写为走 snap_group 的 anchor_edges/direct_contact_ids）。工单 03 锚边在 tests/window_geom.rs 的 4 条（anchored_resize_keeps_the_right_edge_and_grows_left、anchored_resize_keeps_the_left_edge_and_grows_right、anchored_resize_is_followed_by_the_existing_work_area_clamp、resize_without_anchor_keeps_the_existing_position_rule）口径不变、仅类型路径仍走 snap_follow::AnchorEdge，未改写。frame_ui 的 follow_delay_is_the_lowest_priority_wake 改写为 no_snap_poll_wake_remains_after_follow_removal（1 换 1）。
  - **测试对账**：基线 428；删除 25（上条）；新增 21（tests/snap_group.rs 20 条：子树 5、拖动计划 4、临时成树 3、卫生 3、属性解析/菜单条件/帧增量 3、锚边邻居 2；tests/window_geom.rs 审计回归 1 条）；最终 428 − 25 + 21 = 424，Linux 测试副本 cargo test --all-targets = 424 passed / 0 failed。
  - **外力移动吸收审计结论**：审计 window_geom::GeometryMachine::observe 全路径——比例校正块触发条件只有 inner 尺寸变化 / insets 变化 / aspect 未应用 / 松手帧，纯 outer_pos 变化不进校正块；effective_pos 直接取实际观察位置，落盘防抖以实际观察组装记录。故伙伴被 HDWP 搬走后其几何机不发 SetInnerSize、不发位置校正，落盘按防抖跟随新位置——无打架点，不需修；回归测试 external_position_only_move_is_absorbed_without_correction 钉住（先落稳、纯位置移动帧零命令、防抖后 SaveGeometry 为新位置）。
  - **检查**：scripts/check.sh 本体未运行（Windows 专用，本 VM 跑不了，不冒充）；替代三件套全绿——Linux 测试副本 cargo test --all-targets（424/0，见上）、Windows 目标 cargo check --all-targets、cargo clippy --all-targets -- -D warnings、cargo fmt --all -- --check。边界检查（safe_code_edits_and_commits）：最终 diff 专项复读无劈裂/孤儿文档注释；GLOSSARY Avoid 词（磁吸作功能名/贴边停靠/窗口组/编组）grep 全 diff 零命中（文中「吸附跳变」为增量口径描述、功能名一律「窗口吸附/吸附组」）。README 的窗口吸附/拆组段与目录段已按新路线改述。
  - **遗留实机风险**：HDWP 在 winit 模态拖动循环内的实际搬运效果、子类化回调中 SetProp/GetProp 与 HDWP 的时序、被搬伙伴的 egui outer_rect 观察是否与 HDWP 落位一致、跨屏带运后卫生清理的实际表现，均只能实机验证（见实机项）；VM 内仅能保证编译、纯逻辑与回调链自洽。
