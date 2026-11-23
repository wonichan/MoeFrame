# 2026-11-23 收编合并：main 的交叉淡化与几何工作并入切换动画专题

背景：main 上有本地 agent 直推的 `fcaea5e`（专题 `.scratch/switch-animation-and-geometry`：240ms 交叉淡化 + 切图几何平滑动画 + 设置两档 + 原生拖动回弹修复），与本专题 PR #4 正面冲突（11 个文件）。用户 2026-11-23 拍板选项一「以 PR #4 为准」收编。合并提交 `2b3c226`（merge origin/main）。

## 逐项取舍

以我们为准（保留本专题实现，对方同名实现撤除）：
- 档位/枚举：`config::Animation` 用本专题 8 档；对方两档枚举与 `Slide` 占位语义撤除。
- 核心状态机：`app_core` 的 StartTransition/完成提交/单意图队列原样；对方 app.rs 的 `ImageTransition` 淡化机器（begin/finish/advance/paint_crossfade、fitted_texture_rect、240ms 常量）整段删除，其单测随之删除（行为由核心+宿主骨架测试覆盖）。
- 宿主骨架：`transition.rs` 双纹理舞台为唯一转场路径；对方 frame_paint 直调 crossfade 的路径撤除（圆角绘制等 main 侧成果保留）。
- 设置窗：本专题播放组（播放模式→切换动画→间隔相邻、910 高）为唯一入口；对方会话层重复的 animation 快照字段、`SessionAction::ApplyAnimation` 重复臂、单独两档 UI 块撤除。
- 切图落位：Loaded 分支直接 `geometry.set_image_aspect(...)` 瞬时校正；对方的切图延迟落位（pending_aspect 等动画播完才变尺寸）撤除。

收编新增（淡化档）：
- `Animation::Fade`（存盘键 `fade`）+ `ConcreteAnimation::Fade`，`duration_ms` 240（对方原值），`fixed()` 直通、无方向。
- 宿主 `transition.rs` 新增 Fade 绘制臂：旧图满不透明垫底、新图以 `fade_alpha`（三次缓出 ×255）叠加；旧图居中不拉伸与其它臂一致。宿主单测 `fade_alpha_is_eased_and_clamped`。
- 随机池 `ConcreteAnimation::ALL` 5→6（含 Fade），等概率/禁连续重复不变；随机测试的脚本索引边界随池长更新。
- 设置窗 radio 第二行增「淡化」，共 8 档。
- 配置迁移：存量 `fade` → Fade（对方两档时代该值是真实行为，不再按旧口径落 Curl）；`slide` → Curl、`none` → None 不变。config 测试含来历注释与显式断言。

几何（以 spec §7 瞬时口径为准）：
- 撤除：`window_geom` 的 `GeomAnimation`、begin/transition_active/cancel 接口、`transition_deadline_secs`、`GeomCommand::TransitionInterrupted`、中心锚定目标计算、切图位置 6px 钳制与全部插值辅助；宿主 geometry.rs 的动画推进/截止重绘/中断臂随之删除。
- 保留（独立成立）：名义高度语义与 `nominal_record` 落盘（尺寸按名义量、临时缩高不落盘）；切图瞬时校正时按工作区适配显示尺寸（原平滑目标计算中的 fit 部分，改为瞬时一步到位）；工作区观察通路（宿主 hwnd 取 rcWork 喂入）；恢复默认/首帧放置/启动钳制的既有行为。
- 必要修正：中心锚定撤除后，`nominal_record` 的落盘位置改为以实际观察位置为准、名义中心仅兜底——否则从未移动过的窗口会被按名义中心反推的位置写错。名义中心字段降为兜底与拖动跟踪用途，保留。
- 无条件保留：原生拖动回弹修复全套——宿主 `native_gesture_active` 旗标（moe-frame.rs/input.rs/app.rs/geometry.rs）、几何机指针按下名义更新与松手强制重校正、测试 `a_native_move_after_pointer_events_are_lost_keeps_the_new_position` 原样通过。

测试处置：window_geom 对方 32 测试 → 31（改写 4、撤除 1「插值中间帧」、原样保留 26，逐条表见工单 05）；settings_session 对方同名重复测试撤除一份（本专题版本覆盖）；config 的 `Animation::Slide` 旧断言改为 Curl（默认模板无 animation 键 → 默认卷页）。

文档：对方 spec/工单/实施笔记与 ADR-0003 各加 2026-11-23 收编注记（原文不删）；本专题 spec 更新为 8 档/池 6/fade 迁移并加 §11 合并注记；GLOSSARY、README 同步；实机清单新增工单 05 一节（淡化/瞬时几何/临时缩高/回弹修复）。

## 验证口径（如实）

- Linux 测试副本（`~/workspace/tmp/refresh-moeframe-test-merge05.sh`，与前票同法：副本启 wayland/x11、去 rfd/宿主 bin）：`cargo test --all-targets` 全绿，共 327 通过 / 0 失败（app_core 60、config 33、settings_session 28、window_geom 31、window_validation 13、transition_ambient 12 等）。
- Windows 目标（真实仓库）：`cargo check --target x86_64-pc-windows-gnu --all-targets`、`cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings`、`cargo fmt --check` 全过——宿主 bin 的单测（含新增 fade_alpha 测试）在此口径下编译验证，运行验证随实机清单。
- `scripts/check.sh` 本体依赖 Windows 宿主（或本机完整工具链），在本 VM 无法原样运行，沿用前票口径以上两条替代；CI 与用户实机以 check.sh 为准。
