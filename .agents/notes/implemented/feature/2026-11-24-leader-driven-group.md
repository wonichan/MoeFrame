# Agent Note: 领队驱动整组（多实例吸附专题工单 06）

Status: implemented（VM 内可达标准；实机项见工单 06 未勾条目）

## Problem

路线 (a) 的后半（spec 第 15 节 / ADR-0005）：工单 05 已落地
WM_MOVING 实时吸附并留好接缝（DragSession.hits 与 EXITSIZEMOVE
清会话前锚点）。本票补齐方向关系、整组搬运与拆组语义，并整段
拆除路线 (b) 的轮询跟随引擎。

## Decision

- 库新增 snap_group（纯判定）：关系以「从属 → 基座」边表表达。
  - descendant_subtree：广度优先传递闭包，按关系表顺序展开；
    已访问集合断环（异常成环不死循环、根自身不计）。
  - plan_drag_start：拖动开始计划定死——solo（Shift）清基座
    属性且搬运集合为空；本窗有基座（卫生后）清基座属性、只
    带自己的子树；否则带整棵子树；存量接触临时从属并入搬
    运集合（solo 除外）。
  - 关系记录时机：WM_MOVING 每帧求解后，hits 非空且
    choose_base（取命中列表第一个，一窗至多一个基座）与会
    话已记录者不同才写属性；hits 为空不清属性。拖动结束关
    系以最后写入为准、留在窗上。拖动中吸上新基座只改关系，
    本次搬运集合不变、新基座不被搬运——领队仍是被拖窗，
    下次拖基座才按新树联动（mid_drag_new_base_… 测试钉住）。
  - 卫生校验 base_relation_valid：基座存活 + 同屏 + 相对边
    间隙 ≤ 阈值（拖动路径用物理阈值）；三种失效各一测试。
  - temporary_contact_ids：直接接触（工单 02 pair_in_contact，
    同屏 + 相对边贴合）且与本窗无任何方向关系者临时入树，
    不写属性；对齐不入、跨屏不入（接触判定天然同屏）。
  - 锚边邻居：anchor_neighbour_ids = 基座 + 直接从属 + 直接
    接触；anchor_edges 只对名单内邻居的接触产出锚边。
  - menu_detach_available 定死：本窗有基座属性或有直接接触。
  - frame_delta：一帧增量 = 最终落位左上 − 本窗当前实际左上。
- 属性薄层（宿主 snap_enum）：属性名 `MoeFrame.SnapBaseHwnd`
  （与工单 04 的 `MoeFrame.ExitAllRequested` 错开）；read_base /
  write_base / clear_base 以句柄值跨进程读写；parse_base_prop
  纯解析在库。relation_partners / drag_partner_infos 枚举时顺
  带读各伙伴基座组装关系表。
- 子类化层（snap_subclass）：WM_ENTERSIZEMOVE 顺序定死——枚
  举组装关系表 → 卫生校验清失效的本窗基座 → plan_drag_start
  裁决（solo / 有基座时清本窗属性）→ 存量接触临时成树 →
  定死搬运集合。Shift 采样时机沿工单 03 手势开始采样（宿主
  PointerDown 采样 solo_drag 并经 set_solo_drag 镜像给回调
  层），未改用 ENTERSIZEMOVE 时 GetAsyncKeyState。WM_MOVING
  每帧：求解改写拟拖矩形 → 记录关系 → 增量非零且搬运集合
  非空时以 BeginDeferWindowPos/DeferWindowPos/EndDeferWindowPos
  （SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER）单帧原子搬
  运。WM_EXITSIZEMOVE 只清会话，关系属性不清。
- 拆除：库 snap_follow 的 FollowEngine / PollSchedule /
  PollOutcome / PollPhase / 抑制标记整段删除，snap_follow 只
  剩 AnchorEdge + anchored_resize_position；宿主删除
  FrameApp.follow / poll_schedule / snap_contacts，geometry 的
  轮询块改为已唤醒帧上的锚边刷新（不新增唤醒），frame_ui 的
  next_frame_delay 删除吸附轮询参数，input 的轮询 wake 删除。
  菜单「脱离吸附组」改为经 snap_enum::clear_base 清本窗基座
  属性，出现条件改 snap_detach_available（geometry 刷新维护）。
- 外力移动吸收审计结论：观察 window_geom::GeometryMachine::
  observe 全路径——比例校正块的触发条件是 inner 尺寸变化 /
  insets 变化 / aspect 未应用 / 松手帧，纯 outer_pos 变化不
  进校正块；effective_pos 直接取实际观察位置，落盘防抖以实
  际观察组装记录。故伙伴被 HDWP 搬走后其几何机不发
  SetInnerSize / 位置校正、落盘跟随新位置，无打架点，不需
  要修；回归测试 external_position_only_move_is_absorbed_
  without_correction 钉住（tests/window_geom.rs）。

## Testing

- TDD：先在 tests/snap_group.rs 写新纯逻辑测试见红（E0432
  snap_group 模块不存在），再实现转绿。
- Linux 测试副本（refresh-moeframe-test-snap06.sh 刷新自本
  worktree）：cargo test --all-targets = 424 passed / 0
  failed。对账：基线 428 − 拆除 25（snap_follow 旧用例，逐
  条去向见工单 06 Comments）+ 新增 21（snap_group 20 +
  window_geom 审计回归 1）= 424。
- Windows 目标（x86_64-pc-windows-gnu）：cargo check
  --all-targets、cargo clippy --all-targets -- -D warnings、
  cargo fmt --all -- --check 三项全过。scripts/check.sh 本
  体未运行（Windows 专用脚本，本 VM 跑不了，不冒充）。
- 边界检查（safe_code_edits_and_commits）：最终 diff 专项复
  读无劈裂/孤儿文档注释；GLOSSARY Avoid 词 grep 全 diff
  零命中，功能命名一律「窗口吸附 / 吸附组」。
