# 04: 设置窗接入、文档更新与实机清单

**What to build:** 设置窗播放组新增「切换动画」7 档选择（中文标签），走既有设置会话/落盘路径即时生效；相框菜单与托盘菜单不加此项。同步更新 GLOSSARY 词条与需求文档口径，并产出实机清单供用户逐项验收 5 种动画。

**Blocked by:** [01: 配置转正与核心转场状态机](01-config-and-core-transition.md), [02: 宿主渲染·有方向组](02-host-directional-curl-card.md), [03: 宿主渲染·无方向组](03-host-ambient-blinds-ink-pixel.md)

**Status:** resolved

## Acceptance criteria

- [x] 设置窗播放组出现「切换动画」：无/卷页/百叶帘/墨晕/抽卡/像素故障/随机，显示当前配置值；切换即时生效并落盘，重启后保留；动画进行中改档，当前条播完、下一条起用新档。
- [x] 相框右键菜单、托盘菜单无新增项（菜单瘦身口径不变），有回归断言或清单项钉住。
- [x] GLOSSARY：「动画类型」词条更名为「切换动画」，定义与禁用词按 spec 第 9 节改写。
- [x] 需求文档中动画相关旧口径（滑动/淡入淡出/P1 只保存）更新为本专题口径，并注明取代 2026-11-22 即时切换对齐。
- [x] 实机清单（仓库根目录 `real-machine-check-transition-animation.md`）覆盖：5 种动画各手动 + 自动各一轮；随机模式连切 10 张无连续重复；宽图↔高图互切（尺寸先变、不拉伸）；动画中连点只执行最后意图；慢图不闪白；隐藏状态切图直落；暂停中切图播完不续计时；改档中途不换动画；稳态工作集与静止零重绘复测。
- [x] 全量测试绿；fmt/clippy/test 以 Windows 目标与 Linux 测试副本代偿验证（check.sh 本体因 VM 无 Linux GUI 条件未直跑，详见实施笔记）；实施笔记落盘；本票状态置 needs-info 待用户实机回执后转 resolved，并同步回填 01–03 的实机项。

## Context

专题 spec：[切换动画](../spec.md) 第 1/9/10 节。设置接入遵循既有 settings_session / config_sync 路径，不新开落盘通道。

## Comments

- 2026-11-22 实现完成，状态置 needs-info：设置窗播放组落地——播放三项相邻成组（播放模式 → 切换动画 → 切换间隔，间隔块由外观组后上移，外观组退后），切换动画 7 档两行 radio（无/卷页/百叶帘/墨晕 + 抽卡/像素故障/随机），显示核心当前档；会话层同构接入（EffectiveValues/Picks 加 animation、SessionAction::ApplyAnimation、animation_now 同值守卫，应用顺序为模式 → 切换动画 → 外观，顺序注释同步）；宿主执行臂只转发 Intent::AnimationSelected，落盘走 01 的 SaveAnimation 效果臂。SETTINGS_SIZE 高 830→910（加组既有口径）。相框/托盘菜单一行未动，无新增项由实机清单两条逐项核对钉住（仓库无菜单内容自动化断言先例，按工单二选一取清单项）。GLOSSARY「动画类型」更名「切换动画」（spec 第 9 节）；需求文档（仓库外 ~/workspace/moe-frame/REQUIREMENTS.md）升 v0.3：FR-404/405 改写、新增 FR-409、配置示例与 §8.2/§10/FR-504/601 同步、表后加三处近似注记，并注明取代 2026-11-22 即时切换对齐；README 同步删旧口径。实机清单 real-machine-check-transition-animation.md 已就绪（04 设置节 + 01 核心节 + 02/03 分节含卷页/墨晕近似单独目检 + 几何外观节 + 资源复测节），01–03 三票已各补 Comments 回填指向对应节。验证：TDD 会话层先红（6 处编译错误）后绿，新增 2 用例（同值守卫、应用顺序）；Linux 副本全量 314/314 绿（基线 312+2），Windows 目标 check/clippy 干净，fmt 通过；scripts/check.sh 在本 VM 的 Linux 原样跑不了（产品 bin Windows-only，与基线一致），以三路证据互补、不冒充全绿。实施笔记：.agents/notes/implemented/feature/2026-11-22-transition-animation-settings-docs.md。needs-info 原因：设置窗真实渲染与全部实机目检待用户跑完清单回执，届时本票与 01–03 一并转 resolved。
- 2026-11-23 用户实机验收通过（`real-machine-check-transition-animation.md` 全部条目逐项验过、无大问题，含两个近似目检项），状态置 resolved；随 PR #4 合入 main，并以 v0.1.3 发布。
