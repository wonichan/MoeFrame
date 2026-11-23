# Agent Note: 切换动画档位转正与核心转场状态机泛化

Status: implemented

## Problem

[切换动画专题工单 01](../../../../.scratch/transition-animation/issues/01-config-and-core-transition.md) 要把只保存不驱动的 `animation` 配置键（旧枚举 `Slide/Fade/None`）转正为 7 档切换动画，并让核心在转场开始时就把动画种类（随机档须抽成具体一种）、方向与时长全部定死交给宿主。难点有三：核心原本不知道动画设置（`CoreArgs` 无此字段）；核心没有自己的随机源（随机性归播放序列持有），「随机等概率、禁连续重复」无接缝则写不出确定性测试；隐藏时直落与「无」档直落要在核心层与首图直落汇合，不能另起一套提交路径。

## Decision

- 配置转正：`Animation` 改为 7 档 `None/Curl/Blinds/Ink/Card/Pixel/Random`，默认 Curl，模板与 `Config::default` 同步；解析迁移按 spec——`none` 保留，`slide/fade`/未知值落 Curl 并写回；新增 `ConfigStore::set_animation`（只改一键）与 `ConfigSync::save_animation`，启动翻译把档位翻进 `CoreArgs.animation`（核心为准，与播放模式同构）。宿主 dispatch 新增 `SaveAnimation` 落盘臂。
- 具体动画类型：核心新增 `ConcreteAnimation`（5 种，永不含无/随机），自带冻结时长（curl 550 / blinds 650 / ink 750 / card 600 / pixel 550ms）。`Effect::StartTransition` 扩 `animation + duration_ms` 两字段，宿主只按事件渲染、不自选种类与时长；`Transition` 结构同步存已抽定的种类。
- 随机源接缝：复用播放序列的 `RandomSource` trait。`FrameCore::new` 签名不变，内部委托新构造 `FrameCore::with_animation_random(args, thresholds, Box<dyn RandomSource>)`，生产注入 `SystemRandom`，测试注入脚本源。抽法：池 = 全 5 种排除 `last_animation`，`next_below(pool.len())` 等概率取一。
- 「上一次实际播放」在转场**完成**时才记（`TransitionCompleted` 里把 `transition.animation` 写入 `last_animation`）：被重扫取消的转场没播完不算播放过，不进排除池。只存内存、不落盘。
- 直落汇合：`LoadCompleted` 的导航成功分支在起转场前先问 `draw_animation()`——相框隐藏或档位为无返回 None，走既有 `commit_display` 直落（与首图同一提交形态）；随机/固定档位返回具体动画才起转场。提交点、单意图队列、Tick 挂起、暂停语义全部沿用工单 14 骨架，未动。
- 改档意图：新增 `Intent::AnimationSelected`（同值守卫 → 改档 + `Effect::SaveAnimation`，与 `PlaybackModeSelected` 同构）。档位只在起转场时读取，故转场中改档天然自下一次切换生效；核心加 `animation()` getter 供工单 04 的设置窗读当前值。

## Alternatives considered

- **随机抽法放宿主** — 核心改动最小；但「禁连续重复」的状态与抽法就散在渲染层，行为接缝测不到，违反本票验收标准 8，拒绝。
- **给 `FrameCore::new` 直接加第三个随机源参数** — 所有构造点（宿主 + 十余处测试）都要改签名；委托构造变体达到同一接缝效果且生产路径零改动，取后者。
- **`last_animation` 在转场开始时记** — 更简单；但重扫会取消在飞转场，「开始过」不等于「播放过」，完成时记更贴 spec「上一次实际播放」的措辞，取后者。

## Testing

- TDD 三个垂直切片（config 文件接缝 → config_sync 启动翻译/写回接缝 → app_core 意图/效果接缝），每片先红后绿。
- 新增/改断言：config 迁移与往返（7 值原样往返、`none` 保留、`slide/fade/spin` 落 curl 并写回盘面、`set_animation` 只改一键）；config_sync 种入 Ink → 启动包核心入参为 Ink、`save_animation` 落盘可重读；app_core 五档各自的 (种类， 时长） 断言、无档直落、隐藏直落、脚本随机源下 bound 序列 [5,4,4] 与种类序列 Curl→Blinds→Curl（禁连续重复的确定性证明）、改档落盘 + 同值不重写 + 转场中改档下一条生效、重扫替代图直落。既有 StartTransition 全等断言 5 处补新字段；既有测试 `the_slideshow_and_navigation_continue_while_hidden` 按新口径改断言（隐藏改为 LoadCompleted 直接提交，目的不变）。
- 全量：lib + 18 个集成测试目标 **297 passed / 0 failed**（Linux 测试副本，基线 289 + 本票新增 8）。`cargo fmt --check` 通过；clippy（lib + 集成测试，`-D warnings`）干净；Windows 目标（x86_64-pc-windows-gnu）`cargo check --all-targets` 与 clippy `-D warnings` 在真实 worktree 通过（覆盖宿主 bin 的 SaveAnimation 分发臂）。
- 环境注记：本 VM 的 Linux 上 `scripts/check.sh` 原样跑不了（winit 无 Linux 平台、两个产品 bin 为 Windows-only），与基线相同；副本里 check.sh 过 fmt 阶段，clippy 止步于工单 09 decode-validation 验证程序的既有 dead-code 警告（本票未触其文件）。副本另有一条基线既有的 unix-only 测试 lint（`cloned_ref_to_slice_refs`，Windows 上该测试 cfg 不存在），已单独放行核对、未改动。
- 未验证：宿主真实渲染（02/03 的范围，本票宿主仍秒回完成）；实机观感归工单 04 清单。本票因此置 needs-info。

## Consequences

- 02/03 接 `StartTransition` 时：种类与时长以事件为准，宿主不得本地重抽随机或改时长；延迟回送 `TransitionCompleted` 即自动后移全部记账（提交点机制无需再动核心）。
- 04 接设置窗时：改档走 `Intent::AnimationSelected` → `Effect::SaveAnimation` 的核心为准路线（探路笔记路线 A），当前值读 `core.animation()`；`HostInit` 不需要 animation 字段。
- 随机档的排除池只认「完成过」的转场：02/03 若引入转场被打断的新路径（除重扫外），须确认被打断的转场不回送完成事件，否则排除口径会漂移。
