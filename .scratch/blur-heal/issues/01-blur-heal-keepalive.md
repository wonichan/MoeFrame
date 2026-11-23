# 01: 治愈保活——失焦切换后模糊不自愈

**What to build:** 帧尾调度在当前图口径过期且目标刚变化后保活出帧
（50ms，2s 截止），让治愈刷新走完停稳计时并及时收回送；调度裁决
抽纯函数并加回归测试。

**Blocked by:** None

**Status:** resolved

## Acceptance criteria

- [x] `next_frame_delay` 纯函数覆盖四分支优先级（隐藏心跳 / 加载轮询 /
      治愈保活 / 轮播截止）并有回归测试
- [x] `refresh_display_data` 在新过期目标出现时设保活截止，同一目标
      只设一次，口径恢复后清除记录
- [x] 全量测试与 Windows 目标 clippy（-D warnings）通过
- [x] 真机复测：相框持续失焦，自动由竖图切到宽图后，不需重新聚焦即恢复清晰。

## Context

- 诊断全过程见同目录 spec.md。
- 本票以 PR 形式提交（用户 2026-11-22 指定），不直接合入 main。

## Comments

实现（2026-11-22）：诊断与修复见 spec.md 与笔记
`.agents/notes/implemented/bug-fix/2026-11-22-blur-heal-keepalive.md`。
Windows 目标 clippy -D warnings 干净、宿主测试编译通过、库全量
231/231（含临时模拟 1 条）。真机复测未做，故 needs-info。
本票以 PR 形式提交（分支 fix/blur-heal-keepalive），不直接合入 main。

合并（2026-11-22）：PR #1 由用户创建并合并（15:12），2641558 已入
main；票 22 的交付构建验收即基于含本修复的构建，且用户已亲自完成
1 小时轮播 soak。本票仍 needs-info：失焦切到更宽图的定向复测未
单独记录，待用户确认后关闭。

- Windows 补测通过：用户用官方 `v0.1.3` 在 Windows 11 x64 Build 26100 目测，隔离图库以 5 秒顺序轮播、无动画，从蓝色竖图自动切至红色宽图后，不点相框也能变清晰。窗口日志同批记录了 `focused=Some(false)` 下的竖→宽变更。见[补测清单](../../../real-machine-check-closeout.md)，本票关闭。
