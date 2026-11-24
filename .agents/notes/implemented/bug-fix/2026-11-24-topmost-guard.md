# Agent Note: 置顶守擂——层级命令失焦重申与 TOPMOST 自检

Status: implemented

## Problem

用户实机反馈（2026-11-24 截图）：相框设「置于顶层」后过一段时间，被左侧窗口盖住。查实根因不在某个具体的摘标记动作，而在结构：层级命令全仓库只有一处发送点（frame_ui 菜单点击时发一次 `WindowLevel`），之后永不重申——置顶带内被后激活的同为置顶窗口排前、或置顶标记被外部摘掉，任何一种扰动都不可恢复，相框只能一直被压在下面。

## Decision

- 新增库层纯判定 `level_guard::LevelGuard`（仿 `next_frame_delay` / `global_sync` 的纯状态机先例）：宿主每帧以（当前层级，焦点，时刻）轮询。非 Normal 层级在失焦边沿（focused 由 true 变 false，带内排序变化的时刻）触发重申；Normal 永不重申、也不挂起。
- 节流下限 1000ms：间隔内再失焦只挂起不发，间隔一到在后续失焦帧补发一次（不丢弃）；挂起期间重新获得焦点或切回 Normal，挂起作废。判定不产生任何唤醒——只挂靠既有调度已唤醒的帧（焦点事件、轮播滴答等自然重绘），静止零重绘口径不变。
- 重申只走既有 viewport 命令 `WindowLevel`：不新增 SetWindowPos 直调、不激活窗口、不抢焦点；与吸附子类化（WM_MOVING）及几何机互不干涉。
- 自检日志：重申前宿主以 Win32 读本窗扩展样式的 WS_EX_TOPMOST（口径与 window-validation 的 window_long 观测一致），经 `note_reassert` 分类——标记缺失为 MarkMissing、标记在为 BandReorder；仅分类与上次记录不同才经 `level_guard_record` 向 moe_frame.log 写一行 `[level-guard]`（与启动标记同文件同格式、独立前缀）。读不到（句柄未就绪/非 Windows 观测）为 Unknown，不记、不影响重申。

## Alternatives considered

- 定时器周期性无条件重申：简单，但把静止零重绘顶成常开唤醒，与全局成本口径冲突，不采用。
- 只在托盘召回/切图等既有动作点顺带重申：覆盖不了「点了一下别的置顶窗口」这个主场景（扰动时刻与这些动作无关），不采用。
- 宿主直接 SetWindowPos(HWND_TOPMOST) 守擂：绕开 eframe 的层级记账会造成双口径（菜单状态与实际层级可能不一致），不采用；重申走 viewport 命令与菜单发送点同路。

## Consequences

- 置顶/置底在失焦扰动后至多等一个节流间隔（1s）即自愈；焦点反复横跳时每秒至多一条重申命令，不成风暴。
- 日志能区分两种扰动来源，为后续若再现「被盖住」留取证口：MarkMissing 说明标记确被外部摘掉，可继续追摘标记方；BandReorder 说明只是带内排序，守擂已足够。
- 守擂状态是会话级的（与 window_level 同口径，不落盘）；重启后层级回到 Normal，符合既有层级语义。

## Testing

- 纯判定由 `src/level_guard.rs` 内单测钉住（8 个）：顶层/底层失焦重申、Normal 零重申、无边沿不触发、节流挂起与到期补发、回焦/切 Normal 取消挂起、日志分类与去重。
- 宿主接线由 `ui_regressions` 的 FullOutput 回归钉住：菜单设顶层后，焦点经 RawInput 注入由聚焦变失焦，输出含重申的 WindowLevel 命令；节流与补发、底层同理、Normal 零命令同测覆盖。宿主测试在本 VM 只能以 Windows 目标编译校验（check/clippy 全绿），实际运行在 CI（windows-latest）；库层与全量刮擦副本测试 432 个全绿。
- 实机验收见 [real-machine-check-v2-frame.md](../../../../real-machine-check-v2-frame.md) 新增「置顶守擂」一节（工单 01 置 needs-info 待用户跑）。
