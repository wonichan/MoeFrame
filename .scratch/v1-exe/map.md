# V1 可运行程序

Label: wayfinder:map

## Destination

一个通过需求文档第 10 节的 release `MoeFrame.exe`，外加简短 README。第 10 节包含 P0 行为、NFR-1～NFR-5 和 playlist 单测。P1 不挡完成。不是只弹出窗口的骨架，也不是 debug 目录里的临时二进制。

## Notes

本图覆盖 wayfinder 的默认：允许把实现做进本图，终点是可运行的程序，不是一份只含决定的交接路线。上一张图 [V1 交接路线](../v1-route/map.md) 已作废，不要继续。

手势分界、重启后续播、设置窗边界已关闭，分别按 [ADR-0001](../../docs/adr/0001-defer-side-click.md)、[ADR-0002](../../docs/adr/0002-persist-round-and-history.md)、[ADR-0003](../../docs/adr/0003-settings-window-boundary.md) 实现。在 [边缘缩放先验证](issues/06-resize-before-switch.md) 和 [全分辨率解码](issues/07-full-res-decode.md) 关闭之前，不改 GUI 框架，不改解码策略。里程碑先不要预切。

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
- 降采样解码：image 0.25.10 不能边解码边缩小。实测峰值字节数未知。见 [降采样解码](issues/05-downsample-decode.md)。
## Not yet specified

- 第 11 节里程碑怎么切成可执行的票。等 [边缘缩放先验证](issues/06-resize-before-switch.md)、[全分辨率解码](issues/07-full-res-decode.md) 关闭后再说，现在不预切。
- 第 10 节里必须人在这台机器上点的验收（拖动、托盘、开机自启、任务管理器内存）谁点通过。

## Out of scope

- P1 行为：悬停条、悬停暂停、显示文件名、淡入淡出或无动画、定时重扫、拖放文件夹、avif、单实例、图库热更新。第 10 节不验收，不挡完成。
- 第 2.2 节非目标：全屏壁纸、动态壁纸、WorkerW、图片编辑、下载、在线图源、账号、同步、联网、多相框、macOS/Linux 适配。
- 第 12 节 V2 候选。
- Stretch。显示模式只有适应与填充。
