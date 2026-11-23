# 05: 收编合并——main 的交叉淡化与几何工作并入本专题

**What to build:** 把 origin/main（本地 agent 直推的 `fcaea5e`「切图交叉淡化与相框几何动画」，专题 `.scratch/switch-animation-and-geometry`）合入集成分支，按用户 2026-11-23 拍板「以 PR #4 为准」收编：动画体系以本专题为准；交叉淡化收编为新档「淡化」；切图几何回到本专题 spec §7 的瞬时口径；对方的回弹修复等独立成果保留。本票是合并票，不新增用户可见功能（淡化档除外）。

**Blocked by:** 01 / 02 / 03 / 04（均已实现）

**Status:** resolved

## Acceptance criteria

- [x] 冲突 11 个文件全部按取舍规则解决，合并以 merge commit 落在 `feature/transition-animation`，PR #4 变为 MERGEABLE
- [x] 淡化档落地：`Animation::Fade`（存盘键 `fade`）+ `ConcreteAnimation::Fade`，时长 240ms，宿主转场骨架加 fade 绘制臂（旧图垫底居中不拉伸、新图 alpha 三次缓出叠加），无方向
- [x] 随机池 5 → 6（含 Fade），等概率、禁连续重复不变；设置窗 radio 共 8 档
- [x] 配置迁移补一条：存量 `fade` → Fade 档（对方两档时代的真实行为值）；`slide` → 卷页、`none` → 无等既有规则不变；config 测试写明来历
- [x] 对方重复实现撤除：app.rs 的 `ImageTransition` 淡化机器与 fitted 绘制、设置窗两档入口与 `ApplyAnimation` 重复路径、几何平滑动画（`GeomAnimation`、尺寸/位置插值、名义中心锚定、切图位置钳制、`TransitionInterrupted`、切图延迟落位 pending_aspect）
- [x] 回弹修复无条件保留：宿主 `native_gesture_active` 旗标与 input.rs 原生手势记录、几何机指针名义更新与松手重校正，其测试保持通过
- [x] 名义高度与超宽临时缩高不落盘保留（瞬时形式：切图时按工作区适配显示尺寸，落盘仍按名义尺寸）；落盘位置改为以实际观察位置为准、名义中心只兜底（中心锚定撤除后的必要修正）
- [x] 文档留痕：对方专题 spec/工单/实施笔记与 ADR-0003 加收编注记（不删除）；本专题 spec 更新 8 档/池 6/fade 迁移/合并注记；GLOSSARY 与 README 同步；实机清单补工单 05 一节
- [x] 全量测试与 Windows 目标 check/clippy/fmt 通过（327 通过 / 0 失败，数字记入实施笔记；check.sh 本体在 VM 跑不了的口径与前票一致）
- [x] 实机验收并入 `real-machine-check-transition-animation.md` 的工单 05 一节，与 01–04 一并验收

## 对方 window_geom 测试的逐条处置

对方专题新增 32 个 window_geom 测试（基线口径），处置如下：

| 测试 | 处置 | 理由 |
|---|---|---|
| `transition_grows_around_the_old_center_without_moving_the_center` | 改写为 `switching_images_resizes_instantly_without_moving_the_window` | 中心锚定撤除；断言改为一帧到位、只有尺寸命令、无位置命令 |
| `transition_clamps_the_target_to_the_six_pixel_work_area_margin` | 改写并入 `an_oversized_switch_target_shrinks_to_fit_the_work_area` | 位置钳制撤除；尺寸适配工作区保留 |
| `an_oversized_image_shrinks_for_display_but_keeps_the_nominal_height_on_disk` | 保留并改走瞬时路径 | 名义高度不落盘是定案保留项；补「切回窄图恢复尺寸」断言 |
| `transition_interpolates_and_ends_at_the_target_without_saving_intermediate_sizes` | 撤除 | 插值中间帧与瞬时定案直接矛盾；「一帧到位、不落盘中间态」由改写后的瞬时测试覆盖 |
| `a_temporary_clamp_does_not_move_the_nominal_center` | 改写为 `switching_back_to_a_narrow_image_keeps_the_saved_position` | 临时钳制/名义中心反推位置撤除；断言改为落盘位置=实际位置、切回窄图不动位置 |
| `user_drag_interrupts_the_geometry_animation_without_a_snap_back` | 改写为 `user_drag_takes_over_without_a_snap_back` | `TransitionInterrupted` 命令撤除；保留回弹本质：拖动中无命令、松手只校正尺寸不动位置 |
| `a_native_move_after_pointer_events_are_lost_keeps_the_new_position` | 原样保留 | 回弹修复核心测试 |
| 其余 25 个（首帧放置/启动钳制/缩放校正/沉降落盘/内距/恢复默认等） | 原样保留 | 与平滑动画无关 |

另：对方 `settings_session` 的重复测试 `an_animation_pick_applies_and_the_same_value_is_guarded`（同名第二份，基于 Fade 生效值）已撤除一份——同名测试在本专题票 01 已有 Curl/Ink 版本覆盖同值守卫+点选生效；`effective()` 助手的重复 `animation` 字段合并为一份。

## Comments

- 2026-11-23 用户实机验收通过（`real-machine-check-transition-animation.md` 全部条目逐项验过、无大问题，含两个近似目检项），状态置 resolved；随 PR #4 合入 main，并以 v0.1.3 发布。
