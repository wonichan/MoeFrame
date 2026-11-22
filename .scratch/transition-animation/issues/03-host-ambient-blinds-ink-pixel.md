# 03: 宿主渲染·无方向组（百叶帘 + 墨晕 + 像素故障）

**What to build:** 宿主按核心转场事件渲染三种无方向动画：百叶帘（blinds，650ms，8 条竖帘上沿翻下、自左至右错峰）、墨晕（ink，750ms，新图以画面中心为圆心圆形扩散、旧图同步去色）、像素故障（pixel，550ms，开头约 120ms 旧图错位抖动、新图 12×8 块随机拼齐）。上一张/下一张播放完全相同，不做镜像假方向。

**Blocked by:** [01: 配置转正与核心转场状态机](01-config-and-core-transition.md)

**Status:** needs-info

## Acceptance criteria

- [x] 百叶帘：恰 8 条竖帘，自左至右错峰、上沿翻下；帘条为同一张新图纹理的子区域（UV）绘制，禁止生成多张纹理。
- [x] 墨晕：扩散圆心在画面中心，以扇形 Mesh（单纹理）实现圆形扩散；旧图去色按 spec 批准的近似——随进度盖渐强灰纱 + 压暗（egui tint 做不出真去色，用户 2026-11-22 批准）；结束时无残余裁剪/滤镜状态；实机清单单独目检近似效果。
- [x] 像素故障：抖动只发生在开头约 120ms 且只作用于旧图；新图按 12×8 网格块随机顺序拼齐，块为同一纹理的子区域绘制；随机顺序每条动画重新洗，不要求可复现。
- [x] 三种动画对上一张/下一张完全一致（无方向分支代码）。
- [x] 几何与舞台口径同工单 02：新比例舞台、旧图居中不拉伸、只动图片区域、框与轮廓不动。
- [x] 墙钟推进、完成回执、双纹理上限、动画后回到零重绘，同工单 02 口径。
- [x] Windows 目标 check/clippy 通过；实机目检项并入工单 04 的实机清单（本票状态置 needs-info 待回执）。

## Context

专题 spec：[切换动画](../spec.md) 第 2/4/7/8 节。视觉参考：同目录 `prototype.html` 的变体 B、C、E。性能红线在 spec 第 8 节：子区域绘制是验收线，不是优化建议。

## Comments

- 2026-11-22 实现完成，状态置 needs-info：纯计算入库 `src/transition_ambient.rs`（循 frame_layout 先例，TDD 三切片先红后绿，tests/transition_ambient.rs 11 用例：条带拼满/错峰/UV 同步、扇形覆盖四角/灰纱单调、块序全排列/种子确定、抖动窗口归零）；落笔入新宿主模块 `src/bin/moe-frame/ambient_paint.rs`（百叶帘 UV 条带、墨晕 64 段扇形 Mesh + 羽化外环、像素 UV 块拼齐，全程舞台裁剪）；接入面只动 02 骨架的分发臂（占位臂换三臂、`paint_transition` 补 elapsed_ms、`HostTransition` 加 seed 字段存像素块序种子），骨架其余一行未动。墨晕去色按批准近似（压暗 0.65 + 灰纱 alpha→110）。本票开工时 02 未合入，曾自建平行最小骨架跑通，02 合入（86ddeeb）后已主动撤除平行骨架改接 02 骨架，无残迹。验证：Windows 目标 check/clippy 全过、fmt 通过；Linux 副本 lib+集成 312/312 绿。实施笔记：`.agents/notes/implemented/feature/2026-11-22-transition-animation-host-ambient.md`。needs-info 原因：实机目检（三种动画观感、墨晕灰纱近似单独一条、随机档五种混排）并入工单 04 实机清单，待回执后转 resolved。
