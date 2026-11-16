# 无边框窗口能力

Label: wayfinder:research
Type: research
Status: claimed

## Question

当前 stable 的 eframe/egui，在 Windows 10/11 x86_64 上，能否用普通无边框窗口同时满足下面四条，而不引入 WebView，也不嵌入 WorkerW：

- 无标题栏、无系统边框。
- 拖动图片区域可移动窗口。
- 拖动边缘或四角可缩放，并能设最小尺寸。
- 按显示器 DPI 得到窗口物理像素尺寸，且窗口不置顶、不抢焦点。

只核对官方文档、eframe/egui 源码和官方 issue。结论要写明版本号、用到的 API，以及哪一条做不到。做不到时不要在这张票里选备选框架。
