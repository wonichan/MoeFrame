# 05: WM_MOVING 实时磁吸（子类化 / 拖动中求解改矩形 / 求解器评分化）

**What to build:** 把吸附从「松手瞬间算一次」换成 PotPlayer 式「拖动中实时磁吸」：给相框窗口挂窗口过程子类化，`WM_MOVING` 到达时以拟拖矩形 + 伙伴矩形 + 工作区做吸附求解，命中就地改写拟拖矩形，窗口在拖动过程中就被吸住。求解器同步做二维评分化改造（吸收旧路线诊断结论）。本票只做被拖窗自身的磁吸落位，不做整组搬运与关系记录（工单 06）；但求解命中「贴合到某伙伴」这一事实要以可供工单 06 消费的形态留出接缝（命中伙伴身份 + 贴合边）。

**Blocked by:** [02: 实例发现与松手吸附](02-window-discovery-snap.md)（枚举校验层与接触派生保留复用；其松手吸附路径由本票取代）

**Status:** needs-info

## Acceptance criteria

代码项：

- [x] Spike 先行：在集成分支之外验证「SetWindowSubclass 挂到 eframe/winit 相框窗口 + 在 WM_MOVING 里改写 lParam 矩形 + winit 原生模态拖动循环（含松手重校正、拖动回弹）共存不打架」，结论（可行/不可行的具体形态、注意事项）记入本票 Comments；spike 不可行时停下回报，不硬铺实现。
- [x] 子类化边界：只挂相框窗口 HWND（设置窗不挂），严格走 DefSubclassProc 链转发所有消息；子类化安装/卸载时机明确（窗口创建后装、退出前卸），不改动 winit 自有 subclass 的行为；既有 NativeGesture 旗标、模态循环结束判定、回弹修复路径零改动且其测试不回归。
- [x] 实时磁吸求解：WM_MOVING 处理以拟拖矩形（物理像素）为被拖窗矩形、工单 02 枚举层给出的伙伴物理矩形、拟拖矩形所在显示器工作区（rcWork 物理像素）为输入；命中贴合/对齐/工作区边缘时就地改写拟拖矩形至目标，未命中时不改。拖动全程每帧可重复求解（同一拖动内多次进出阈值行为稳定、无抖动黏滞异常，判定口径在实现中定死并有测试或注释说明）。
- [x] 坐标纪律：本路径全程物理像素；本窗矩形不用 egui outer_rect（需要本窗现状时用自家 HWND 的 GetWindowRect）；阈值 12 逻辑 px 按本窗 DPI（GetDpiForWindow）换算物理像素，换算有纯函数测试（含 100%/125%/150% 三档）。
- [x] 求解器评分化（库内纯逻辑）：候选按二维评分裁决——投影重叠比例与间隙距离参与评分；贴合优先于对齐（同等邻近程度下）；对齐候选加另一轴邻近约束（另一轴相距过远不触发对齐）；工作区边缘不凭类别压过明显更近的伙伴。用例至少覆盖旧诊断点名的反例：X/Y 两轴吸向不同伙伴、仅擦边投影的伙伴压过正对伙伴、远处伙伴对齐抢走近处贴合、工作区贴合压过更近的伙伴对齐。旧单轴类别裁决被替换的旧测试逐条交代去向（改写/删除及理由记 Comments）。
- [x] 拆除松手吸附路径：工单 02 的松手求解挂接（几何机 GeomObservation.snap 消费、松手帧落位）整段移除，几何机恢复到吸附专题之前的手势口径（松手只做既有重校正）；被移除路径的旧测试逐条交代去向。吸附开关关闭时子类化回调不求解、不改矩形（开关判定与工单 02 落地共用同一开关值）。
- [x] 几何机分工不打架：WM_MOVING 改矩形后的落位经 OS 模态循环成为窗口实际位置，几何机在手势中的沉默口径不变；松手后的观察/防抖落盘按既有机制吸收新位置（以实际观察为准），不出现吸附落位被松手重校正顶开的路径（审计两者同帧顺序并在 Comments 写明结论）。
- [x] 检查：`scripts/check.sh` 本体未运行（Windows 专用，本 VM 跑不了，不冒充）；替代口径全绿——Linux 测试副本 `cargo test --all-targets` 通过数不低于基线 428（增删逐项对账），Windows 目标 `cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all -- --check` 三项通过。

实机项（汇总进专题实机清单，返工收尾时重写对应条目）：

- [ ] 实机：双开相框，拖一扇缓慢靠近另一扇，**拖动过程中**在阈值内即被吸住贴合（不松手也能看到贴合），拖离阈值可正常拖走、无黏滞。
- [ ] 实机：拖相框靠近屏幕工作区四边，拖动中即贴边；靠近伙伴的对齐边时拖动中对齐成线。
- [ ] 实机：拖动中反复进出阈值、快速晃动，窗口不抖、不跳错位；松手位置与拖动中最后所见一致。
- [ ] 实机：关掉吸附开关后拖动，全程不吸附。
- [ ] 实机：吸附落位后切图/等自动轮播，尺寸校正与落盘正常（与几何机无打架）；原生拖动回弹问题不复现。

## Context

专题 spec 第 15 节（路线修订）与 ADR-0005 为本票口径来源；第 3 节形态与 12px 阈值不变。旧路线诊断（2026-11-24）确证：松手吸附无拖动反馈是手感差的第一根因，求解器单轴类别裁决是「吸不准」的代码因素，两者在本票一并解决。工单 02 的 snap_enum 枚举层（标题 + exe 校验、GetWindowRect、rcWork、GetDpiForWindow）已齐备，本票复用并按物理像素口径调整消费方式。子类化与 winit 共存有同栈公开前例，但本仓库零先例——故 spike 先行。

## Comments

- 2026-11-24 用户拍板换路线 (a) 后建票（路线修订组第一票），待 spike 与实现。

- 2026-11-24 工单 05 实现完成（分支 impl/multi-instance-snap-05，代码提交 4c9ee40，基于集成分支 95b61be；VM 内验收口径，Status 置 needs-info 待协调者验收与实机清单）。
  - **Spike 结论：可行（VM 内可达标准）**。HWND 提取通路现成（装配处经 raw-window-handle 取 Win32 HWND 存 FrameApp.hwnd，install 直接接其后）；windows-sys 0.61 的 Win32_UI_Shell feature 含 SetWindowSubclass/RemoveWindowSubclass/DefSubclassProc（Cargo 只加此一 feature），WM_MOVING/ENTERSIZEMOVE/EXITSIZEMOVE、MonitorFromRect、GetDpiForWindow 均现成，Windows 目标 check/clippy 全过、回调签名与 SUBCLASSPROC 精确对齐；共存形态为全消息 DefSubclassProc 收尾转发，NativeGesture 旗标/模态结束判定/回弹路径零改动且测试不回归。未发现结构性不可行项。实机才能证的：WM_MOVING 在 winit 模态循环中如期到达子类化过程、改写 RECT 后落位与观察一致性——留实机项。
  - **子类化与数据传递设计**：新模块 src/bin/moe-frame/snap_subclass.rs，只挂相框 HWND（装配 install、Effect::Exit 前 uninstall，设置窗不挂）。回调无 &mut App 访问——回调在窗口所在 UI 线程由消息泵触发，数据走进程级静态：开关值 AtomicBool 镜像（apply_window_snap_value 同步，与工单 02 落地共用同一开关值；关闭时回调不求解不改矩形），拖动会话 Mutex<Option<DragSession>>（每进程一扇相框窗、会话至多一份，实际访问全在 UI 线程无竞争）。会话在 WM_ENTERSIZEMOVE 经 snap_enum::drag_partners 枚举一次（复用标题 + exe 校验，矩形另取物理 GetWindowRect 带句柄），WM_MOVING 每帧逐个 GetWindowRect 刷新伙伴矩形（销毁剔除）、工作区取拟拖矩形所在屏 rcWork（MonitorFromRect，物理）、阈值按本窗 GetDpiForWindow 换算。
  - **求解器评分化（库 snap::solve_drag_snap，物理像素）**：两轴独立裁决组合；贴合成本 = 间隙 + (1 − 另一轴投影重叠比例) × 阈值；对齐成本 = 间隙 + 另一轴相距距离 + 阈值/2 类别罚分（同等邻近贴合优先、明显更近的对齐可赢更远贴合）；对齐加另一轴邻近约束（相距超阈值不产生候选）；工作区与伙伴同尺竞争、生成在伙伴之后（同成本伙伴优先）；同成本先生成者胜。纯函数无状态、目标位幂等——反复进出阈值无黏滞/漂移的判定口径即此。工单点名四反例均有测试：drag_counterexample_axes_attach_to_different_partners / _grazing_projection_loses_to_head_on_partner / _far_alignment_does_not_steal_near_attach / _work_area_does_not_beat_nearer_partner_by_category。阈值换算 physical_snap_threshold 带 100/125/150% 三档测试（threshold_scales_with_dpi、drag_physical_threshold_governs_the_gap）。
  - **工单 06 接缝**：库层 DragSnapOutcome { target, hits: Vec<SnapHit> }，SnapHit { partner_hwnd: u64, edge: SnapEdge::{Left,Right,Top,Bottom}, target: WindowRect }（物理像素；对齐/工作区命中不产出）；宿主 DragSession { partners, hits, last_target } 每帧更新 hits，WM_EXITSIZEMOVE 清会话处留「清前消费 hits」注记锚点。本票不设窗口属性、不搬伙伴。
  - **几何机分工审计结论**：WM_MOVING 改写经 OS 模态循环成为实际位置；几何机手势中照旧沉默，松手帧重校正的位置平移只有既有工作区溢出钳制一条路径且输入为实际观察位置，不存在把吸附落位顶开的路径；落盘以实际观察为准。两者同帧顺序无打架点。
  - **拆除清单**：宿主 geometry.rs 松手 SnapRequest 组装整段、库 window_geom 的 GeomObservation.snap 字段与松手消费块、snap_enum::release_snap_request 与 enumerate_candidates、库 solve_snap/SnapRequest/SnapPartner/AxisSolver。snap_anchor 锚边接缝（工单 03）保留不动；轮询跟随引擎不在本票拆除范围（归工单 06 路线收尾）。
  - **旧测试逐条去向**：tests/snap.rs 旧 solve_snap 用例 12 条全删（solve_attaches_right_edge_and_zeroes_the_gap、solve_attaches_each_relative_edge、solve_beyond_threshold_does_nothing、solve_at_exact_threshold_still_attaches、solve_aligns_same_name_edges_without_contact、solve_attach_wins_over_alignment、solve_attach_on_one_edge_beats_smaller_gap_alignment_on_the_other、solve_nearest_partner_wins_and_tie_keeps_enumeration_order、solve_combines_axes_attach_on_x_align_on_y、solve_work_area_edge_zeroes_gap_without_reserve、solve_ignores_partners_on_other_monitors、solve_already_touching_is_no_command）——旧单轴类别裁决已被评分求解替代，对应用例由新拖动求解用例覆盖（贴合四边/阈值内外/对齐/工作区/幂等/四反例/DPI 换算等共 14 条新用例）；其中 solve_ignores_partners_on_other_monitors 的同屏口径：原交代称「改由宿主组装层承担」并不准确（drag_partners 实际给全量伙伴、组装层未过滤）——审查修复 F3 已改为与实现一致的形态：伙伴的显示器事实在拖动开始时与伙伴成对保存，WM_MOVING 每帧由宿主以拟拖矩形所在显示器调用库纯函数 snap::drag_partners_on_monitor 过滤后才进 solve_drag_snap（求解器本身仍不带 monitor 字段，过滤判定沉库可测）。tests/window_geom.rs 删 2 条（release_frame_applies_snap_target_via_position_command、snap_request_without_a_release_is_ignored，随 GeomObservation.snap 接缝拆除），release_without_snap_request_keeps_nominal_position 改名 release_frame_keeps_nominal_position_without_a_target 保留（松手无目标保持名义位仍是现行口径）。
  - **检查数字**：Linux 测试副本 cargo test --all-targets = 428 passed / 0 failed（基线 428；删 14 加 14 持平）。Windows 目标 cargo check --all-targets、cargo clippy --all-targets -- -D warnings、cargo fmt --all -- --check 三项全过。scripts/check.sh 本体未运行（Windows 专用，本 VM 跑不了，不冒充）。边界检查（safe_code_edits_and_commits）：最终 diff 专项复读无劈裂/孤儿文档注释；GLOSSARY Avoid 词检查口径（审查修复 S3 改述）：「磁吸」作功能名零命中（diff 内的「磁吸命中/实时磁吸」均为动作描述、功能名一律「窗口吸附」）、「贴边停靠」零命中——不冒充全 diff 对「磁吸」二字零命中。
  - **实机项**：下方五条实机验收全部未勾，汇总进专题实机清单；另注意 spike 遗留风险——WM_MOVING 到达率与改写生效、outer_rect 观察与实际落位一致性、回弹问题不复现，均只能实机证。

- 2026-11-24 双轴审查修复（分支 impl/multi-instance-snap-review-fixes，基于 be46780；代码提交 e8cfab9、文档提交见本笔；Status 保持 needs-info 不变）。
  - **F3（中）WM_MOVING 求解无同屏过滤**：伙伴的显示器事实在拖动开始时与伙伴成对存入 DragSession，WM_MOVING 每帧由宿主以拟拖矩形所在显示器（新增 snap_enum::monitor_rect_for_physical_rect）调用库纯函数 snap::drag_partners_on_monitor 过滤后才进 solve_drag_snap；所在屏取不到时按全量求解（MonitorFromRect 带 NEAREST 兜底，实践中恒有值）。新增用例 drag_partners_filtered_to_proposed_monitor。原 Comments 里「同屏口径改由宿主组装层承担」的旧交代已在原文处改述为与新实现一致（见上方旧测试去向条）。
  - **S3（低）Avoid 词记录口径**：原 Comments 的「grep 全 diff 零命中」已在原文处改述为「磁吸作功能名零命中」，不冒充全 diff 对「磁吸」二字零命中（与工单 06 口径一致）。
  - **S4/F8（低）死代码拆除**：src/snap.rs 的 snap_groups / derive_contacts / Contact 整簇删除（生产已无调用方；pair_in_contact 保留——snap_group 的临时成树/锚边/卫生在用，并已成为唯一的接触判定，见工单 06 的 S5）；tests/snap.rs 对应 7 条接触派生用例一并删除（contact_single_shared_edge、contact_chain_of_three_forms_one_group、contact_l_shape_two_edges、contact_none_when_apart、contact_gap_beyond_tolerance_does_not_count、contact_across_monitors_does_not_count、contact_requires_projection_overlap）——其接触几何语义由 tests/snap_group.rs 的临时成树/直接触/锚边用例继续覆盖。删后 lib 公共面无其他引用跟不上（Windows check/clippy 全过为证）。
  - **检查数字**：Linux 测试副本 cargo test --all-targets = 424 passed / 0 failed；Windows 目标 cargo check --all-targets、cargo clippy --all-targets -- -D warnings、cargo fmt --all -- --check 三项全过。scripts/check.sh 本体未运行（Windows 专用，本 VM 跑不了，不冒充）。测试对账（与工单 06 合并记账）：基线 424 − 删 7（本条 S4）+ 增 7（F3 本票 1 条 + 工单 06 的 F1/F2 5 条、F4 对角反例 1 条）= 424，另 F5 菜单判定用例改名改义 1 条（净零）。逐项明细见工单 06 同日 Comments。
