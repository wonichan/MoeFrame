# 工单 04：清屏色覆盖为全透明——去掉 eframe 默认深灰清屏层

**What to build:** 相框窗（FrameApp）覆盖 eframe::App::clear_color，恒返回全透明
[0.0, 0.0, 0.0, 0.0]。eframe 0.37 的默认实现返回 (12,12,12,180) 的半透明深灰
（其源码注释自陈是给未处理透明选项的应用留的占位缺省），glow 集成每帧以此清屏；
本应用从未覆盖它，于是窗口表面每个未被绘制覆盖的像素——正是异形相框图轮廓外
的透明区——都带着这层深灰被 DWM 合成到桌面上，呈与窗口矩形严丝合缝、1px 硬边、
等效 black @ ≈150/255 的均匀暗带（量测链见工单 03 Comments 的探针定案）。
清屏色改为全透明后，透明区回归真透明；不透明绘制（内置画框的黑色面板、图片
本体、设置窗面板）逐像素覆盖清屏，不受影响。

**Blocked by:** 无（工单 03 的探针已排除样式写回与几何错位两个嫌疑，本票是定案后的修复）

**Status:** ready-for-agent

## Acceptance criteria

- [ ] FrameApp 覆盖 clear_color 恒返回 [0.0; 4]，注释写清为什么（eframe 默认 (12,12,12,180) 占位清屏 + 透明窗必须清为全透明，链回本工单）；返回值经一个可单测的自由函数/常量给出并有回归测试钉住全零（防后人误改回非透明）
- [ ] 设置窗外观与行为不变（其面板不透明、逐像素覆盖清屏）；内置画框与极简模式绘制路径零改动
- [ ] 工单 01/02/03 的既有代码与日志零改动
- [ ] 全套验证通过：刮擦副本 cargo test --all-targets、Windows 目标 check / clippy -D warnings / fmt（scripts/check.sh 本机不可跑，如实记录替代口径）
- [ ] 实现笔记落 .agents/notes/implemented/ 并过三道校验门（format 门既有 4 篇失败不代改）；工单 03 Comments 补记探针定案（甲/乙 两嫌疑排除、win_rect=draw_rect、样式在探针时刻干净）
- [ ] 实机清单「窗口投影」节追加验收条目：PR 合并构建下自定义哥特相框四周暗带消失（亮壁纸宽幅截图）、极简模式窗口透明区无暗带、设置窗外观正常；本票 Status 置 needs-info 待该轮实机确认

## Context

- 探针定案（2026-11-28，用户回传 pid=23908 运行）：[window-probe] event=focus-loss
  style=0x160B0000（CAPTION/SIZEBOX 位不在，剥离存活）、exstyle=0x00040010
  （WINDOWEDGE 不在）、win_rect=(2591,134,3629,926) 与 draw_rect 逐像素相等
  （1038×792，与同轮截图量得的暗带矩形 1038×792 严丝合缝）。无 restrip 行。
  结论：暗带不是样式问题、不是几何错位，而是窗口表面自身在透明区呈现的像素。
- eframe 0.37.0 源码实证：src/epi.rs 默认 clear_color 返回
  egui::Color32::from_rgba_unmultiplied(12, 12, 12, 180)；src/native/glow_integration.rs
  每帧对每个视口调 painter.clear(screen, clear_color)。本仓库 grep 确认应用从未
  覆盖 clear_color。应用使用 glow 后端（Cargo.toml eframe features 含 glow）。
- 量测口径：暗带等效 black @ ≈150/255（多张截图、多壁纸、多边一致）；默认清屏
  经 to_normalized_gamma_f32 的线性化后 RGB 压至近黑，与「色近黑」吻合；alpha 的
  呈现链（180 → 等效 ≈150）细节不逐环追认——本票以实机结果为最终判据：暗带消失
  即定案；若暗带原样仍在，清屏色嫌疑排除，回报中如实记录并停手待议。

## Comments
