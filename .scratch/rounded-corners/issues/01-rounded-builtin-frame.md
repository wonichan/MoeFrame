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

**Status:** ready-for-agent

## Acceptance criteria

- [ ] 布局接缝数据断言：默认内距（框 14 + 卡纸 18）下三边界半径为定值 20 / 10 / 12，四角圆心位置正确
- [ ] 退化与夹紧：卡纸宽 = 0 时画面边缘半径为 12；框宽 = 0 时外沿半径落到卡纸带；带厚小于定值时半径按带厚夹紧且弧段不自交；极小窗口下半径受半宽 / 半高夹紧
- [ ] 角部弧段齐全：截面插值带、唇线、凹槽线、木纹（胡桃木 / 原木）、框→卡纸投影、卡纸斜切面、卡纸→画面细影、外沿软阴影在四角均有弧段对应物，直边要素的数量与口径不变
- [ ] 宿主沿弧作画且绘制计划结构不变：圆角要素只在内置臂可达，极简 / 自定义臂无圆角（paint_plan 层断言或既有测试覆盖）
- [ ] 几何零改动：window_geom、解码目标、缩放带、命中相关代码无 diff 触及，其既有测试全绿
- [ ] `scripts/check.sh` 全绿（fmt --check + clippy -D warnings + test --all-targets），Windows 目标 check 通过
- [ ] 文档修订完成：FR-311 补圆角口径、FR-312 补「内沿圆角覆盖角部不在裁切之列」、README「外观（V2）」内置画框条目补圆角一句（REQUIREMENTS.md 在仓库外，改动内容记入本票 Comments）
- [ ] `real-machine-check-v2-frame.md` 尾部新增「圆角」独立一节（四验点：四角圆润无锯齿、外角透桌面正常、内角压图自然、滑杆 0 与 40 极值不穿帮），既有 27 项一字不动
- [ ] 实现笔记落 `.agents/notes/implemented/` 并通过三道校验门（tree / format / archived）

## Context

- Spec：`.scratch/rounded-corners/spec.md`（含夹紧与退化规则、测试决策全文）
- 原型：`.scratch/rounded-corners/prototype-rounded.html`（仅供半径观感参照，
  CSS 近似不是渲染口径；半径定值以 spec 为准）
- 先行实现：内置画框明暗重做（布局接缝 builtin_shading + 宿主 frame_paint），
  本票在同一接缝上扩圆角数据，沿用其测试惯例（tests/frame_layout.rs 数据断言）
- 半径为固定常量（与 PROFILE_STOPS 同级），不入配置、无设置项

## Comments

（暂无）
