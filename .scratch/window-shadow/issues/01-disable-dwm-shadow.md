# 01 关掉相框窗口的系统投影（DWM shadow）

**What to build:** 修复「自定义异形相框周围有一圈矩形阴影」。诊断已查
实（2026-11-27 用户实机截图）：相框图 PNG 本身干净（合成到品红/深绿
底上外轮廓无光晕），应用也不给自定义模式画阴影（FR-315 口径）；那圈
圆角矩形、比相框大一圈、微微下沉的带是 **Windows 11 DWM 给相框窗口
本体加的系统投影**——它跟窗口矩形走，不跟相框异形轮廓走，用矩形内
置框时藏在框边上看不见，换透明异形框就露馅。修法：相框窗口创建后，
在宿主 Windows 代码里对相框窗口 HWND 调
`DwmSetWindowAttribute(DWMWA_NCRENDERING_POLICY, DWMNCRP_DISABLED)`
关掉非客户区渲染（投影随之消失）。这也落实 FR-313「不使用原生窗口
阴影」的既有口径。设置窗是普通窗口，不动它。

**Blocked by:** None（基线 main @ 50687a8，含 PR #12 置顶守擂）。

**Status:** needs-info

## Acceptance criteria

- [x] 相框窗口启动后 DWM 非客户区渲染被禁用（DwmSetWindowAttribute 调用落在相框窗口 HWND 上、仅 Windows、仅一次），设置窗不被触碰
- [x] 调用结果记一行日志（成功/失败及错误码，走 moe_frame.log 既有口径），失败不影响启动与任何既有行为
- [x] 不与既有窗口机制冲突：透明窗口、8px 缩放带（应用自绘命中测试）、WM_MOVING 吸附子类化、置顶守擂自检、托盘隐藏/召回既有测试全绿
- [x] 三种外观模式行为不回归：极简/内置画框原本靠绘制层内侧阴影（FR-313），不依赖原生投影，关掉后观感口径不变（实机清单确认）
- [x] `scripts/check.sh` 口径全绿（fmt / clippy -D warnings / test --all-targets；VM 代偿口径如实记入 Comments）
- [x] 实机清单新增「窗口投影」一节：自定义异形框贴桌面无矩形影带；切换极简/内置画框无异常；设置窗外观如常（保留其窗口投影）；重启应用后仍无影带
- [x] 实现笔记落 `.agents/notes/implemented/` 并通过三道校验门

## Context

- 诊断来源：2026-11-27 用户截图（哥特相框套用后四周一圈灰影）；排查记录见主会话当日对话（PNG 合成验证 + 仓库 grep：宿主无任何 shadow 处理，frame 窗口为 with_decorations(false) + with_transparent(true)）
- HWND 获取与 Win32 调用先例：吸附子类化（WM_MOVING）与 window-validation 观测代码、置顶守擂的 GetWindowLongPtrW 自检
- 若 DWMNCRP_DISABLED 在用户 Win11 版本上不足以去掉投影，后备路线（摘 WS_THICKFRAME 等）只记入 Comments 待议，本票不预先实现

## Comments

- 2026-11-27 实现完成（分支 fix/window-shadow）：库层新增纯判定
  `src/dwm_shadow.rs`——与 Win32 文档对齐的常量
  （DWMWA_NCRENDERING_POLICY=2、DWMNCRP_DISABLED=1）与 HRESULT
  日志口径（result=ok/failed + 十六进制码），单测以文档值为独立
  真值钉住；宿主新增 `src/bin/moe-frame/dwm_shadow.rs` 接线模块，
  在装配处（创建闭包取得相框 HWND、挂完吸附子类化之后）对相框
  窗口调一次 DwmSetWindowAttribute，结果经新增的
  `dwm_shadow_record` 以 `[dwm-shadow]` 前缀记一行 moe_frame.log；
  句柄未就绪记 event=skipped，失败/跳过均不影响启动。设置窗未碰。
  windows-sys 新增 Win32_Graphics_Dwm 特性，无新增依赖。
- 诊断补正（主会话排查口径修正）：创建闭包里自工单 10 起已有
  winit 的 `set_undecorated_shadow(false)`，并非「宿主无任何
  shadow 处理」——主会话 grep 范围漏了 src/bin/moe-frame.rs 本
  体。该开关只让 winit 不主动索取投影（不扩边框进客户区），挡
  不住 DWM 给可缩放无边框窗口画的投影，用户截图即取自含该调
  用的构建；本票的 DWMNCRP_DISABLED 在 DWM 层面关闭，与之互
  补、不替代（详见实现笔记）。
- 验证（VM 代偿口径，如实记录）：库层单测 6 个 TDD 红→绿在
  Linux 刮擦副本真跑；刮擦副本 `cargo test --all-targets` 440
  个全绿（基线 434 + 新增 6）；Windows 目标在 worktree 直跑
  `cargo check --all-targets`、`cargo clippy --all-targets --
  -D warnings`、`cargo fmt --check` 全绿。宿主 Dwm 调用是
  Windows 一次性接线，本 VM 无法运行其实效，由实机清单「窗口
  投影」一节验收，本票置 needs-info。
- 实现笔记 `.agents/notes/implemented/bug-fix/2026-11-27-dwm-
  shadow.md` 三道校验门：tree 通过、archived 通过、format 门
  失败名单恰为仓库既有 4 篇（crossfade-merge、
  multi-instance-snap-review-fixes、leader-driven-group、
  wm-moving-snap），本笔记不在名单、未代改。
- 后备路线待议（本票不预先实现）：若实机确认 DWMNCRP_DISABLED
  不足以去掉投影（日志 result=ok 但影带仍在），下一步候选为摘
  除 WS_THICKFRAME 等样式位，须另票评估对缩放/吸附的影响。
- 2026-11-27 后备路线正式承接：实机回执本票日志 result=ok 而
  暗层依旧（触发条件成立），根因定位为 winit 恒置的边框样式
  使 DWM 按带框窗口画框架影；剥样式 + 自研模态环接管拖动/
  缩放由工单 02（02-strip-frame-styles.md）实现，本票的 DWM
  调用与日志保留作基线对照。
