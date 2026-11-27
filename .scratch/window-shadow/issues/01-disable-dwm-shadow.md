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

**Status:** ready-for-agent

## Acceptance criteria

- [ ] 相框窗口启动后 DWM 非客户区渲染被禁用（DwmSetWindowAttribute 调用落在相框窗口 HWND 上、仅 Windows、仅一次），设置窗不被触碰
- [ ] 调用结果记一行日志（成功/失败及错误码，走 moe_frame.log 既有口径），失败不影响启动与任何既有行为
- [ ] 不与既有窗口机制冲突：透明窗口、8px 缩放带（应用自绘命中测试）、WM_MOVING 吸附子类化、置顶守擂自检、托盘隐藏/召回既有测试全绿
- [ ] 三种外观模式行为不回归：极简/内置画框原本靠绘制层内侧阴影（FR-313），不依赖原生投影，关掉后观感口径不变（实机清单确认）
- [ ] `scripts/check.sh` 口径全绿（fmt / clippy -D warnings / test --all-targets；VM 代偿口径如实记入 Comments）
- [ ] 实机清单新增「窗口投影」一节：自定义异形框贴桌面无矩形影带；切换极简/内置画框无异常；设置窗外观如常（保留其窗口投影）；重启应用后仍无影带
- [ ] 实现笔记落 `.agents/notes/implemented/` 并通过三道校验门

## Context

- 诊断来源：2026-11-27 用户截图（哥特相框套用后四周一圈灰影）；排查记录见主会话当日对话（PNG 合成验证 + 仓库 grep：宿主无任何 shadow 处理，frame 窗口为 with_decorations(false) + with_transparent(true)）
- HWND 获取与 Win32 调用先例：吸附子类化（WM_MOVING）与 window-validation 观测代码、置顶守擂的 GetWindowLongPtrW 自检
- 若 DWMNCRP_DISABLED 在用户 Win11 版本上不足以去掉投影，后备路线（摘 WS_THICKFRAME 等）只记入 Comments 待议，本票不预先实现

## Comments

（暂无）
