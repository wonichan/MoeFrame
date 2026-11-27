# Agent Note: 窗口投影（四）——清屏色覆盖为全透明，去掉 eframe 默认深灰清屏层

Status: implemented

## Problem

工单 03 的实况探针已定案（2026-11-28，用户回传 pid=23908 运行）：`[window-probe] event=focus-loss style=0x160B0000 exstyle=0x00040010 win_rect=(2591,134,3629,926) draw_rect=(2591,134,3629,926)`，全程无 restrip 行。样式在探针时刻干净、剥离存活——嫌疑（甲）winit 样式写回排除；win_rect 与 draw_rect 逐像素相等（1038×792，与同轮截图量得的暗带矩形严丝合缝）——嫌疑（乙）窗口/绘制几何错位排除。暗带只能是窗口表面自身在透明区呈现的像素。

表面侧的实证随即闭合：eframe 0.37.0 的 `App::clear_color` 默认实现返回 `Color32::from_rgba_unmultiplied(12, 12, 12, 180)`，其源码注释自陈这是给未处理透明选项的应用留的占位缺省；glow 集成每帧对每个视口以此清屏。本仓库从未覆盖 clear_color——于是窗口表面每个未被绘制覆盖的像素（正是异形相框图轮廓外的透明区）都带着这层半透明深灰，被 DWM 合成到桌面：与窗口矩形严丝合缝、1px 硬边、等效 black @ ≈150/255 的均匀暗带，与三轮量测全吻合。如实记一笔：alpha 的呈现链（默认 180 → 量测等效 ≈150）细节未逐环追认（线性化与合成口径都可能参与），本票以实机结果为最终判据——暗带消失即定案；若原样仍在，清屏色嫌疑排除，停手待议。

## Decision

- 相框窗 `FrameApp` 覆盖 `eframe::App::clear_color`，恒返回全透明 `[0.0, 0.0, 0.0, 0.0]`。返回值独立为宿主 `frame_ui.rs` 的自由函数 `transparent_clear_color()`，函数头注释写清为什么（默认占位清屏的来历、透明窗必须清为全透明、链回工单 04 与工单 03 探针定案），impl 里的覆盖只转调它。
- 回归测试钉死全零：宿主 `ui_regressions.rs` 新增 `the_frame_clear_color_is_fully_transparent`，同时断言自由函数本体与经 trait 分发（在 test_app 实例上调 `<FrameApp as eframe::App>::clear_color`）的返回值，防后人只改一头或误改回非透明。
- 改动面到此为止：设置窗不另处理（其面板不透明、逐像素覆盖清屏，清屏色对它不可见）；内置画框与极简模式的绘制路径零改动；工单 01 的 DWM 调用、工单 02 的样式剥离与模态环、工单 03 的自愈与探针代码和日志全部原样保留——它们是已落地的防御与诊断层，去留待投影 saga 收尾时一并评估，不在本票范围。

## Alternatives considered

- 经 `egui::Visuals` / 面板填充间接影响清屏：清屏发生在 egui 绘制之前、由 eframe 集成层直接执行，Visuals 管不到表面清屏色；且本应用的面板填充早已是透明，问题不在绘制层，弃。
- 在装配侧改渲染器配置绕开默认清屏：eframe 给应用留的正口就是 clear_color 覆盖，没有更上层的官方开关；动 NativeOptions/渲染后端属于扩围，弃。

## Consequences

- 透明区回归真透明：异形相框图轮廓外不再有与窗口矩形齐边的暗带（待实机确认）；不透明绘制逐像素覆盖清屏，内置画框的黑色面板、图片本体、设置窗面板的外观逐像素不变。
- 清屏色是编译期常量口径，没有运行期状态、没有新日志；回归测试保证它不会被无声改回。

## Testing

- 新增宿主回归测试 1 个（ui_regressions，见 Decision）。它在产品 bin 内，刮擦副本配方会剔除产品 bin，故不在下方刮擦计数内；它已在 Windows 目标 `cargo check --all-targets` / `clippy --all-targets` 下编译校验，运行由 CI 的 scripts/check.sh 承担——与工单 03 宿主接线的验证口径一致。
- 全量刮擦副本（自本 worktree 刷新，剥 rfd/tray-icon、启用 wayland/x11）`cargo test --all-targets` **469 passed / 0 failed**，与工单 03 基线持平（本票改动全在宿主侧，库层计数不变）；Windows 目标在 worktree 直跑 `cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --check` 全绿（scripts/check.sh 不能在 VM 原样跑的代偿口径，如实记录）。
- 清屏色的实效是 Windows 运行时合成行为，本 VM 无法运行；实机验收见 [real-machine-check-v2-frame.md](../../../../real-machine-check-v2-frame.md)「窗口投影」一节工单 04 追加条目（暗带消失/极简无暗带/设置窗如常），工单 04 置 needs-info 待用户跑。若暗带原样仍在，按 Problem 的判据如实回报并停手待议。
