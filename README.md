# MoeFrame

Windows 10/11 x64 的无边框相框程序（Rust + eframe/egui）。V1 规格见
[.scratch/v1-exe/spec.md](.scratch/v1-exe/spec.md)；票 10–21 的功能已实现：
图库扫描与自然排序、顺序/随机轮播（历史不重复）、续播、转场、托盘常驻与左键切换、
设置窗（路径与秒数草稿、校验）、窗口位置与 DPI 恢复、开机自启、点击/双击/拖动分界。

发布验收尚未完成：[22 资源达标](.scratch/v1-exe/issues/22-release-resource-budgets.md)
与 [23 端到端验收](.scratch/v1-exe/issues/23-release-acceptance.md) 仍在待办，
当前产物不是交付版本。

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

## 构建与运行

目标为 Windows 10/11 x64；工具链版本与组件由 `rust-toolchain.toml` 固定（含 clippy、rustfmt、rust-analyzer）。

```powershell
cargo build --release --bin MoeFrame --locked
.\target\release\MoeFrame.exe
```

配置在 `%APPDATA%\MoeFrame\config.toml`，日志在 `%APPDATA%\MoeFrame\moe_frame.log`。

## 检查

`scripts/check.sh` 是唯一检查入口，CI 跑同一套：

```bash
bash scripts/check.sh   # cargo fmt --check + clippy -D warnings + test --all-targets
```

## 运行验证工具

08 的窗口验证程序仍在：

```powershell
cargo build --release --bin window-validation --locked
.\target\release\window-validation.exe
```

- 内容区拖动移动，四边/四角缩放，最小 120×120 逻辑尺寸。
- `F9` 记快照，`F10` 最大化/还原，`Esc` 退出。
- 日志在 `%APPDATA%\MoeFrame\window-validation\`，每次启动独立保存。

操作清单与实测记录见 [窗口验证清单](docs/window-validation.md)。用户已在本机（Windows 11 x64 Build 26200、150% 缩放）操作确认通过；Windows 10、Build 26100、100% 缩放与混合 DPI 双屏仍未验证。编译、日志或截图都不代表人工验收。

## 目录

```text
src/lib.rs                      egui 输入 → 视口命令（拖动、八方向缩放、最大化守卫）
src/app_core.rs、playlist.rs    轮播状态机与历史
src/config.rs、config_sync.rs   配置读写与跨进程同步
src/library.rs、loading.rs      图库扫描与解码
src/display.rs、display_data.rs 转场显示与共享帧数据
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