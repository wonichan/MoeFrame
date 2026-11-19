# Agent Note: 解码内存验证不把分配限额冒充进程内存上限

Status: implemented

## Problem

[工单 09](../../../../.scratch/v1-exe/issues/09-decode-memory-validation.md) 需要同时验证小窗口显示、预加载、坏图后继续和进程内存。上一阶段研究只能说明 API 路径，不能证明 GUI 上传后的驻留或一小时趋势；`image::Limits::max_alloc` 也不是整个进程的硬上限。若把库默认值或单测结果当成产品安全线，正式相框将继承未经测量的承诺。

## Decision

- 单独提供 `decode-validation`，不并入工单 08 的窗口验证器，不实现正式相框、扫描或持久化。使用已锁定的 `image=0.25.10` 和 `eframe=0.37.0`。
- 真实文件加载入口接收窗口物理像素、适应/填充模式及三个显式实验 Limits；不提供产品默认 Limits。完整解码与缩放依次执行，原尺寸像素在显示数据返回前释放。
- 单个后台工作线程通过有界通信预加载，长期只有当前和下一张的小尺寸显示数据；真实轮播入口可观察路径、拒绝和释放，不为测试暴露私有调度。两处接缝已由用户确认。
- Windows `GetProcessMemoryInfo` 记录当前工作集及进程峰值；逐阶段记录是观测，不自动写通过结论。分别测空载基线、稳定驻留和切换峰值，不主动修剪工作集来满足 50MB。实机长测、Limits 与峰值建议另作验收证据。

## Alternatives considered

- **UI 线程同步解码** — 实现与生命周期最直接；但大图解码会挡住绘制和退出，无法证明坏图后轮播仍响应，因此采用单工作线程，不增加并发解码。
- **整张 RGBA 上传后由 GPU 缩放** — 显示链短，省去 CPU 缩放；但长期 GPU/CPU 数据仍与原图尺寸相关，不能验证“小窗口只持有显示所需数据”的要求，因此先生成物理像素匹配的数据再上传。

## Testing

- `cargo fmt --all -- --check`、`cargo check --all-targets`、`cargo test --all-targets` 和 `cargo test --doc` 通过，全量 38 项测试包含本票新增 19 项。真实损坏 PLTE 回归先触发 `png=0.18.1` 的 panic，再验证其成为带路径的拒绝且轮播继续；只包住依赖解码调用，不吞宿主观察回调的错误。三项笔记校验也通过。
- 样本生成程序产出 13 条普通路径、19 条混合路径，涵盖四种格式、1080p/2K/4K、有效 100MP、极端长宽比、截断、伪装和损坏 PLTE。样本由独立进程生成，不把生成过程内存记成显示内存。
- 空载 60 秒默认字体回退组的稳定工作集为 138.2–142.5 MB；空载 90 秒显式关闭字体回退组为 73.6–80.0 MB，均未通过 50 MB。两个组并非同一二进制下交错重复的严格 A/B，不把差值全部归因于字体。图像显示字节为零也不能说明字体或驱动不占内存。
- [复测说明](../../../../docs/decode-validation.md) 区分真实显示、稳定采样、进程生命周期峰值和资源释放请求。双轴审查已完成；图片 GUI、一小时趋势与两项用户确认仍未完成，原生启动审批是当前实测阻塞，不绕过原生确认。审查遗留一处待改行为：首次配置同步准备当前与下一张（`src/decode_validation.rs` 的 `ValidationCycle` 准备流程），与 `spec.md`“当前图显示后预加载下一张”的起始资源路径不一致，需改为先显示当前图再请求下一张。

## Consequences

- 收益是加载与轮播接缝可独立复测，原图不进入长期显示持有集，损坏文件的 unwinding panic 不再直接中断验证程序；验证日志保留失败而不是自动判通过。
- 代价是仍有短时全分辨率解码，以及后台线程、当前/下一张 CPU 显示数据和 GUI 上传副本。库 Limits 是尽力约束，不能约束整个进程、所有解码器内部临时分配、abort 或 OOM；系统进程峰值也不是 GPU 内存或单次解码峰值。
- 可选 `--no-system-font-fallback` 只帮助诊断，会改变字体覆盖；默认仍启用，不作产品选择。空载已超过 50 MB，因此仅收紧图片 Limits 不能据此宣布普通图片达标，不通过修剪工作集或扣除空载基线改口径。
- 实机长测与两项用户确认仍是工单 09 的完成门：不采纳实验 Limits 或峰值作为正式安全线，不将工单标完成。若统一口径实测仍失败，保留证据再讨论备选验证票，不在本票擅自切换框架或解码器。

## 2026-11-19 后续：预加载顺序修复（审查遗留 P2）

- 接缝变更：`ValidationCycle::prime` 拆为 `prime_current`（只准备并发布当前图，不触碰下一张）与 `preload_next`（当前图发布后才准备下一张）；`advance` 只做退役与提升并立即发布，不再同步解码新的下一张。旧的合并式 `prime` 已删除，宿主无法再走“当前图等下一张”的起始路径。
- 宿主接线：worker 新增 `Command::PreloadNext`，`Ready` 以 `ReadyKind::{Current, Preload, Advanced}` 区分三段回包；UI 在 Current/Advanced 回包时先上传/确认当前图纹理，仅登记待发预加载；请求在本帧 `draw_submitted` 记录之后才真正发出（双轴审查 Spec 轴指出初版在同一帧内、绘制提交记录之前就发了请求，已按此修正），Preload 回包只上传下一张纹理。`display_rgba_bytes` 因此分两段更新（当前图一段、预加载完成后一段），稳定采样仍以 `pending` 结束与 QUIESCENT 阶段为准，口径未放宽。
- 测试先行：在真实轮播接缝先加两项失败测试（`cycle_publishes_current_before_touching_the_next_image`、`advance_promotes_without_decoding_the_new_next_until_preload`，先以“方法不存在”形式确认 RED），再实现到 GREEN；原有 5 处使用 `prime` 的测试迁移到两段式调用，行为断言不变或更严（advance 后 `next` 必须为空、提升不重复解码）。
- 验证：`cargo fmt --all -- --check` 通过；Windows 目标 `cargo check --all-targets` 与 `cargo clippy --all-targets` 通过（clippy 仅有 observation.rs / native.rs 既有的 FFI 结构体初始化警告，本次未新增）。测试执行在 Linux 副本上完成（副本仅为可执行测试而给 winit/eframe 启用 wayland/x11 特性，未提交）：全量 40 项通过（解码 17、CLI 4、诊断 6、窗口 13），`cargo test --doc` 通过。Windows 实机 GUI 复测仍未执行，本项不改变内存门结论：空载 50 MB 未通过、图像组与一小时趋势无数据、Limits 与峰值上限仍待用户确认，工单 09 不标完成。

## 2026-11-19 用户口径：稳定工作集 <100 MB 可接受

- 用户原话：50MB 也并非严格，只要是 100MB 以下都能接受。据此本票稳定工作集评估线改为 <100 MB（空载关字体回退组 73.6–80.0 MB 已在该线内，但图像组与一小时趋势仍无数据，且默认字体回退组 138.2–142.5 MB 仍在线外）。Limits 与峰值上限的确认程序不变：先实测、后确认，不拿本次放宽当作已确认阈值。

## 2026-11-19 内存诊断（进行中）：差值构成假设与单字体实验臂

按 diagnosing-bugs 流程推进。反馈环是实机差分复测（同一二进制、基线模式、独立日志目录），只能由用户在 Windows 上执行；已有的 font-off 组（73.6–80.0 MB）与默认组（138.2–142.5 MB）构成现成差分对。

源码级事实（eframe/epaint/egui_system_font_provider 0.37 本地源码核对）：

- 系统字体回退不是启动时全量加载字体文件。eframe 只注册一个 `SystemFontProvider`；它在后台线程用 fontique 枚举全部系统字体（DirectWrite），字体文件以 mmap 方式访问、按缺失字形逐簇查询，命中的字体以 `FontInsert` 插入并由 epaint 长期保留。
- 关字体组的字体图集实测仅 16384×32（理论 RGBA 2 MB），图集不是约 60 MB 差值的主体。

排序假设（均可证伪）：

1. **枚举/数据库驻留**：fontique 的 Collection 枚举触及数百个字体面的表数据，CJK 系统字体多且大，被触及的 mmap 页与数据库元数据计入工作集。预测：默认组日志 `font_state` 的 `discovered_fallback_fonts` 为空或极少，而工作集仍约 140 MB；单字体组 ≈ 关字体组 + 该文件被触及部分。
2. **插入字体保留**：实际渲染触发回退的字形簇把整份字体文件数据插入并保留（微软雅黑 TTC 约 19 MB，多个脚本/家族可叠加）。预测：`discovered_fallback_fonts` 非空且与工作集增量同量级；单字体组 ≈ 关字体组 + 约一个字体文件大小，且中文覆盖正常。
3. **图集增长**：默认组图集显著大于关字体组。预测：两组 `font_state` 的 atlas_pixels 差值 ×4 字节能解释差值的主要部分（现有证据对关字体组不利此说，默认组图集尺寸待日志确认）。

为此给验证器新增诊断臂 `--font-file FILE`：强制关闭系统字体枚举，只把指定单个字体文件注册为唯一回退（Proportional/Monospace 最低优先级），日志记 `font_file_registered` 路径与字节数；CLI 新增 1 项测试，全量 41 项通过，Windows 目标 check 通过。复测命令已写入 `docs/decode-validation.md`。三组数据回来后再定产品字体路线；在此之前不下“差值就是字体”的结论（两组原记录并非同二进制交错 A/B）。
