# V1 可运行程序

Label: wayfinder:map

## Destination

一个通过需求文档第 10 节的 release `MoeFrame.exe`，外加简短 README。第 10 节包含 P0 行为、NFR-1～NFR-5 和 playlist 单测。P1 不挡完成。不是只弹出窗口的骨架，也不是 debug 目录里的临时二进制。

## Notes

本图覆盖 wayfinder 的默认：允许把实现做进本图，终点是可运行的程序，不是一份只含决定的交接路线。上一张图 [V1 交接路线](../v1-route/map.md) 已作废，不要继续。

手势分界、重启后续播、设置窗边界已关闭，分别按 [ADR-0001](../../docs/adr/0001-defer-side-click.md)、[ADR-0002](../../docs/adr/0002-persist-round-and-history.md)、[ADR-0003](../../docs/adr/0003-settings-window-boundary.md) 实现。[边缘缩放先验证](issues/06-resize-before-switch.md) 和 [全分辨率解码](issues/07-full-res-decode.md) 也已关闭路线决定，但两项实际验证尚未执行。先分别做独立最小验证，通过后才进入对应的正式实现；失败先排查，确认仍不达标后再另开备选票，不直接更换 GUI 框架或解码器。本轮不预切里程碑。

领域用词以根目录 `GLOSSARY.md` 为准。grilling 票同时使用 grilling 与 domain-modeling。不要把本图票标成 `ready-for-agent`。

本仓库有 git remote。研究结论已写到 `.scratch/v1-route/research/`，不建抛支、要推送。

编码默认（小细节，不再开票）：

- 历史只存路径，不存像素。
- 空图库显示空状态，不崩溃，不反复弹窗。
- 不做 Stretch。
- 切换动画进行中，新的切换意图合并为最后一次，不并行开播。
- 跳过坏图时，把路径追加到 `%APPDATA%\MoeFrame\moe_frame.log`。
- 自然排序只规定连续数字按数值比较，其余按 Unicode 码点，不按拼音。

已钉死、不要重开：Rust + eframe/egui，无 WebView，无联网，配置在 `%APPDATA%\MoeFrame\config.toml`，关闭是隐藏，退出才结束进程。

## Decisions so far

- 手势分界：侧边单击等系统双击窗口结束才切换，双击不切换。见 [ADR-0001](../../docs/adr/0001-defer-side-click.md) 和 [手势分界](issues/01-gesture-boundary.md)。
- 重启后续播：恢复当前图，本轮进度和历史分开并跨启动保存，不恢复洗牌袋。见 [ADR-0002](../../docs/adr/0002-persist-round-and-history.md) 和 [重启后续播](issues/02-resume-random.md)。
- 设置窗边界：设置窗必做，但不出现悬停暂停、动画类型和显示文件名。三键按第 6 节默认写入，合法手改只保存，行为不跟。间隔是 3 到 99999999 秒。见 [ADR-0003](../../docs/adr/0003-settings-window-boundary.md) 和 [设置窗边界](issues/03-settings-boundary.md)。
- 无边框窗口：eframe 0.37.0 不能把四条同时记成已证实。拖边缘未跑过。见 [无边框窗口能力](issues/04-borderless-window.md)。
- 边缘缩放先验证：先在本机 Windows 11 x64（计划 Build 26100，实际 26200、缩放 150%）做独立最小验证，通过后才继续正式窗口实现并沿用 eframe/egui；失败先排查调用方式，确认仍不能满足要求后另开票，不直接换框架。验证含四边四角、120×120 逻辑最小尺寸、内容区移动、创建时不抢焦点、默认不置顶、DPI 与物理尺寸记录，以及最大化后恢复的移动和缩放。由用户手工确认；本机通过不是完整兼容性验收。用户已于 2026-11-17 实机确认通过，见 [08 窗口最小验证](issues/08-window-validation.md)与[操作清单](../../docs/window-validation.md)。
- 降采样解码：image 0.25.10 不能边解码边缩小。实测峰值字节数未知。见 [降采样解码](issues/05-downsample-decode.md)。
- 全分辨率解码：暂定保留 image，允许短暂全分辨率解码，缩到窗口物理像素尺寸（填充按覆盖尺寸）后释放整图；长期只留当前图与下一张的显示数据。用 Limits 拒绝过大输出，记录路径和超限原因后跳过，不承诺 100MP 一定显示，普通 1080p、2K、4K 图片仍须正常显示。80MB 约束解码、缩放及下一张预加载完成后的稳定工作集，连续轮播 1 小时无持续增长的要求不变。先做独立最小验证，通过后才正式实现；瞬时峰值上限和 Limits 阈值实测后由用户确认，不能把 Limits 当作进程内存硬上限。路线已定，验证尚未执行。见 [全分辨率解码](issues/07-full-res-decode.md)。

## 当时待确认（历史记录）

- 无边框最小验证已在本机（Win11 Build 26200、150% 单屏）由用户实机确认通过，正式窗口实现的本机门禁解除；Build 26100、Win10、100% 缩放与混合 DPI 双屏仍未验证，不沿用本机结果。实测最小客户区 188×183.3 逻辑点，**120×120 逻辑下限未达成也未验证**，留给正式窗口实现解决。
- 解码最小验证尚未执行，没有内存达标结果。下一步测普通图片与超大图的稳定工作集、瞬时峰值和拒绝行为，并提出峰值上限、Limits 阈值建议，由用户确认；[07](issues/07-full-res-decode.md) 已关闭的是路线决定，不是验证执行或验收通过。
- 第 11 节里程碑怎么切成可执行的票。[边缘缩放先验证](issues/06-resize-before-switch.md)、[全分辨率解码](issues/07-full-res-decode.md) 的路线决定现已关闭；本轮只记录决定，不预切实现票。
- 第 10 节里必须人在这台机器上点的验收（拖动、托盘、开机自启、任务管理器内存）谁点通过。

## V1 交付结果

- 窗口门禁：[08](issues/08-window-validation.md)完成独立验证；其未达成的 120×120 下限随后在正式程序的[票 23 实机回执](acceptance-23.md#人工待办本人执行后勾选并回填)中验证。两次证据不混用。
- 解码门禁：[09](issues/09-decode-memory-validation.md)完成独立验证；最终 release 在[22](issues/22-release-resource-budgets.md)确认稳态预算 ≤80 MB、峰值预算 ≤192 MB（实测峰值 130.82 MB）。
- 实施票已拆为 08–23；[23](issues/23-release-acceptance.md)记录交付与用户实机验收，状态 `done`。Windows 10、100% / 125% 缩放和混合 DPI 双屏没有被这份回执覆盖。

## Out of scope

- P1 行为：悬停条、悬停暂停、显示文件名、淡入淡出或无动画、定时重扫、拖放文件夹、avif、单实例、图库热更新。第 10 节不验收，不挡完成。
- 第 2.2 节非目标：全屏壁纸、动态壁纸、WorkerW、图片编辑、下载、在线图源、账号、同步、联网、多相框、macOS/Linux 适配。
- 第 12 节 V2 候选。
- Stretch。显示模式只有适应与填充。
