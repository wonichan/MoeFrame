# 边缘缩放先验证

Label: wayfinder:grilling
Type: grilling
Status: open

## Question

eframe 0.37.0 在 Windows 上，无边框、客户区拖动、最小尺寸、默认不置顶可以做到。只靠 `with_resizable(true)` 不能拖边缘或四角。`ViewportCommand::BeginResize` 有实现，这台机器没跑过，不能记成做得到。

Rust + eframe/egui 已经钉死。要决定的只是：在打开这个选择之前，是否先在这台 Windows 10/11 上验证 `BeginResize`。验证通过就不换框架。验证失败，才另开一张票问备选界面方案。

不要在这张票里选备选框架，也不要现在写窗口代码。
