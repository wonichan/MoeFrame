# 独立图片解码与内存验证

> 工具现状见[深挖工单 07 的实施笔记](../.agents/notes/implemented/architecture/2026-11-23-decode-merge.md)：`decode-validation` 通过薄 adapter 调用产品 `loading`，保留独立实验 Limits、命令行与内存观测。工单 09 的历史实测对应合流前的验证器；[V1 发布门记录](release-measurements.md)测的是完整产品，不是原验证器的复测。

对应[工单 09](../.scratch/v1-exe/issues/09-decode-memory-validation.md)。`decode-validation` 与窗口验证器独立，不是正式相框，不扫描目录、不落盘产品状态，也不实现托盘生命周期。这里的 Close 结束验证程序，不改变正式产品“关闭＝隐藏”的语义。

## 验证边界

- 与产品共用锁定的 `image=0.25.10`，支持 PNG、JPEG、WebP、BMP 与 GIF 首帧。验证 GUI 使用 `eframe/egui=0.37.0`。
- 窗口客户端为 480×320 逻辑像素，禁止调整尺寸/最大化。加载目标取真实窗口物理客户端尺寸；本机 DPI 1.5 时为 720×480，不把逻辑尺寸直接当成纹理尺寸。
- 适应完整显示，整数派生边向下取整且至少 1 像素；填充按覆盖尺寸向上取整并居中裁切。缩放后立即释放原图，长期只保存当前和下一张的小尺寸 CPU 显示数据与纹理引用。
- 一个后台线程通过有界通信加载；下一张准备后才可切换。旧纹理释放请求、CPU 持有释放、绘制提交、稳定采样分开记录，不将任一日志事件等同于 GPU 释放或真实显示。
- 三个 Limits 由实验命令显式指定，不读取产品默认值，也不是进程硬内存沙箱。有源图尺寸、解码输出和覆盖缓冲限制，没有编码文件大小预检。解码器的 unwinding panic 会变成带路径的拒绝；abort/OOM 不可捕获，`panic=abort` 下拒绝图片加载。

## 构建与样本

在项目根目录执行；生成目录必须不存在或为空，避免覆盖已有材料：

```powershell
cargo build --release --bin decode-validation --example generate_decode_validation_samples
$cases = "target/decode-memory-validation/cases-new"
cargo run --release --example generate_decode_validation_samples -- $cases
```

生成内容是确定性渐变、边框和中心标记，不包含私人照片：

| 清单 | 内容 |
| --- | --- |
| `ordinary.txt` | 1920×1080、2560×1440、3840×2160 的 PNG/JPEG/WebP/BMP，加 2048×1080 PNG，共 13 条 |
| `mixed.txt` | 普通图片与有效 10000×10000 PNG、10000×1 极端比例 PNG、截断、扩展名伪装、损坏 JPEG、损坏 PLTE 交错，共 19 条 |
| `large-and-valid.txt` | 有效 100MP 和有效普通图片，用于检查明确拒绝后继续 |
| `panic-and-valid.txt` | 可触发已锁定 `png=0.18.1` 依赖 panic 的真实文件与有效图片 |
| `all-bad.txt` | 没有可显示图片的错误路径，验证不崩溃、不忙等；人工关闭结束 |
| `samples.tsv` | 路径、宽高、编码字节数、样本类别 |

`--list` 读取 UTF-8 清单，每条相对路径以清单所在目录为基准。样本生成是独立进程，不能把它的峰值算作显示验证结果。复测时保留生成程序与验证器二进制摘要、完整依赖版本和清单；不要覆盖旧组。

## 复测命令

下面的 12000×12000、96 MiB 是命令显式传入的实验值，恰与当前产品默认值相同；可单独更改，不改变产品设置。每组使用独立进程与日志目录。图片组的时长从首次准备好的显示数据开始；全坏清单没有首次显示，需人工 Close。

空载与可选字体诊断：

```powershell
& .\target\release\decode-validation.exe --baseline --duration-secs 90 --log-dir target/decode-memory-validation/baseline-new
& .\target\release\decode-validation.exe --baseline --no-system-font-fallback --duration-secs 90 --log-dir target/decode-memory-validation/font-off-new
& .\target\release\decode-validation.exe --baseline --font-file C:\Windows\Fonts\msyh.ttc --duration-secs 90 --log-dir target/decode-memory-validation/font-single-new
```

第三组为单字体文件诊断：关闭系统字体枚举，只注册指定的一个字体文件作为回退（日志记 `font_file_registered` 的路径与字节数）。与默认组对比时同时看各组日志的 `font_state` 行：`discovered_fallback_fonts` 为空而工作集仍高，说明开销在系统字体枚举/数据库本身；非空则说明开销在实际插入并保留的回退字体。

先独立确认普通图片四格式能显示，再检查混合错误；适应与填充 分组：

```powershell
& .\target\release\decode-validation.exe --list "$cases/ordinary.txt" --mode fit --max-width 12000 --max-height 12000 --max-alloc-mib 96 --interval-ms 5000 --sample-ms 25 --duration-secs 90 --log-dir target/decode-memory-validation/ordinary-fit-new
& .\target\release\decode-validation.exe --list "$cases/mixed.txt" --mode fill --max-width 12000 --max-height 12000 --max-alloc-mib 96 --interval-ms 5000 --sample-ms 25 --duration-secs 90 --log-dir target/decode-memory-validation/mixed-fill-new
```

确认普通图片没有大量拒绝、坏图之后仍显示有效图片，再运行一小时混合轮播：

```powershell
& .\target\release\decode-validation.exe --list "$cases/mixed.txt" --mode fill --max-width 12000 --max-height 12000 --max-alloc-mib 96 --interval-ms 5000 --sample-ms 25 --duration-secs 3600 --log-dir target/decode-memory-validation/hour-fill-new
```

这些命令可用于复测合流后的验证器。实测要保留对应构建、日志和截图，不复用工单 09 或 V1 发布门的历史数值。

## 观察什么

1. 记录逻辑/物理客户端尺寸、DPI、可见及最小化状态。截图确认普通四格式确实绘制、适应/填充 正常、拒绝后有效图片继续；`draw_submitted` 只证明代码提交绘制，不代替截图。
2. 核对每条路径的源尺寸、编码大小、实验 Limits、完整像素/显示数据阶段和拒绝原因。100MP 可以明确拒绝，但不能崩溃；普通 4K 不能靠大量拒绝满足内存数字。
3. 解码、缩放和预加载完成后，UI 可见且未最小化、距准备完成至少一秒时才标 `quiescent`。这是统一采样标签，不是 GPU fence；不得使用最小化、关闭后的值或缺失样本作为稳定通过。
4. 报告原始字节与十进制 MB（1 MB＝1,000,000 字节）。同时保留采样最大工作集和系统进程生命周期峰值；定时采样会漏短峰，生命周期峰值包含启动/字体/渲染器，不是单次解码峰值。
5. 一小时按每分钟/后半段窗口报告稳定样本范围与均值、有效切换/拒绝计数，确认持续轮播而非空窗口等待。内存不持续增长必须来自真实工作集趋势，不由 `Arc`/`Weak` 测试、缓存清空或理论字节推出。
6. 结束后核对 `run_end`、失败信息、采样数量、CPU/纹理退场记录及原生运行结果。日志写入或采样失败不是“通过”。

Windows 指标来自 `GetProcessMemoryInfo`：

| 字段 | 含义 |
| --- | --- |
| `working_set_bytes` | 当前进程工作集 |
| `peak_working_set_bytes` | 系统记录的进程生命周期工作集峰值 |
| `private_bytes` | `PrivateUsage` 的私有提交量，不是私有工作集 |
| `page_fault_count` | 系统页错误计数 |
| `display_rgba_bytes` | 当前/下一张 CPU 显示数据的理论字节量，不是工作集 |
| `image_texture_metadata_bytes` / `texture_metadata_bytes` | 验证程序图像纹理的元数据量，不包含实际 GPU 分配或字体内存 |
| `theoretical_atlas_rgba_bytes` | 实测字体图集尺寸乘以四，不是该图集的实际驻留量 |

不调用 `EmptyWorkingSet`、不扣除空载基线、不把 GPU/CPU 元数据或均值当作瞬时峰值，不主动改变框架或解码路线。

## 历史实测

- [工单 09 的实施笔记](../.agents/notes/implemented/testing/2026-11-18-decode-memory-validation.md)记录合流前验证器的三组空载与四组 90 秒图片实验，含构建 revision、二进制摘要、样本和逐组字节数。普通适应与混合填充在默认字体回退下的稳定工作集最大值都低于当时的 100 MB 验收线；工单 09 没跑一小时轮播，也不确认产品 Limits 或峰值预算。
- [V1 票 22/23](release-measurements.md)验的是完整产品：一小时轮播无持续增长，峰值 130.82 MB（确认后的预算 ≤192 MB）。这些记录不是合流后验证器的资源测量。

## 关键依据

- [Windows 进程内存计数器](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
- [GetProcessMemoryInfo 示例与口径](https://learn.microsoft.com/en-us/windows/win32/psapi/collecting-memory-usage-information-for-a-process)
- [已锁定 eframe 0.37.0 NativeOptions](https://docs.rs/eframe/0.37.0/eframe/struct.NativeOptions.html)
- [已锁定 png 0.18.1 调色板源码](https://docs.rs/crate/png/0.18.1/source/src/decoder/transform/palette.rs)
- [前序解码路线研究](../.scratch/v1-exe/research/downsample-decode.md)
