# Agent Note: 位置尺寸不恢复——几何落盘不再等下一次轮播

Status: implemented

## Problem

用户手工验收（票 23）报告：移动窗口后关闭再打开，位置与尺寸回到旧值；
「恢复默认位置与大小」也时好时坏。

实测（临时 APPDATA + `SetWindowPos` 外移窗口）：移动后 2 秒内 `config.toml`
仍写旧值 `x=2056 y=1096`，到第 8 秒才变成新位置 `x=300 y=180`。

根因与 [失焦切换模糊](2026-11-22-blur-heal-keepalive.md) 同一类：几何落盘靠
「每帧观察 + 停稳 `SAVE_SETTLE_SECS = 0.6` 秒」，计时走 egui 帧时间，而帧尾
调度在没有加载/保活时直接睡到下一次轮播截止。10 秒间隔下，移动后最多要等
10 秒才有帧把候选结算掉；这期间用户关窗口、关到托盘或直接退出，候选从未落盘，
`config_sync.shutdown` 交出的 `observed_geom` 也停在被钳制前的旧观察上。

## Decision

只动调度与交接，不改「停稳才写」的既有口径：

- `window_geom::GeometryMachine::settle_deadline_secs()`：有候选时返回
  `since + SAVE_SETTLE_SECS`，把「还差多久可以落盘」暴露给宿主。
- `frame_ui::next_frame_delay` 增加 `geom_settle_ms` 参数，落在治愈保活之后、
  与轮播截止取更早者（`rotation.min(settle)`）——几何结算不能被轮播饿死，
  轮播也不因几何提前而被推迟。
- `observe` 在 `frame_visible == false` 且仍有候选时立刻冲刷落盘：隐藏帧不再
  更新观测量，候选没有下一位接棒，留着就等于丢失；已落盘后重复的隐藏帧不重写。

## Alternatives considered

- 每帧立刻写盘：拖拽/缩放过程中会写几十次配置，且拆掉「停稳才落盘」的
  防抖口径（该口径防的是拖动中的反复写盘），不取。
- 固定 1 秒定时重绘：空转帧，且与状态机自身的 `now_secs` 时钟不对齐，
  等于把同一件事算两遍，不取。
- 只靠 `geometry::exit_settle` 在退出时兜底：覆盖不到「关到托盘后继续轮播、
  最后被任务管理器结束」的路径，且用户看到的是「重开回到旧位置」——退出兜底
  本来就该是最后一道，不是唯一一道，不取。

## Testing

- `tests/window_geom.rs` 新增 2 项：`pending_geometry_saves_when_the_settle_window_ends`
  （候选 0.6 秒后结算、`settle_deadline_secs` 从 `Some(1.6)` 回到 `None`）、
  `hiding_flushes_the_pending_geometry`（隐藏帧冲刷且不重复写盘）。
- `frame_ui` 新增 `geometry_settle_wakes_earlier_than_the_slide_deadline`，
  钉住「取更早者」两个方向。
- `scripts/check.sh` 全绿；实机复测：同一探针在修复后「移动后 1 秒内
  `config.toml` 已更新」，修复前同条件 2 秒内不更新。

## Consequences

- 移动/缩放后最多多出 1 帧（结算时刻那一帧），常驻占用不变。
- `next_frame_delay` 的优先级现在有四层：隐藏心跳 → 加载轮询 → 治愈保活 →
  几何结算与轮播截止取更早者。任何新增的「等停稳再做事」逻辑都必须同样给
  帧尾调度一个截止，否则会再次变成「等下一次轮播才生效」。
- 副屏误钳等既有已知限制不变。