# Agent Note: 实例发现与松手吸附（多实例吸附专题工单 02）

Status: implemented

## Problem

工单 01 给了多实例身份与落盘隔离，但进程间仍零通路：相框不知道
其他相框的存在，拖动松手后窗口停在任意位置。本票做「松手吸附」
本身：拖动松手瞬间枚举并校验出伙伴相框、按几何接触实时派生吸
附关系、按 12 逻辑 px 阈值算贴合/对齐目标并落位。组联动跟随归
工单 03，本票不做；拖动全程不干预（ADR-0004 路线 b）。

## Decision

- 新增库 module `snap`（纯逻辑可测），三块：
  - 伙伴判定 `is_partner_frame`：候选事实（标题、所属进程 exe、
    存活、自窗旗标、矩形、所在屏、工作区）全部可注入。两层过滤
    ——标题恰为 `MoeFrame`（设置窗标题不同，第一层挡住）、所属
    进程 exe 与自身一致（第二层挡住其他程序的同名窗口）；另要
    求所属进程存活（宿主经工单 01 的 `process_alive` 接缝供给）
    且不是自窗。exe 比较归一化大小写与分隔符写法。
  - 接触派生 `derive_contacts` / `snap_groups`：同屏（比较带原点
    的显示器矩形，不用只有尺寸的退化口径）、相对边间隙绝对值
    ≤ `CONTACT_TOLERANCE`（1 逻辑 px)、且法线方向投影重叠才判
    贴合；贴合边的连通分量即吸附组。关系不落盘。
  - 松手求解 `solve_snap`：阈值 `SNAP_THRESHOLD` = 12 逻辑 px。
    横纵两轴独立裁决后组合。选择规则定死：单轴内按被拖窗的边
    分组（左/右、顶/底——作用于不同边的候选互不压制：一边已贴
    合不挡另一边被吸过去，此规则由测试失败逼出，见 Testing）；
    组内贴合优先于对齐、同类间隙小者优先、同距按生成顺序（伙
    伴枚举顺序，工作区候选最后生成）取先者；两组赢家只在真能
    移动窗口者里按间隙小、同距生成在先决胜。贴合要求另一轴投
    影重叠，对齐不要求。工作区边缘是贴合候选，目标即边缘本身、
    间隙归零——与工单 06 钳制的 6px 保留距离互不引用、互不改
    口径（注释与 spec 引用处均已写明区分）。
- 宿主薄枚举层 `src/bin/moe-frame/snap_enum.rs`：Win32 调用集中
  一处（EnumWindows / IsWindowVisible / GetWindowTextW /
  GetWindowThreadProcessId / OpenProcess +
  QueryFullProcessImageNameW / GetWindowRect / MonitorFromWindow +
  GetMonitorInfoW / GetDpiForWindow），只取事实喂库判定，本层无
  判定逻辑。坐标换算口径：Win32 矩形是物理像素，每扇窗口按它
  自己的 DPI（GetDpiForWindow ÷ 96）换算成逻辑像素（各屏 DPI 可
  不同，不拿被拖窗比例换算伙伴矩形），取不到时退到被拖窗的
  pixels_per_point。
- 挂接时机：宿主 `update_window_geometry` 在原生移动手势结束
  （`native_gesture == Move` 且主键已抬起）那一帧先清手势旗标，
  使本帧观察的 `pointer_down` 如实为假，几何机据此走既有松手帧；
  同帧组装 `SnapRequest` 随 `GeomObservation.snap` 递交（新接缝，
  `GeomObservation` 因此不再是 Copy）。几何机只在松手帧消费请
  求：以校正后的有效位置与外尺寸为被拖窗矩形求解，目标经既有
  `GeomCommand::SetOuterPosition` 落位并成为本帧落盘观察位置
  （防抖落盘走工单 01 实例段，无新通道）。按下帧递交的请求从
  不被读，静止帧递交的请求被忽略——拖动中不发吸附命令、静止
  零打扰；枚举只在松手瞬间发生，无常驻轮询。缩放手势不吸附。
- 开关落地：全局段 `general.window_snap`（默认开，字段修复同口
  径补默认），`ConfigStore::set_window_snap` 走工单 01 的
  `update` 唯一写入通道，不开第二条覆写路径；`ConfigSync` 启动
  包 `HostInit.window_snap` 携带初值、`save_window_snap` 写回；
  设置窗在窗尾加「窗口吸附」勾选（播放/显示相关控件之后、自启
  之下），经会话机 `Picks.window_snap` → `ApplyWindowSnap`（同值
  守卫、应用顺序在自启之后）→ 宿主改当前值并写回。判定逻辑读
  宿主持有的当前值：本进程启动值与自身切换即时生效；关闭时宿
  主不枚举、不组装请求，几何机无从计算。跨实例实时同步归工单
  04，本票不预支。

## Alternatives considered

- 吸附求解放宿主、几何机直接收目标位置：判定与求解会藏进不可
  测的宿主层，且落位时机与几何机的松手重校正顺序要另行协调
  ——不采纳，求解沉库、几何机只多一个观察字段。
- 单轴不分组、全部候选一起比：实测被否——窗口左缘已贴工作区
  （间隙 0 的贴合候选）会压制右缘真正的伙伴贴合候选，松手纹丝
  不动。分组按被拖窗的边是最小且可解释的修法。
- 枚举时机放在几何机松手帧由宿主回调供给：几何机是库、回调反
  转依赖；现方案宿主预判手势结束帧组装、几何机按「只在松手帧
  消费」兜底，职责不倒置。

## Testing

- TDD：先写测试观察失败（snap 模块/set_window_snap/snap 字段/
  window_snap 字段缺失共 9 处编译错误），再实现转绿；求解器中
  途因对边候选压制失败 5 项，分组规则修后转绿。
- 新增 31 项：tests/snap.rs 24 项（伙伴判定五态含同标题异进程
  与设置窗混入、exe 写法归一；接触派生单边/链式三窗/L 形/无接
  触/超容差/跨屏/投影错开；求解四边贴合、恰阈值、超阈值、对齐
  不要求接触、贴合优先于对齐、最近优先、同距枚举序决胜、双轴
  组合、工作区贴边归零、跨屏伙伴忽略、已贴合不发命令）；
  tests/window_geom.rs 3 项（松手帧经 SetOuterPosition 落位、按
  下帧带请求不落位、无松手的静止帧带请求被忽略、无请求松手保
  持名义位置）；tests/config.rs 2 项（默认开与只改一键的往返、
  非法值修复回默认开）；tests/config_sync.rs 1 项（启动包携带与
  save_window_snap 写回往返）；tests/settings_session.rs 1 项
  （开关点选一次动作、同值守卫）。
- 全量（Linux 测试副本，沿用既有替代口径）：381 passed / 0
  failed（基线 350 + 本票新增 31）。Windows 目标：`cargo check
  --all-targets`、`cargo clippy --all-targets -- -D warnings`、
  `cargo fmt --all -- --check` 三项均通过。`scripts/check.sh` 本
  体未运行（Windows 专用，本 VM 跑不了），未冒充。
- 边界检查（safe_code_edits_and_commits）：最终 diff 专项复读
  ——无劈裂/孤儿文档注释、无重复属性；GLOSSARY Avoid 词（磁
  吸、贴边停靠、窗口组、编组、窗口号、进程号、设置窗口）对
  diff 与新文件 grep 零命中。

## Consequences

- 双开时拖一扇靠近另一扇或工作区边缘，松手即贴合/对齐；单实
  例时枚举不到伙伴，行为与旧口径一致（工作区贴边对单实例同样
  生效，是 spec 第 3 节的既定形态）。
- 遗留风险：(1) Win32 宿主枚举层未实机验证——EnumWindows 回调、
  QueryFullProcessImageNameW 取路径、GetDpiForWindow 换算均只
  过了 Windows 目标编译与 clippy，真实多开下的枚举结果、模态
  拖动结束帧的组装时机（先清手势旗标一帧）与落位观感都要等工
  单 04 实机清单验。(2) 伙伴的工作区/显示器矩形按伙伴自身 DPI
  换算，若两屏 DPI 不同且窗口跨屏摆放，逻辑坐标系以各自屏为
  准的换算可能有小偏差；同屏判定先行挡住跨屏求解，影响面仅限
  同屏内的换算精度。(3) 松手帧若恰与切图比例校正同帧，吸附命
  令排在校正的位置命令之后发出（同帧后发者生效），吸附以校正
  后的有效位置为求解输入，口径自洽但未实机验过同帧叠加。
  (4) 设置窗标题当前为「萌框设置」（spec 第 9 节定案的带编号
  标题「萌框设置 · 相框 N」尚未落地），本票判定用例按带编号
  形态验证混入排除；无论哪种形态，判定靠的是「标题恰为
  MoeFrame」的第一层过滤，都不受影响。
- 实机四项（四边贴合、顶/底对齐、工作区贴边、关开关不吸附）
  未验，汇总进工单 04 实机清单。
