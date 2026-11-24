# Agent Note: WM_MOVING 实时吸附（多实例吸附专题工单 05）

Status: implemented（VM 内可达标准；实机项见工单 05 未勾条目）

## Problem

路线 (b)（松手贴合 + 轮询跟随）被用户实机否决（spec 第 15 节 /
ADR-0005）：松手吸附拖动全程零反馈、求解器单轴类别裁决吸不准。
本票换路线 (a) 的前半：给相框窗口挂窗口过程子类化，WM_MOVING
阶段就地改写拟拖矩形实现拖动中实时吸附；求解器同步二维评分
化。整组搬运与方向关系记录归工单 06，本票只留接缝。

## Spike 结论（VM 内可达标准：能编译、逻辑自洽、回调链正确）

- HWND 提取通路现成：装配处（src/bin/moe-frame.rs）经
  raw-window-handle 从 winit 窗口取 Win32 HWND 存入
  FrameApp.hwnd，本票在其后直接 install，无需新通路。
- windows-sys 0.61 已含全部所需 API：SetWindowSubclass /
  RemoveWindowSubclass / DefSubclassProc 在 Win32_UI_Shell
  feature（comctl32），Cargo.toml 只加了这一个 feature；
  WM_MOVING(0x0216) / WM_ENTERSIZEMOVE / WM_EXITSIZEMOVE、
  MonitorFromRect、GetDpiForWindow 均为既有 feature 内符号。
  Windows 目标 check/clippy 全过，回调签名与 SUBCLASSPROC 类型
  精确对齐。
- 共存形态：子类化过程对所有消息（含处理过的三条）一律以
  DefSubclassProc 收尾转发，winit 自有 subclass 链不断；
  NativeGesture 旗标、GetAsyncKeyState 模态结束判定、松手重校
  正/回弹路径零改动，其测试不回归（428 全绿含几何机全部用例）。
- 未能 VM 内实证（留实机）：WM_MOVING 在 winit 模态循环里是否
  如期到达子类化过程、改写 RECT 后 egui 的 outer_rect 观察是否
  与实际落位一致、与回弹修复的同帧顺序。结构性不可行项未发现。

## Decision

- 库 snap 新增物理像素拖动求解 `solve_drag_snap`，删除旧单轴
  类别裁决 `solve_snap` 与 `SnapRequest` / `SnapPartner`：
  - 评分（文件头定死）：两轴独立裁决组合；贴合成本 = 间隙 +
    (1 − 另一轴投影重叠比例) × 阈值；对齐成本 = 间隙 + 另一轴
    相距距离 + 阈值/2（类别罚分——同等邻近下贴合优先，明显更
    近的对齐可赢过更远的贴合）；对齐加另一轴邻近约束（相距超
    阈值不产生候选）；工作区贴合与伙伴候选同尺竞争、生成顺序
    在伙伴之后（同成本伙伴优先）。同成本先生成者胜（替换只在
    成本严格更小时发生）。
  - 纯函数无状态：同一输入恒得同一输出，已在目标位时目标与现
    位一致（幂等）——反复进出阈值无黏滞/漂移的口径即此。
  - `physical_snap_threshold(dpi)`：12 逻辑 px × dpi/96，0 退
    回逻辑值；100/125/150% 三档有测试。
  - 接缝：`DragSnapOutcome { target, hits: Vec<SnapHit> }`，
    `SnapHit { partner_hwnd, edge: SnapEdge, target }`（物理像
    素）；对齐与工作区命中不产出 SnapHit。
- 宿主新模块 snap_subclass（子类化薄壳）：
  - 只挂相框 HWND（装配处 install、Effect::Exit 前 uninstall；
    设置窗是另一个 HWND，不挂）。
  - 线程模型：回调在窗口所在 UI 线程由消息泵触发，无 &mut
    FrameApp 访问。故数据走进程级静态——开关值 AtomicBool 镜
    像（apply_window_snap_value 同步写入，与宿主 window_snap
    同一开关值）；拖动会话 Mutex<Option<DragSession>>（每进程
    一扇相框窗、会话至多一份，Mutex 只为静态 Sync，实际访问
    全在 UI 线程无竞争）。
  - 会话：WM_ENTERSIZEMOVE 经 snap_enum::drag_partners 枚举一
    次伙伴（复用工单 02 的标题 + exe 校验判定，矩形另取物理
    GetWindowRect 并带句柄）；WM_MOVING 每帧逐个
    GetWindowRect 刷新伙伴矩形（销毁者剔除），工作区取拟拖矩
    形所在屏 rcWork（MonitorFromRect，物理），阈值按本窗
    GetDpiForWindow 换算；命中改写 lParam 的 RECT 并把 hits /
    last_target 存入会话；WM_EXITSIZEMOVE 清会话（工单 06 在
    此处清会话前消费 hits——代码里已留注记锚点）。开关关闭时
    回调不求解、不改矩形。
  - DragSession 字段（partners / hits / last_target）即工单 06
    的扩展点：06 在其上加子树搬运钩子与属性关系记录。
- snap_enum 新增物理供给：drag_partners /
  partner_rect_physical / work_area_for_physical_rect / own_dpi；
  删除 release_snap_request 与其专属的 enumerate_candidates。
- 拆除松手吸附：geometry.rs 的松手 SnapRequest 组装整段移除
  （移动手势结束只剩清旗标 + 调度 wake + solo 清理）；库
  window_geom 的 GeomObservation.snap 字段与松手帧消费块移
  除。工单 03 的 snap_anchor 锚边接缝是另一条路径，保留不动。
- 几何机分工审计：WM_MOVING 改写后的落位经 OS 模态循环成为
  窗口实际位置；几何机在手势中（pointer_down / native_gesture
  期间）照旧沉默，松手帧的重校正只在手势结束时按名义高度做
  比例校正——其位置平移只有既有的工作区溢出钳制一条路径，
  输入是实际观察位置，不存在「按旧位置把吸附落位顶开」的路
  径；落盘以 effective_pos = 实际观察为准。故两者同帧顺序结
  论：子类化层先在拖动中定位置，几何机松手后只吸收、不回改。

## Testing

- TDD：先在 tests/snap.rs 写新求解测试见红（E0432 旧入口不存
  在/新入口缺失），再实现转绿。
- Linux 测试副本（refresh-moeframe-test-snap05.sh 刷新自本
  worktree）：cargo test --all-targets = 428 passed / 0 failed，
  与基线 428 持平。对账：删除 14（旧 solve_snap 用例 12 +
  window_geom 松手接缝用例 2）、新增 14（拖动求解 10 + 补充
  4），逐条去向见工单 05 Comments。
- Windows 目标（x86_64-pc-windows-gnu）：cargo check
  --all-targets、cargo clippy --all-targets -- -D warnings、
  cargo fmt --all -- --check 三项全过。scripts/check.sh 本体未
  运行（Windows 专用脚本，本 VM 跑不了，不冒充）。
- 边界检查（safe_code_edits_and_commits）：最终 diff 专项复读无
  劈裂/孤儿文档注释、无重复属性；GLOSSARY「窗口吸附」Avoid 词
  （磁吸 / 贴边停靠）grep 全 diff 零命中，功能命名一律「窗口
  吸附」。
