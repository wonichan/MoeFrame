# Agent Note: 切换动画专题双轴评审修复（工单 01–04 收口后）

Status: implemented

## Problem

切换动画专题（工单 01–04 全部合入集成分支 `feature/transition-animation`，tip `3bcd16d`）的双轴 code-review 提出四项发现：Standards 硬伤两项（工单 01/04 的验收勾选写「scripts/check.sh 通过」，与各自 Comments 和实施笔记自相矛盾——check.sh 在本 VM 原样跑不了、靠副本 + Windows 目标代偿；README「目录」段未登记新库模块 `src/transition_ambient.rs`）、Standards 判断项一项（`fit_rect`、`progress`、`ease_out_cubic` 在宿主 `transition.rs` 与库 `transition_ambient.rs` 各有一份同数学实现、测试也重复——工单 03 实施笔记的 Consequences 已预告此收敛）、Spec 轻微口径差一项（spec 第 3 节随机「与上一次实际播放不同」与实现「只在转场完成时记 last、取消不记」的口径差，只在实施笔记有记、工单未明示）。

## Decision

- 工单 01/04 的该条验收勾选统一改述为：「全量测试绿；fmt/clippy/test 以 Windows 目标与 Linux 测试副本代偿验证（check.sh 本体因 VM 无 Linux GUI 条件未直跑，详见实施笔记）」，与两票 Comments 的既有如实口径对齐，不再冒充 check.sh 直跑全绿。
- README「目录」段在 display_data 行后补登记 `src/transition_ambient.rs` 一行，格式与同段一致。
- 重复实现收敛为一份、以库为准（AGENTS.md 代码地图：可复用逻辑归 `src/`）：库 `ease_out_cubic` 由私有改公开，成为全仓唯一曲线实现；宿主 `transition.rs` 删除自有 `ease_out_cubic`、`progress` 与 `fit_rect` 的数学实现——`progress` 只留 (started, now) 时间戳适配转调库 `transition_progress`，`fit_rect` 只做 egui↔LayoutRect 类型转换 + 舞台退化守卫后转调库 `fit_rect`（纹理尺寸非法时宽高比 ≤0/非有限由库侧判退舞台，与原宿主守卫同效），卷页 `curl_frame` 改经库取曲线。宿主侧重复的 progress/fit 两条内联测试删除，断言由库 `tests/transition_ambient.rs` 钉住，并为曲线补一条直接断言（端点/中点 0.875/单调/越界钳制）。
- Spec 口径差只动工单 01 Comments：新增一条明示——`last_animation` 只在转场完成时记，被重扫中途取消的转场（已部分播放）不记，理由是取消的转场没有完成提交、计入会让下一次随机平白排除一种动画，且与「提交点在完成时」的全专题口径一致。

## Alternatives considered

- **宿主保留薄包装函数名（progress/fit_rect）** — 调用点（HostTransition::progress、ambient_paint 三处 fit_rect）零改动，包装内只剩类型转换与时间戳相减，不含数学；比全改调用点签名动静小，取之。
- **把 ease_out_cubic 留私有、另在库加公开包装** — 等于再加一层同义函数，收敛不彻底；直接公开原函数，取之。

## Testing

- Linux 测试副本（refresh-moeframe-test-transition-fix.sh，与 01–04 副本同口径）：全量 **315 passed / 0 failed**（基线 314 + 本次新增 ease 曲线断言 1）；`transition_ambient` 与 `app_core` 目标全绿——按评审口径，既有两目标全绿即为「动画数值行为不变」的证据（时长、曲线、fit 几何均由库内同一实现产出，宿主只剩转换层）。副本 clippy（lib + transition_ambient 测试目标，`-D warnings`）干净。
- 真实 worktree：`cargo fmt --all -- --check` 通过；Windows 目标（x86_64-pc-windows-gnu）`cargo check --all-targets` 与 `cargo clippy --all-targets -- -D warnings` 均 exit 0（覆盖宿主 bin 的收敛改动与保留的卷页/抽卡内联测试编译）。
- 环境注记：本 VM 的 Linux 上 `scripts/check.sh` 原样跑不了（产品 bin Windows-only、winit 无 Linux 平台），与基线相同，不冒充全绿——这正是本次工单改述要对齐的事实。

## Consequences

- 工单 03 笔记 Consequences 里预告的「第三处 fit 需求应先收敛」已兑现：fit/进度/曲线全仓各只剩一份数学实现（库），宿主 transition.rs 与 ambient_paint.rs 共用之；后续新动画的纯计算直接进 `transition_ambient` 或同级库模块，不再在宿主落第二份。
- 宿主 `fit_rect` 对舞台退化（宽/高 ≤0）仍在转换层守卫、直接退舞台，与收敛前一致；纹理尺寸来自真实纹理恒为正，宽高比路径无行为差。
