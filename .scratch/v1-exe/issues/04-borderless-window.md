# 无边框窗口能力

Label: wayfinder:research
Type: research
Status: resolved

## Question

当前 stable 的 eframe/egui，在 Windows 10/11 x86_64 上，能否用普通无边框窗口同时满足下面四条，而不引入 WebView，也不嵌入 WorkerW：

- 无标题栏、无系统边框。
- 拖动图片区域可移动窗口。
- 拖动边缘或四角可缩放，并能设最小尺寸。
- 按显示器 DPI 得到窗口物理像素尺寸，且窗口不置顶、不抢焦点。

只核对官方文档、eframe/egui 源码和官方 issue。结论要写明版本号、用到的 API，以及哪一条做不到。做不到时不要在这张票里选备选框架。

正在跑的研究结论写到 [borderless-window.md](../../v1-route/research/borderless-window.md)。不要另起一份。

## Answer

eframe/egui 0.37.0，所钉 winit 0.30.13。四条不能同时记成已证实，也不是只靠 ViewportBuilder 就能稳定满足。不选备选框架。全文在 [borderless-window.md](../../v1-route/research/borderless-window.md)。

- 无标题栏、无系统边框：做得到，`ViewportBuilder::with_decorations(false)`。
- 拖图片区域移动：做得到，但不自动。应用要自己命中，再发 `ViewportCommand::StartDrag`。
- 最小尺寸、默认不置顶：做得到。
- 只靠 `with_resizable(true)` 拖边缘或四角：做不到。
- `ViewportCommand::BeginResize` 有 Windows 实现，本机没跑，不能记成做得到。
- 库默认会抢焦点。创建时不抢必须 `with_active(false)`。之后是否还会抢，不确定。
- 按显示器 DPI 取物理客户区尺寸：公开数据做得到，有使用条件，没在本机跑过。
