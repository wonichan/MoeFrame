# Agent Note: V1 终点是可验收的 release exe

Status: implemented

## Problem

[V1 交接路线](../../../../.scratch/v1-route/map.md) 把终点定成「只锁定路线、不写程序」。按这个终点，写出 `MoeFrame.exe` 在范围外。用户随后要求终点改为一个可运行的程序。

## Decision

活地图是 [V1 可运行程序](../../../../.scratch/v1-exe/map.md)。终点是通过需求文档第 10 节的 release `MoeFrame.exe`，外加简短 README。P1 不挡完成。这张图允许实现。旧图保留原终点文字，只标作废，不改写成反面。

设置窗边界已关闭，按 [ADR-0003](../../../../docs/adr/0003-settings-window-boundary.md) 实现。[边缘缩放先验证](../../../../.scratch/v1-exe/issues/06-resize-before-switch.md) 和 [全分辨率解码](../../../../.scratch/v1-exe/issues/07-full-res-decode.md) 已关闭验证路线的决定，但两项实际验证都尚未执行。先分别做独立最小验证，通过后才进入对应的正式实现；失败先排查，确认仍不能满足要求后再另开备选票，不直接更换 GUI 框架或解码器。

## Alternatives considered

- **改旧图终点，把实现捡回来** — 旧图已经把写程序划到范围外。直接改终点会把划出的工作在同一张图上复活。否掉，改画终点后另开图。
- **终点停在能弹出的骨架** — 这样最快能看到窗口。否掉，因为用户接受的完成线是第 10 节，不是 debug 骨架。

## Consequences

- **收益**：完成线与第 10 节和 release 交付物一致，后续会话不会停在「只写决定」。
- **代价与已知上限**：设置窗边界不再挡住那一块实现。无边框窗口与解码路线虽已确定，但正式实现仍须等待各自的最小验证通过；解码瞬时峰值上限与 Limits 阈值还须实测后由用户确认。P1 仍不在本图里；若要 P1 进完成线，必须再改终点。
