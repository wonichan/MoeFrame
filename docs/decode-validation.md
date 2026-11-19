# 独立图片解码与内存验证

> 工具已实现，内存验收未通过。当前仅有空载实测；图片 GUI 和一小时长测尚未完成，实验 Limits 与峰值上限未经用户确认。

对应[工单 09](../.scratch/v1-exe/issues/09-decode-memory-validation.md)。`decode-validation` 与窗口验证器独立，不是正式相框，不扫描目录、不落盘产品状态，也不实现托盘生命周期。这里的 Close 结束验证程序，不改变正式产品“关闭＝隐藏”的语义。

## 验证边界

- 使用已锁定的 `image=0.25.10`、`eframe/egui=0.37.0`，支持 PNG、JPEG、WebP、BMP。BMP 使用 `image` 自带解码器，不虚构 `bmp-rs` 的依赖版本。
- 窗口客户端为 480×320 逻辑像素，禁止调整尺寸/最大化。加载目标取真实窗口物理客户端尺寸；本机 DPI 1.5 时为 720×480，不把逻辑尺寸直接当成纹理尺寸。
- Fit 保持比例，Fill 按覆盖尺寸缩放后居中裁切。完整解码像素短时存在；返回显示数据前释放原图。长期只保存当前和下一张的小尺寸 CPU 显示数据与纹理引用。
- 一个后台线程通过有界通信加载；下一张准备后才可切换。旧纹理释放请求、CPU 持有释放、绘制提交、稳定采样分开记录，不将任一日志事件等同于 GPU 释放或真实显示。
- 三个 Limits 必须由实验命令显式指定。它们不是产品默认值，不是整个进程的硬内存沙箱。损坏解码器的 unwinding panic 会变成带路径的拒绝；abort/OOM 不可捕获，`panic=abort` 下拒绝图片加载。

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

下面的 12000×12000、96 MiB **只是实验输入示例，未经批准为产品安全线**。每组使用独立进程与日志目录。图片组的时长从首次准备好的显示数据开始；全坏清单没有首次显示，需人工 Close。

空载与可选字体诊断：

```powershell
& .\target\release\decode-validation.exe --baseline --duration-secs 90 --log-dir target/decode-memory-validation/baseline-new
& .\target\release\decode-validation.exe --baseline --no-system-font-fallback --duration-secs 90 --log-dir target/decode-memory-validation/font-off-new
```

先独立确认普通图片四格式能显示，再检查混合错误；Fit 与 Fill 分组：

```powershell
& .\target\release\decode-validation.exe --list "$cases/ordinary.txt" --mode fit --max-width 12000 --max-height 12000 --max-alloc-mib 96 --interval-ms 5000 --sample-ms 25 --duration-secs 90 --log-dir target/decode-memory-validation/ordinary-fit-new
& .\target\release\decode-validation.exe --list "$cases/mixed.txt" --mode fill --max-width 12000 --max-height 12000 --max-alloc-mib 96 --interval-ms 5000 --sample-ms 25 --duration-secs 90 --log-dir target/decode-memory-validation/mixed-fill-new
```

确认普通图片没有大量拒绝、坏图之后仍显示有效图片，再运行一小时混合轮播：

```powershell
& .\target\release\decode-validation.exe --list "$cases/mixed.txt" --mode fill --max-width 12000 --max-height 12000 --max-alloc-mib 96 --interval-ms 5000 --sample-ms 25 --duration-secs 3600 --log-dir target/decode-memory-validation/hour-fill-new
```

上述图片组和一小时命令尚未作为本票实机证据完成。原生桌面驱动当前要求用户确认启动；应通过其可信授权提示批准，或由操作人执行复测并提供日志/截图。代理不得改权限模式或换工具绕过拒绝。

## 观察什么

1. 记录逻辑/物理客户端尺寸、DPI、可见及最小化状态。截图确认普通四格式确实绘制、Fit/Fill 正常、拒绝后有效图片继续；`draw_submitted` 只证明代码提交绘制，不代替截图。
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

## 已有证据与缺口

当前证据都是**没有图片、没有预加载**的空载，不能替代普通 4K 与一小时验收：

| 组 | 时长 | 稳定工作集最小/均值/最大（字节） | 原生生命周期峰值（字节） | 结论 |
| --- | --- | --- | --- | --- |
| 默认系统字体回退 | 60 秒 | 138240000 / 142154140.63 / 142499840 | 142499840 | 未通过 50 MB |
| 显式关闭系统字体回退 | 90 秒 | 73617408 / 79328327.81 / 80019456 | 80162816 | 未通过 50 MB |

两次记录不是同一二进制下交错重复的严格 A/B；关闭字体组仍超过 50 MB，但不能把两组差值全部归因于字体。默认字体回退未改变，可选关闭会影响字符覆盖。

本机环境：Windows NT 10.0.26200 x64，480×320 逻辑/720×480 物理客户端，DPI 1.5；关闭字体组 GL 记录 `NVIDIA GeForce RTX 4070 Ti SUPER/PCIe/SSE2`、`3.3.0 NVIDIA 616.64`、最大纹理边 32768。字体图集为 16384×32、理论 RGBA 字节 2097152，非整个字体或进程内存。

既有记录的工具链为 rustc/cargo 1.99.1，image 0.25.10、eframe/egui/epaint/egui_glow 0.37.0、winit 0.30.13、glutin 0.32.3、glow 0.17.0、png 0.18.1、image-webp 0.2.4、zune-core 0.5.3、zune-jpeg 0.5.15、windows-sys 直接依赖 0.61.2。完整编译信息由日志 `WV_*` 头保留；这些空载记录来自任务起点 `5f143fe26125d30f8490596de03db513847c5cc2` 的 dirty 工作树、格式化前 release 二进制，不宣称等同最终提交。

本机未入 Git 的证据路径：

- `target/decode-memory-validation/baseline/validation-1795018044594454100-11420-0.log`
- `target/decode-memory-validation/baseline-font-off-1/validation-1795020024714806500-20268-0.log`
- `target/decode-memory-validation/baseline-font-off-1.png`
- `target/decode-memory-validation/samples-20261119/samples.tsv`

`cargo fmt --all -- --check`、`cargo check --all-targets`、`cargo test --all-targets`、`cargo test --doc` 全部通过：38 项测试，包含本票新增的 15 项真实加载/轮播与 4 项 CLI 测试；三项笔记门禁通过。**待完成**：普通与混合图片实机显示、一小时趋势、Limits 与峰值两项用户反馈。50 MB 门禁保持失败，工单保持未完成；数据仍不达标时应先查持有与回收时点，再基于证据讨论备选验证票。

## 关键依据

- [Windows 进程内存计数器](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
- [GetProcessMemoryInfo 示例与口径](https://learn.microsoft.com/en-us/windows/win32/psapi/collecting-memory-usage-information-for-a-process)
- [已锁定 eframe 0.37.0 NativeOptions](https://docs.rs/eframe/0.37.0/eframe/struct.NativeOptions.html)
- [已锁定 png 0.18.1 调色板源码](https://docs.rs/crate/png/0.18.1/source/src/decoder/transform/palette.rs)
- [前序解码路线研究](../.scratch/v1-exe/research/downsample-decode.md)
