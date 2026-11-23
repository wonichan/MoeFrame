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


## 2026-11-20 实机复测尝试：GUI 启动受阻

- 本地 `main` 同步至 `77eb9ff`；`cargo build --release --bin decode-validation` 在 release 配置下成功（3 分 18 秒），没有产品代码改动。
- 旧默认回退日志 `target/decode-memory-validation/baseline/validation-1795018044594454100-11420-0.log` 不含 `font_state` 行，因此无法从旧记录摘取 `discovered_fallback_fonts` 或 `atlas_pixels`，假设 1/2 仍未区分。原 60 秒组稳定工作集为 138240000 / 142154140.63 / 142499840 字节，生命周期峰值 142499840 字节；旧记录来自不同提交/脏工作树，不等价于本轮同二进制 A/B。
- 通过桌面驱动按原会话与默认后台方式启动 77eb9ff release 基线两次，均返回 `Launched (pid 0) in background`。随后窗口/进程观察未发现验证器或原生确认窗口，`target/decode-memory-validation/baseline-77eb9ff` 未创建；桌面截图接口两次返回 `The execution outcome is unknown`。因此新实测没有开始，不用 shell 启动，不更改会话标签或 delivery 模式。
- 默认、关系统字体、单字体三组新基线及 ordinary/fit、mixed/fill 的四个图像组均未跑；中文文件名覆盖截图未取得。Limits、工作集与峰值无新数据；一小时轮播按本轮计划不跑。工单保持未完成，等待桌面驱动能正常启动 GUI 后续测。


## 2026-11-20 后续实机复测：同二进制字体与图像组

- 构建与口径：本轮未改产品代码；Windows release `decode-validation.exe` 来自 `77eb9ff44ae94c0936cb463fca747719b496926d`（dirty=false），SHA-256 `A14D8D751291C78C4C6474422C52074F30EC32C0EF3E271061AE84DEE8F05DD0`。三组基线和四组图片均使用该二进制；基线各 90 秒，图片各 90 秒、25 ms 采样、5 秒轮播。图片组统一使用实验输入 12000×12000 / 96 MiB（100663296 字节），未经批准，不代表产品 Limits。未修剪工作集、未扣除空载基线。原始日志均保留在忽略目录 `target/decode-memory-validation/`。
- 默认系统字体回退基线：稳定工作集 min/mean/max `69853184 / 73965951 / 74399744 B`，生命周期峰值 `74461184 B`；`font_state` 为 `atlas_pixels=[16384, 32]`、`discovered_fallback_fonts=[]`；`run_end prepared=0; rejected=0; failure=None`。日志：`baseline-manual-77eb9ff/validation-1795178353688206700-11436-0.log`。
- 关闭系统字体回退基线：`68792320 / 72910201 / 73351168 B`，峰值 `73420800 B`；`font_state` 同为 `[16384, 32]`、空 fallback 列表；`run_end prepared=0; rejected=0; failure=None`。日志：`font-off-manual-77eb9ff/validation-1795178723695772300-18088-0.log`。
- 单字体基线：`88711168 / 92820817 / 93245440 B`，峰值 `93315072 B`；`font_file_registered` 记录 `C:\Windows\Fonts\msyh.ttc`、`19704352` 字节，`font_state` 仍为 `[16384, 32]`、空 system-fallback 列表；`run_end prepared=0; rejected=0; failure=None`。日志：`font-single-manual-77eb9ff/validation-1795178835431071100-21424-0.log`。
- 字体差分：同二进制下默认回退比关闭回退的 mean 高约 `1.06 MB`；显式注册微软雅黑比关闭回退高约 `19.91 MB`，与 TTC 文件 `19.70 MB` 接近。当前默认组未发现插入的 system fallback 字体；图集尺寸三组相同。旧默认组日志没有 `font_state`，无法补出旧值；旧组稳定工作集 `138240000 / 142154140.63 / 142499840 B`，与本轮结果差异明显且并非同二进制，故不能据此把旧约 60 MB 差额归因到枚举或字体保留。
- 图片组（格式为 stable min/mean/max B；peak B）：
  - 普通 Fit / 默认回退：`80289792 / 84571155 / 86233088`；峰值 `143040512`；`prepared=19; rejected=0; draw_submissions=18; failure=None`。日志目录 `ordinary-fit-default-manual-77eb9ff/`，首张 `draw_submitted` 为 1080p PNG / Fit。
  - 普通 Fit / 单字体：`99024896 / 103489475 / 105127936`；峰值 `160964608`；`prepared=19; rejected=0; draw_submissions=18; failure=None`。首张中文名 PNG 的 `draw_submitted` 成功；同组 `font_file_registered` 为 `19704352` 字节。日志目录 `ordinary-fit-single-font-manual-77eb9ff/`。
  - Mixed Fill / 默认回退：`80797696 / 85595382 / 86978560`；峰值 `142524416`；`prepared=19; rejected=12; draw_submissions=18; failure=None`。日志目录 `mixed-fill-default-manual-77eb9ff/`。
  - Mixed Fill / 单字体：`99639296 / 104123833 / 104910848`；峰值 `161382400`；`prepared=19; rejected=12; draw_submissions=18; failure=None`。日志目录 `mixed-fill-single-font-manual-77eb9ff/`。
- 两个 mixed 组各有 6 类样本重复拒绝两次：损坏 PLTE（stderr 的 png 0.18.1 panic 文本为 `range end index 3 out of range for slice of length 1`，日志记为带路径拒绝）、100MP（`Memory limit exceeded`）、截断、极端长宽比、坏图片头、扩展名/头不匹配；进程持续显示后续图并以 `failure=None` 结束。普通组均 `rejected=0`。不改动已知 `load_display` 编码大小/max_alloc 误拒风险或 Standards 轴命名问题；本次普通组未复现前一风险。
- 中文覆盖：单字体 Fit 的截图 `target/decode-memory-validation/chinese-coverage-single-font-retry-180s-77eb9ff/chinese-label.png` 显示标签 `中文样本-1080p.png` 为正常中文字形、非方框；图片 1920×1080 → 720×405。截图覆盖检查窗口当时显示 `Ws 103.8 MB; process peak 109.9 MB; prepared 15; rejected 0`，这是单图显示检查的实时覆盖，不并入四组 90 秒汇总。
- 当前结论与门槛：本轮默认回退两组图片的稳定工作集最大值均低于 100 MB；单字体两组的 mean/max 超过 100 MB，单字体虽显示中文正常，但内存代价与 TTC 体量接近。该结果支持继续评估默认字体路线，但不能替代一小时轮播，也不确认产品 Limits 或生命周期峰值上限；本轮按计划未跑一小时，工单 09 不完成，等待用户确认阈值和后续字体路线。

## 2026-11-20 用户决定：工单 09 验收范围调整

- 工单 09 不执行一小时轮播，也不提出或确认产品 Limits 与峰值上限；稳定工作集 <100 MB 口径和其他验收项保留。细节见[工单 09 验收范围调整](2026-11-20-issue-09-acceptance-scope.md)。
- 该范围调整仅适用于工单 09；V1 正式图片加载前的项目级 Limits / 峰值门仍按 [V1 终点决策](../process/2026-11-16-v1-destination-exe.md) 保留。本票的 90 秒测量不代表一小时趋势或已批准的产品阈值。

## 2026-11-20 收尾：工单 09 关闭与字体路线结论

- 用户指示由 Muse 侧收尾：工单 09 按调整后范围（不含一小时轮播、不含产品 Limits / 峰值确认）逐项核对验收，九项均勾选并对证据限定作括注，状态转 `resolved`。
- 字体路线结论（由本票同二进制差分支撑）：正式实现采用默认系统字体回退路线。默认回退相对关闭回退的稳定工作集差仅约 1.06 MB（mean），基线中 `discovered_fallback_fonts` 为空、图集尺寸三组相同，原假设 1（枚举驻留）与假设 3（图集增长）证伪；单字体文件注册路线的 +约 19.91 MB 与 TTC 文件体量接近（`FontData::from_owned` 整份驻留），图片组稳定工作集超过 100 MB，不采用为产品路线，其 `--font-file` 诊断臂保留在验证器中备查。假设 2 的机制仅对显式注册成立，不适用于默认回退路径。
- 旧默认组 138240000 / 142154140.63 / 142499840 B 记录来自不同提交与脏工作树，与本轮同二进制结果不可比，不再作为内存差值的归因依据；其成因未追查，也不影响本票结论。
- 移交：(1) 一小时趋势与产品 Limits / 峰值上限仍为 V1 正式图片加载前的项目级门（见范围调整笔记与 V1 终点决策）；(2) `load_display` 编码大小 / max_alloc 误拒风险与 Standards 轴命名问题保持已知未修，后续另票处理。

> 本文实测保留为合流前证据；现行验证接缝与编码预检处置见[深挖工单 07：产品解码与验证接缝合流](../architecture/2026-11-23-decode-merge.md)。
