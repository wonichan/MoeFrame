# V1 交接路线

Status: superseded
Superseded by: [V1 可运行程序](../v1-exe/map.md)

终点已改画。不要在本图上继续。下面的原终点保留，不改写成反面。

Label: wayfinder:map

## Destination

锁定一条可交给编码会话的 V1 决策路线，对齐需求文档第 10 节（P0 行为、NFR-1～NFR-5、playlist 单测）。走到头时，动手写 `MoeFrame.exe` 之前没有还要拍板的路线问题。本图不写程序。

## Notes

领域是桌面相框，不是壁纸。用词以根目录 `GLOSSARY.md` 为准：相框、图库、播放序列、历史、轮播、切换、关闭、退出。

每个会话先读本图，再按票的 `Type:` 工作。grilling 票同时使用 grilling 与 domain-modeling。不要把本图票标成 `ready-for-agent`。

本仓库没有 git remote。研究结论写在 `.scratch/v1-route/research/`，不建抛支、不推送。票上的上下文指针指向该文件。

编码默认（小细节，不再开票）：

- 历史只存路径，不存像素。
- 空图库显示空状态，不崩溃，不反复弹窗。
- 不做 Stretch。
- 切换动画进行中，新的切换意图合并为最后一次，不并行开播。
- 跳过坏图时，把路径追加到 `%APPDATA%\MoeFrame\moe_frame.log`。
- 自然排序只规定连续数字按数值比较，其余按 Unicode 码点，不按拼音。

已钉死、不要重开：Rust + eframe/egui，无 WebView，无联网，配置在 `%APPDATA%\MoeFrame\config.toml`，关闭是隐藏，退出才结束进程。

## Decisions so far

## Not yet specified

- 若 [无边框窗口能力](issues/04-borderless-window.md) 证明 eframe 不能同时满足无边框、可缩放、按物理像素、不抢焦点，备选界面方案是什么。
- 若 [降采样解码](issues/05-downsample-decode.md) 证明按窗口物理像素只持有两张图仍守不住 50MB，或 100MP 会撑爆内存，备选解码路径是什么。

## Out of scope

- 写出 `MoeFrame.exe` 与第 11 节里程碑实现。本图只锁定路线。
- P1 行为：悬停条、悬停暂停、淡入淡出或无动画、定时重扫、拖放文件夹、avif、单实例、图库热更新。第 10 节不验收，不挡交接。
- 第 2.2 节非目标：全屏壁纸、动态壁纸、WorkerW、图片编辑、下载、在线图源、账号、同步、联网、多相框、macOS/Linux 适配。
- 第 12 节 V2 候选。
- Stretch。显示模式只有适应与填充。
