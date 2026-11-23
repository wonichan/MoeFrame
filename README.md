# MoeFrame

Windows 10/11 x64 的无边框相框：选一个图片文件夹，它按你的高度把整张图铺满窗口，
按间隔自动轮播，托盘常驻，最小化到后台继续放。

免安装：到 [Releases](https://github.com/wonichan/MoeFrame/releases) 下载最新 `MoeFrame.exe`，
拷到任意目录双击即可运行；不装运行库、不写注册表。自己构建的产物是 `target\release\MoeFrame.exe`，
与发布版同一条命令构建，静态链接单文件。

## 外观（V2）

相框有三种外观，在设置窗「外观」组里切换，即时生效并持久化，右键菜单不加项：

- **极简**：图片贴边铺满、无画框，就是 V1 的样子。
- **内置画框**（默认）：程序绘制的框体 + 卡纸 + 内沿三层；框体四款预设
  （胡桃木 / 原木 / 黑 / 白），框宽与卡纸宽可调（0–40 逻辑 px，卡纸可为 0），
  带框体外沿内侧的自绘软阴影。
- **自定义相框图**：用自己画的 PNG 当画框。

设置的高度指相框整体（含画框）的总高度，宽度仍按当前图比例推导；
插画在任何外观下都完整显示、绝不裁切。

自定义相框图只需一张带透明通道的 PNG：中间留一个全透明的矩形区域作「洞」，
四周画框体图案，不用填任何参数——程序从图片中心自动找洞，洞可以偏置
（比如拍立得式宽下边）；四角图案按九宫格绘制、不会被拉变形，框厚随窗口
宽度等比缩放；外轮廓可以不规则，透明处直接透出桌面（本期透明处不穿透，
和框体一样可拖动）。配置只记相框图路径、不复制文件；文件被删、被移动或
不符合要求时，自动回退内置胡桃木画框，设置窗会在路径旁写明原因，修好图
后切回自定义即恢复。

## 第一次使用

首次启动（`%APPDATA%\MoeFrame\config.toml` 不存在或没有图库路径）会弹出系统文件夹对话框，
选一个图片文件夹即可。之后可以随时在设置窗里改。

## 操作

| 操作 | 结果 |
| --- | --- |
| 拖动相框内容区 | 移动窗口 |
| 拖八条边/四个角 | 等比缩放（窗口比例始终等于当前图片比例） |
| 键盘 ← / → | 上一张 / 下一张 |
| 右键 | 相框菜单：上一张、下一张、播放/暂停、重新扫描、选择文件夹、打开当前原图、复制图片路径、设置、退出 |
| 双击 | 用系统默认程序打开当前原图 |
| 单击、滚轮 | 不切图（避免误触） |
| 托盘左键 | 显示 / 隐藏相框 |
| 托盘右键 | 托盘菜单：显示/隐藏、上一张、下一张、播放/暂停、设置、退出 |

图片始终完整显示，不裁切也不留黑边：窗口高度由你决定，宽度随当前图片比例变化。
切图带切换动画（设置窗播放组可选，见下「设置」）：卷页、百叶帘、墨晕、抽卡、
像素故障、淡化六种，或随机抽一种，也可以选「无」回到瞬时切换。

## 轮播

顺序模式按自然序循环（连续数字按数值比较，其余按 Unicode 码点）；随机模式按洗牌袋
抽完一轮再重抽，本轮内不重复。已看过的图会记进历史，回到最新图后继续未看的；
关掉程序再打开，会恢复上次显示的图和本轮进度。
拖住边或角缩放时，到期的自动切换等松手再执行；不会累积多次切换。

- 关闭窗口（右上 × 或 Alt+F4）= 隐藏到托盘，轮播继续。
- 托盘或菜单里的「退出」= 结束进程，同时按当前状态写回配置。

## 设置

设置窗是独立窗口，改动先存草稿，点「保存」才生效；直接关掉设置窗就丢弃草稿。
间隔有 10 秒、30 秒、1 分钟、5 分钟、10 分钟、30 分钟六档，也可在自定义里填
3 到 99999999 之间的整数秒；
填同一个值不会被记成新改动。

切换动画（`playback.animation`）已生效：设置窗播放组里八档可选
（无 / 卷页 / 百叶帘 / 墨晕 / 抽卡 / 像素故障 / 淡化 / 随机），默认卷页，
点选即时生效并落盘；旧值 `slide` 打开时自动落到卷页，`fade` 保留为淡化，
`none` 保留为无。

两个配置键目前保留但尚未生效：悬停暂停（`playback.pause_on_hover`）、
显示文件名（`display.show_filename`）。

## 配置与日志

- 配置：`%APPDATA%\MoeFrame\config.toml`（程序自动维护，也可以手改，重启生效）
- 日志：`%APPDATA%\MoeFrame\moe_frame.log`（跳过坏图时会记下路径和原因）
- 开机自启：设置窗里勾选，写入当前用户的 Run 键；写入失败会回滚勾选状态

## 构建与检查

工具链版本与组件由 `rust-toolchain.toml` 固定（含 clippy、rustfmt、rust-analyzer）。

```powershell
cargo build --release --bin MoeFrame --locked
.\target\release\MoeFrame.exe
```
发布用的 `MoeFrame.exe` 在 Windows 上用 MSVC 按同一命令构建，验收后手动上传到 Releases。

`scripts/check.sh` 是唯一检查入口，CI 跑同一套：

```bash
bash scripts/check.sh   # cargo fmt --check + clippy -D warnings + test --all-targets
```

## 验证工具与文档

- `cargo run --release --bin window-validation`：08 的窗口验证程序（`F9` 快照、`F10` 最大化、`Esc` 退出），
  清单见 [窗口验证清单](docs/window-validation.md)。
- `cargo run --release --bin decode-validation -- --help`：09 的独立解码与内存验证器，共用产品解码，见 [复测手册](docs/decode-validation.md)。
- 发布资源测量：`scripts/measure-release.ps1`，历史结果见 [发布测量记录](docs/release-measurements.md)。
- 规格与实施票：[.scratch/v1-exe/spec.md](.scratch/v1-exe/spec.md)、[最终验收票](.scratch/v1-exe/issues/23-release-acceptance.md)
- 架构决定：[docs/adr/](docs/adr/)；工程笔记：[.agents/notes/](.agents/notes/)

## 目录

```text
src/lib.rs                      egui 输入 → 视口命令（拖动、八方向缩放、最大化守卫）
src/app_core.rs、playlist.rs    轮播状态机与历史
src/config.rs、config_sync.rs   配置读写与跨进程同步
src/library.rs、loading.rs      图库扫描与解码
src/display.rs、display_data.rs 显示数据与共享帧
src/transition_ambient.rs       切换动画纯计算（fit/进度/曲线与百叶帘、墨晕、像素）
src/window_geom.rs              窗口位置/尺寸与 DPI 恢复
src/diagnostics.rs              真实文件日志与窗口观测
src/bin/moe-frame.rs            正式相框入口：main、日志、配置装载
src/bin/moe-frame/              宿主分模块：app、frame_ui、settings_ui、input、geometry、
                                tray、texture、autostart_store、ui_regressions
src/bin/window-validation/      08 验证程序与 Win32 观测
src/bin/decode-validation/      09 解码与内存验证程序
tests/                          输入、配置、轮播与日志的自动测试
scripts/check.sh                唯一检查入口
docs/adr/                       已接受的架构决定
.scratch/v1-exe/                V1 规格、路线图与本轮实施票
.agents/notes/                  工程决定笔记
```