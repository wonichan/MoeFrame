# 01 内置画框圆角——布局接缝出圆角数据、宿主沿弧作画与文档修订

**What to build:** 按 `.scratch/rounded-corners/spec.md` 给内置画框的三个角
边界（窗口外沿 / 框体内沿 / 卡纸内沿）加圆角，半径独立定值 20 / 10 / 12
逻辑 px（原型四变体对比后用户选定 C，见 spec Further Notes）。布局接缝
（库层 frame_layout）在现有层与明暗数据之上补出四角圆心与夹紧后半径、角部
弧段要素；宿主 frame_paint 沿数据作画，截面明暗、唇线、凹槽、木纹、框→卡
纸投影、卡纸斜切面、卡纸→画面细影、外沿内侧软阴影全部沿圆弧连续。内角覆
盖式：插画解码与绘制矩形不变，卡纸圆弧压住角尖。几何、命中、解码零改动；
极简与自定义相框图路径不触碰。附带文档修订（FR-311 / FR-312 / README）
与真机清单新增「圆角」一节。

**Blocked by:** None（spec 与原型已在集成 分支 feature/rounded-corners 上）。

**Status:** resolved

## Acceptance criteria

- [x] 布局接缝数据断言：默认内距（框 14 + 卡纸 18）下三边界半径为定值 20 / 10 / 12，四角圆心位置正确
- [x] 退化与夹紧：卡纸宽 = 0 时画面边缘半径为 12；框宽 = 0 时外沿半径落到卡纸带；带厚小于定值时半径按带厚夹紧且弧段不自交；极小窗口下半径受半宽 / 半高夹紧
- [x] 角部弧段齐全：截面插值带、唇线、凹槽线、木纹（胡桃木 / 原木）、框→卡纸投影、卡纸斜切面、卡纸→画面细影、外沿软阴影在四角均有弧段对应物，直边要素的数量与口径不变
- [x] 宿主沿弧作画且绘制计划结构不变：圆角要素只在内置臂可达，极简 / 自定义臂无圆角（paint_plan 层断言或既有测试覆盖）
- [x] 几何零改动：window_geom、解码目标、缩放带、命中相关代码无 diff 触及，其既有测试全绿
- [x] 检查口径如实达标：fmt --check 与 clippy -D warnings 以 Windows 目标直跑通过、test 以刮擦副本 `cargo test --all-targets` 296 绿代偿；`scripts/check.sh` 本体因本 VM 无 Linux GUI 构建条件未直跑（口径详见 Comments）
- [x] 文档修订完成：FR-311 补圆角口径、FR-312 补「内沿圆角覆盖角部不在裁切之列」、README「外观（V2）」内置画框条目补圆角一句（REQUIREMENTS.md 在仓库外，改动内容记入本票 Comments）
- [x] `real-machine-check-v2-frame.md` 尾部新增「圆角」独立一节（四验点：四角圆润无锯齿、外角透桌面正常、内角压图自然、滑杆 0 与 40 极值不穿帮），既有 27 项一字不动
- [x] 实现笔记落 `.agents/notes/implemented/` 并通过三道校验门（tree / format / archived）

## Context

- Spec：`.scratch/rounded-corners/spec.md`（含夹紧与退化规则、测试决策全文）
- 原型：`.scratch/rounded-corners/prototype-rounded.html`（仅供半径观感参照，
  CSS 近似不是渲染口径；半径定值以 spec 为准）
- 先行实现：内置画框明暗重做（布局接缝 builtin_shading + 宿主 frame_paint），
  本票在同一接缝上扩圆角数据，沿用其测试惯例（tests/frame_layout.rs 数据断言）
- 半径为固定常量（与 PROFILE_STOPS 同级），不入配置、无设置项

## Comments

- 2026-11-22 实现完成（提交 e548256，分支 impl/rounded-corners-01）：
  布局接缝 `BuiltinShading.corners` 给出三边界圆心与夹紧后半径
  （定值 20 / 10 / 12），造型线 / 两道 FadeBand / 斜切面各带弧
  半径；宿主 frame_paint 沿数据作画（直边条带 + 四角描弧、木纹
  直边钳制 + 角部短弧、覆盖式角部修补），window_geom / 命中 /
  解码文件零 diff。新增接缝测试 4 个（tests/frame_layout.rs），
  既有测试一字未改。
- 验证口径：本 VM 无 Linux GUI 构建条件，`scripts/check.sh` 的
  fmt 与 clippy 在本分支以 Windows 目标直跑全绿（fmt --check、
  clippy --all-targets -D warnings、check --all-targets），test
  以刮擦副本（剥 rfd / tray-icon、启用 wayland/x11）代偿：
  296 passed / 0 failed（基线 292 + 新增 4）。绘制观感待用户
  按真机清单「圆角」一节验收，故 Status 置 needs-info。
- 「外沿软阴影」的弧连续口径：现行绘制里 FR-313 的框体外沿
  内侧观感由截面首档 + 外沿压线承载（观感重做时已删除独立的
  扩散带图层），二者在角部均已沿弧——截面首档随角弧带落笔、
  外沿压线以接缝给出的 outer_radius 描边。
- REQUIREMENTS.md（仓库外 `~/workspace/moe-frame/REQUIREMENTS.md`）
  改动全文：FR-311 末尾在「内置模式单边内距 = 框宽 + 卡纸宽」
  后补「；三个角边界（窗口外沿 / 框体内沿 / 卡纸内沿）均为圆
  角，半径独立定值 20 / 10 / 12 逻辑 px（固定常量、不设设置
  项），实际半径按相邻带厚与窗口尺寸夹紧，卡纸 = 0 时画面边
  缘圆角由框体内沿呈现、框宽 = 0 时外沿圆角落在卡纸带上；圆
  角只在绘制层生效，内距、窗口几何、解码与命中口径不变」；
  FR-312 末句改为「任何外观下插画完整显示、绝不裁切（内置
  画框的内沿圆角以卡纸圆弧覆盖插画角尖，不在裁切之列）」。
- 实现笔记：`.agents/notes/implemented/feature/2026-11-22-builtin-frame-rounded-corners.md`
  （三道校验门已过，47 篇）。
- 2026-11-22 code-review 双轴修复（提交 3b56635，分支
  impl/rounded-corners-review-fixes）：截面条带角部弧半径改
  由布局接缝 `corner_strip_radii` 逐带出数据，宿主只照单描
  弧（接缝新增断言 1 个；复验口径同前——刮擦副本全量 297
  passed / 0 failed，Windows 目标 check / clippy / fmt
  --check 直跑全绿）；宿主角弧方向表与采样抽为共用
  CornerArc、扇形填充参数打包为 CornerSector。本票验收项 6
  的表述已改写为如实口径，重复的 Comments 标题已合并。木
  纹角弧的宿主机制口径、Story 9 软阴影的承载映射、egui
  CornerRadius 的 u8 取整三项差异注记，见 spec Further
  Notes 评审后补记 1–3。

- 2026-11-22 真机验收通过：用户已跑完真机清单「圆角」一节，四点
  全过（四角圆润无锯齿、外角透桌面正常、内角压图自然、滑杆极值
  不穿帮），清单四项已勾选；PR #3 已由用户合并入 main
  （cc3e975）。Status 置 resolved，工单关闭。
