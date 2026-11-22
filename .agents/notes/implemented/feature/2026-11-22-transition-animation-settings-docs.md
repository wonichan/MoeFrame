# Agent Note: 切换动画设置窗接入、文档更新与实机清单（专题收口）

Status: implemented

## Problem

[切换动画专题工单 04](../../../../.scratch/transition-animation/issues/04-settings-docs-checklist.md) 是专题收口票：01 已把档位做成核心为准（`Intent::AnimationSelected` / `Effect::SaveAnimation` / `core.animation()`），02/03 已把五种动画画出来，但用户还摸不到——设置窗里没有这个选项，GLOSSARY 与需求文档还停在「滑动/淡入淡出/P1 只保存」的旧口径，01–03 的实机未验证项也散在三份笔记里没有一份能一次跑完的清单。另外设置窗现状没有「播放组」概念：播放模式与切换间隔原本被外观组隔开，与 spec「与播放模式、间隔秒数同组」对不齐。

## Decision

- 会话层接入（settings_session，循 mode/appearance 先例同构）：`EffectiveValues` 加 `animation`（宿主快照自 `core.animation()`）、`Picks` 加 `animation`、`SessionAction` 加 `ApplyAnimation(Animation)`；`animation_now` 在 open/begin_frame 播种，end_frame 在模式之后、外观之前同值守卫并内部推进，`SessionAction` 的顺序文档注释同步更新（模式 → 切换动画 → 外观 → 画框参数 → 间隔预设…）。宿主执行臂只有一行意图转发：`core.handle(Intent::AnimationSelected(..))` 后 dispatch，落盘走 01 的 `SaveAnimation` 效果臂，不新开落盘通道；「动画中改档下一条生效」由核心只在起转场时读档位天然保证，会话/宿主不另做处理。
- 设置窗布局（最终决定）：播放三项自此相邻成组——「播放模式 → 切换动画 → 切换间隔」。做法是把原在外观组之后的切换间隔块整体上移到播放模式之后，外观组退到播放组之后；切换动画 7 档两行排布（无/卷页/百叶帘/墨晕 + 抽卡/像素故障/随机）。窗口高度按既有加组口径 830→910（`SETTINGS_SIZE`，注释留痕），底部「开机自启」不被截断列入实机清单目检。
- 菜单不加项：相框右键菜单与托盘菜单一行未动；仓库无菜单内容的自动化断言先例（探路结论），按工单「回归断言或清单项」二选一取清单项——实机清单工单 04 节有两条逐项核对（菜单项逐个点名，含窗口层级在内）。
- GLOSSARY：「动画类型」更名为「切换动画」，定义与 _Avoid_ 按 spec 第 9 节改写（无/卷页/百叶帘/墨晕/抽卡/像素故障/随机；Avoid：转场、过渡动画）。
- 需求文档（仓库外 `~/workspace/moe-frame/REQUIREMENTS.md`，不在 git 管理下，循 V2 工单 05 先例只做内容对齐）：版本升 v0.3 并注明取代 2026-11-22 即时切换对齐；FR-404 改写为转场不变量口径、FR-405 改写为七档与迁移、新增 FR-409（方向与随机口径）；§1.2 特性行、配置示例 `animation = "curl"`、§8.2 实现要点、§10 验收行、FR-504/601 的「动画类型」措辞同步更新；表后加近似注记段（卷页 clip 近似、墨晕灰纱近似、百叶帘纵向展开近似——按已发布/已批准行为写，不掩盖）。
- README 同步：删「切图瞬时完成，没有滑动转场」，改述五种动画与随机/无；「尚未生效的配置键」名单去掉 `playback.animation` 并补迁移说明。`docs/adr/0003` 是历史决策记录不改写，其「animation 不得读成行为」已被本专题在事实上取代，靠 spec 与本笔记留痕。
- 实机清单：仓库根目录新增 `real-machine-check-transition-animation.md`（沿用 V2 清单体例，全部未勾）：工单 04 节（7 档/落盘重启/菜单无新增项/旧配置迁移/窗高不截断）、工单 01 节（随机连切 10 张、连点、慢图、隐藏直落、暂停不续计时、改档下一条、首图直落）、工单 02 节（卷页/抽卡手动+自动、转正时序、**卷页近似单独目检**）、工单 03 节（三动画手动+自动、**墨晕灰纱近似单独目检**、随机混排）、几何与外观节（宽↔高互切尺寸先变、三外观两处绘制点）、资源复测节（稳态 ≤80MB、静止零重绘）。01–03 三票各补一条 Comments 回填指向本清单对应节（只动评论）。

## Alternatives considered

- **动画块就近插在播放模式后、间隔不挪** — diff 更小；但播放三项仍被外观隔开，spec 的「同组」只满足一半。挪间隔块只是渲染代码搬家（会话应用顺序与渲染顺序无关），成本低，取整体成组。
- **给菜单内容写宿主自动化断言** — 仓库无此先例，egui 菜单内容断言要新起一套接缝；工单允许清单项二选一，取清单逐项核对。
- **需求文档搬进仓库** — V2 工单 05 已议并否（文档与视觉基准同目录、工单引用按仓外路径），本票只做内容对齐，不重议。

## Testing

- TDD：接缝先行——tests/settings_session.rs 先加两条用例（动画点选同值守卫、应用顺序 模式→动画→外观→间隔），红相位为 6 处编译错误（字段/变体不存在）；会话层实现后转绿。纯 UI 渲染（radio 排布、执行臂）以 Windows 目标编译 + 回归口径为准（工单口径）。
- 全量：Linux 测试副本（refresh-moeframe-test-transition-04.sh，与 01–03 副本同口径）lib + 集成测试 **314 passed / 0 failed**（基线 312 + 本票新增 2；settings_session 单目标 28/28）。副本内本票目标 clippy `-D warnings` 干净。
- Windows 目标（x86_64-pc-windows-gnu，真实 worktree）：`cargo check --all-targets` 通过、`cargo clippy --all-targets -D warnings` 干净、`cargo fmt --check` 通过（覆盖宿主 settings_ui/moe-frame.rs 改动编译）。
- 环境注记：`scripts/check.sh` 在本 VM 的 Linux 上原样仍跑不了（产品 bin Windows-only、winit 无 Linux 平台），与基线一致，不冒充全绿；以上三路证据与 01–03 同口径互补。
- 未验证：设置窗真实渲染与全部实机目检项——本票置 needs-info，待用户跑完 `real-machine-check-transition-animation.md` 回执后，本票与 01–03 一并转 resolved。

## Consequences

- 专题四票至此全部落地：用户路径完整（设置窗选档 → 核心定死种类/方向/时长 → 宿主五种渲染 → 完成回执提交）。后续新增动画档位时：config 枚举 + 核心 ConcreteAnimation + 会话/设置窗 radio 三处同构扩展，顺序注释与本笔记的布局决定继续有效。
- 设置窗高度 910 已接近小屏上限；再加新组时先考虑分组折叠或分页，不要无声继续加高。
