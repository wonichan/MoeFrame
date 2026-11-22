# 发布资源测量记录（票 22）

本文件留存 V1 发布预算的实测证据，供复测与复核。测量日期 2026-11-22，对象是 `cargo build --release --bin MoeFrame --locked` 的交付构建（strip + LTO，静态 CRT）。

## 环境与构建

| 项 | 值 |
|---|---|
| 系统 | Microsoft Windows 11 专业版 10.0.26200 |
| CPU | 12th Gen Intel Core i5-12600KF，16 逻辑核 |
| 内存 | 47.9 GB |
| 显示器 | 2560×1440，系统 DPI 96（100%），单屏 |
| exe | 8.02 MB（`target/release/MoeFrame.exe`） |
| 预算 | 稳定工作集 ≤80 MB；峰值工作集 ≤192 MB；单核 CPU <1%；冷启动 <1s |

## 测量方法

`scripts/measure-release.ps1` 用独立临时 `APPDATA` 启动真实程序，不碰用户配置与日志：

- 冷启动：从进程 `StartTime` 到日志 `event=first-image`（`phase=texture-upload`）的时间差，包含目录扫描与解码。
- 稳定工作集：首图后 5s 起 30s 内 `Process.WorkingSet64` 的中段均值（去掉首尾，避免首图瞬间与退出抖动）。
- 峰值：`Get-Process` 采样 `PeakWorkingSet64`，覆盖至少一轮含 100MP 样本的完整轮播。
- CPU：同窗口内 `TotalProcessorTime` 增量 ÷ 墙钟时间，按单核口径百分比。

复现（会重建样例图，写 `target/release-measure/`）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-release.ps1 `
  -GenerateSamples -WithTenThousand -SoakMinutes 3 -SlideSecs 3 `
  -OutDir target\release-measure\gate
```

## 结果

| 场景 | 冷启动到首图 | 稳定工作集 | 峰值工作集 | 平均单核 CPU |
|---|---|---|---|---|
| 样例图库静止显示（不切换） | 281 ms | 76.15 MB（76.09–76.31） | 79.35 MB | <0.01% |
| 一万张图库（只枚举路径） | 275 ms | 72.86 MB | 76.89 MB | 0.05% |
| 13 张样例连续轮播 3 分钟（3s 一张，含 100MP） | 257 ms | 76.68 MB（69.14–87.40） | 130.82 MB | 0.11% |

- 冷启动三次独立运行 275–303 ms，均低于 1s。
- 稳定工作集与图库规模、图片尺寸无关（同样 13 张样例 76 MB、一万张 73 MB），说明图库侧只持有路径。
- 峰值 130.82 MB 出现在 100MP 样本首次全分辨率解码，低于 192 MB 预算，属正常峰值而非例外。
- 一小时连续轮播由用户在本机以任务管理器读数与持续观察核对：工作集无持续增长，通过（未在本文件内重跑）。

## 显示方式

窗口创建后按当前图片比例锁定内尺寸（日志 `event=inner_size_command` 请求 `requested=[568.8889, 320.0]` 等），纹理按窗口物理像素等比缩放后 1:1 铺满，不拉伸不变形。

用 1920×1080、2560×1440、3840×2160 三种尺寸各跑一次：均出现 `event=first-image`、`skipped=0`、进程未退出；窗口内尺寸分别锁到对应比例。

## 只保留当前图与下一张

宿主只持有两个纹理句柄：`texture`（当前）与 `incoming`（下一张，见 `src/bin/moe-frame/app.rs`）。`load_first_available` 返回后立即 `drop` 全分辨率 `DynamicImage`，只留缩放后的显示缓冲。实测侧证：13 张样例轮播 3 分钟、工作集 69–87 MB，与图数、像素数无关。