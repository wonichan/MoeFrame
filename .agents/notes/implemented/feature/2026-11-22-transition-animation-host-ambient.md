# Agent Note: 切换动画无方向组宿主渲染（百叶帘 / 墨晕 / 像素故障）

Status: implemented

## Problem

[切换动画专题工单 03](../../../../.scratch/transition-animation/issues/03-host-ambient-blinds-ink-pixel.md) 要在工单 02 的宿主转场骨架上补齐三种无方向动画：百叶帘（8 条竖帘错峰翻下）、墨晕（圆形扩散 + 旧图去色）、像素故障（旧图抖动 + 新图块拼齐）。难点不在骨架（02 已备齐并留占位臂），而在三件事：错峰/扇形/块序这类计算要可测（宿主绘制本身无自动化先例）；墨晕撞 egui 0.37 硬限（无圆形裁剪、tint 乘法做不出真去色）；像素块序既要每帧可复现（逐帧重洗会闪烁）又要每条动画不同。

## Decision

- 纯计算入库存档接缝：新增库模块 `src/transition_ambient.rs`（循 frame_layout 先例——宿主绘制的纯几何归库、tests/ 直接引用）：墙钟进度 `transition_progress`、旧图 fit `fit_rect`（LayoutRect 口径，与 02 宿主侧 egui 口径的 fit_rect 同数学、不同接缝）、百叶帘 `blinds_strips`（条带 dest/UV/shade 一次算齐）、墨晕 `ink_fan` + `ink_ring_point`（扇形参数与环点/UV）、像素 `pixel_order` / `pixel_visible_count` / `pixel_block` / `pixel_jitter`。宿主落笔只消费结论。
- TDD 三个垂直切片（条带+进度+fit → 墨晕扇形 → 像素块序/抖动），每片先红（未解析导入）后绿，共 11 个用例（tests/transition_ambient.rs）。
- 百叶帘：单条揭示跨度 0.65、自左至右错峰总跨度 0.35，最后一条在进度 1 恰好播满；每条从舞台顶部向下揭示（dest 高与 UV 的 v 同步截断——「上沿翻下」按探路近似：纵向展开而非真 3D 翻转），shade 0.55→1.0 模拟翻转明暗，满格全回 1 无残余。
- 墨晕：扇形 Mesh 单纹理——圆心 + 主环（64 段，不透明）+ 羽化外环（1.12 倍半径、全透明）一次拼成（ambient_paint::ink_fan_shape）；半径 ease-out 增长、满格 = 半对角线盖全四角。旧图去色按用户 2026-11-22 批准的近似：tint 压暗到 0.65 + 灰纱 alpha 0→110 盖旧图 fit 区，不做真去色、不预生成去色纹理（守双纹理上限）。
- 像素故障：块序 = xorshift64* 驱动的 Fisher–Yates 洗牌（96 块），种子在转场开始时派生并存入 `HostTransition.seed`（02 笔记预留的加法点：开始时刻 × 黄金常数 ^ 种类盐）；同条内逐帧同序、每条不同。抖动只活在开头 120ms，按 33ms 帧格从种子派生偏移（±4px），窗口外恒零；拼齐块数随进度线性增长。
- 接入面极小：`paint_transition` 的占位臂换成三臂转调 ambient_paint 的 `paint_blinds / paint_ink / paint_pixel`（新宿主模块 `src/bin/moe-frame/ambient_paint.rs`，自包含落笔 + 舞台裁剪）；`paint_transition` 签名补 `elapsed_ms`（像素抖动用）；骨架（begin/advance/paint_stage、帧尾唤醒、slide_active、转正搬家）一行未动。三种函数签名均无方向参数——无方向不是约定而是类型事实。
- 开发中途的合并处理：本票开工时 02 尚未合入，曾先自建平行最小骨架（ambient 专用字段/结算）让三种动画独立跑通；02 合入集成分支（86ddeeb）后主动撤除平行骨架、改接 02 骨架，平行实现未留残迹（最终 diff 只剩分发臂、seed 字段与新模块）。

## Alternatives considered

- **纯计算放宿主内联 mod（循 02 的 transition.rs 先例）** — 宿主测试在本 VM 只能编译不能执行，TDD 红绿环跑不起来；入库后 Linux 副本可真跑，取入库。
- **墨晕环带 rect 近似圆** — 锯齿明显，探路已否；扇形 Mesh 是 epaint 标准用法，取之。
- **像素块序用宿主现取的系统随机每帧洗** — 每帧重洗会让已亮块闪烁消失；开始时洗一次存种子派生，取之。
- **百叶帘条做真 rotateX 翻转（梯形 Mesh）** — 02 的抽卡已有梯形先例，但百叶帘 8 条逐条 Mesh 成本与复杂度远高于纵向展开 + 明暗的观感收益，探路定案即纵向展开近似，取之。

## Testing

- TDD：三切片各先红（E0432 未解析导入）后绿；tests/transition_ambient.rs 共 11 用例全过（进度钳制、fit 三态、百叶帘拼满/错峰序/UV 同步、墨晕半径覆盖四角/灰纱与压暗单调、环点落圆周、块序全排列且种子确定、块 dest/UV 切分、抖动窗口与归零）。
- 全量：Linux 测试副本（refresh-moeframe-test-transition-03.sh，与 01/02 副本同口径）lib + 集成测试 **312 passed / 0 failed**（含本票新增 11；副本 clippy 单独验本票目标干净，副本全量 clippy 仍卡工单 01 已记录的既有 unix-only 测试 lint `cloned_ref_to_slice_refs`，本票未触其文件）。
- Windows 目标（x86_64-pc-windows-gnu，真实 worktree）：`cargo check --all-targets` 通过、`cargo clippy --all-targets -- -D warnings` 干净、`cargo fmt --check` 通过（覆盖宿主 bin 全部改动与 02 内联测试的编译）。
- 环境注记：本 VM 的 Linux 上 `scripts/check.sh` 原样跑不了（产品 bin Windows-only、winit 无 Linux 平台），与基线相同，不冒充全绿。
- 未验证：三种动画的真实窗口观感、墨晕灰纱近似的目检、与卷页/抽卡在随机档下混排的实机表现——本票置 needs-info，目检项并入工单 04 实机清单（墨晕近似单独一条）。

## Consequences

- 随机档混排时五种动画共用同一骨架与提交点，03 的三臂与 02 的两臂在分发处平级；后续若加新动画，纯计算仍先入 transition_ambient 接缝、落笔再进分发。
- `HostTransition.seed` 目前只服务像素块序；墨晕/百叶帘不读它（两者无随机成分，spec 定案）。
- 库 transition_ambient::fit_rect 与宿主 transition::fit_rect 是同一数学的两个口径接缝（LayoutRect vs egui::Rect），后续若再有第三处 fit 需求，应先考虑收敛而非再复制。
