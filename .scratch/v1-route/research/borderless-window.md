# 无边框窗口

核对日：2026-11-16。只陈述 crates.io 当前 stable 的 eframe/egui，以及它所钉的 winit，在 Windows 上用普通窗口能做什么。不选备选框架。没有在 Windows 10/11 上运行。核对的是 eframe native 的 winit 窗口，不是 WebView，也不是 WorkerW。

票面问题：无标题栏、无系统边框；拖动客户区可移动；拖动边缘或四角可缩放并能设最小尺寸；能取得 per-monitor DPI 下的窗口物理像素尺寸，且窗口默认不置顶、不抢焦点。四条要同时满足。

## 结论

四条不能同时记成已证实、且只靠 ViewportBuilder 就能稳定满足。

- 无标题栏、无系统边框：做得到。
- 拖动客户区移动：做得到，但不是自动的。应用要自己命中图片区域，再发 ViewportCommand::StartDrag。
- 最小尺寸：做得到。
- 拖边缘或四角缩放：只靠 with_resizable(true) 做不到。ViewportCommand::BeginResize 有 Windows 实现，非最大化时有官方 issue 把它当作有效，最大化后同一 issue 记录失效。本核对没有运行，这一条不确定。
- 默认不置顶：做得到。
- 默认不抢焦点：库默认做不到。必须 with_active(false)。创建时不激活有实现；之后是否还会抢焦点，不确定。
- per-monitor DPI 物理客户区尺寸：公开数据做得到。zoom 不是 1、窗口最小化、或进程不是 per-monitor DPI aware 时，不能直接把 inner_rect 乘 native_pixels_per_point 当成物理像素。没有运行验证。

## 版本

crates.io eframe 的 max_stable_version 和 newest_version 都是 0.37.0，未 yanked，created_at 为 2026-10-15T15:28:53.621684Z。crate updated_at 相同。

来源：https://crates.io/api/v1/crates/eframe

crates.io egui 的 max_stable_version 和 newest_version 都是 0.37.0，未 yanked。

来源：https://crates.io/api/v1/crates/egui

docs.rs 上 eframe 0.37.0、egui 0.37.0 的 ViewportBuilder、ViewportCommand、Context，以及 winit 0.30.13 的 Window，本次请求都返回 HTTP 200。

来源：https://docs.rs/eframe/0.37.0/eframe/
来源：https://docs.rs/egui/0.37.0/egui/viewport/struct.ViewportBuilder.html
来源：https://docs.rs/egui/0.37.0/egui/viewport/enum.ViewportCommand.html
来源：https://docs.rs/winit/0.30.13/winit/window/struct.Window.html

已发布的 egui-winit 0.37.0 依赖 winit 0.30.13。下文 Windows 行为来自这三个已发布 crate，不是 GitHub main。

官方示例取自 tag 0.37.0。

来源：https://github.com/emilk/egui/blob/0.37.0/examples/custom_window_frame/src/main.rs

## 做得到的 API

### 无标题栏、无系统边框

ViewportBuilder::with_decorations(false)。docs.rs 原文：Sets whether the window should have a border, a title bar, etc. 默认 true。

来源：https://docs.rs/egui/0.37.0/egui/viewport/struct.ViewportBuilder.html#method.with_decorations

winit 0.30.13 的 WindowAttributes::with_decorations 文档同样写 border、title bar，默认 true。

来源：https://docs.rs/winit/0.30.13/winit/window/struct.WindowAttributes.html#method.with_decorations

Windows 上，无装饰不是再套 WebView。WM_NCCALCSIZE 在 MARKER_DECORATIONS 为假且 wparam 不为 0 时不走 DefWindowProc，客户区盖住窗口。最大化时把矩形限制到当前显示器的 work area。

来源：已发布 winit 0.30.13 的 src/platform_impl/windows/event_loop.rs，WM_NCCALCSIZE 分支。发布包：https://static.crates.io/crates/winit/winit-0.30.13.crate

tag 0.37.0 的 custom_window_frame 在非 macOS 上调用 with_decorations(false)。

来源：https://github.com/emilk/egui/blob/0.37.0/examples/custom_window_frame/src/main.rs

### 拖客户区移动

公开命令是 ViewportCommand::StartDrag。egui-winit 0.37.0 把它转成 window.drag_window()。

来源：已发布 egui-winit 0.37.0 的 src/lib.rs，ViewportCommand::StartDrag 分支。发布包：https://static.crates.io/crates/egui-winit/egui-winit-0.37.0.crate

winit Windows 实现是 handle_os_dragging(HTCAPTION)：ReleaseCapture，再 PostMessageW(WM_NCLBUTTONDOWN, HTCAPTION, 当前光标点)。

来源：已发布 winit 0.30.13 的 src/platform_impl/windows/window.rs，drag_window。

官方示例只在标题栏的 Sense::click_and_drag() 上，于 drag_started_by(PointerButton::Primary) 时 send_viewport_cmd(ViewportCommand::StartDrag)。图片区域要移动窗口，应用得对自己的区域做同样的命中，再发同一条命令。

来源：https://github.com/emilk/egui/blob/0.37.0/examples/custom_window_frame/src/main.rs

with_movable_by_window_background 出现在 macOS 窗口属性上。已核对的 Windows 无边框路径没有「点背景即拖动」开关。

### 最小尺寸

ViewportBuilder::with_min_inner_size。docs.rs 原文：Sets the minimum dimensions a window can have. 未设置则没有最小尺寸（保留值除外），并写明应大于 0。

来源：https://docs.rs/egui/0.37.0/egui/viewport/struct.ViewportBuilder.html#method.with_min_inner_size

ViewportCommand::MinInnerSize 在 egui-winit 里调用 window.set_min_inner_size。winit Windows 把值存进窗口状态，再 request_inner_size 让系统重新检查。

来源：已发布 egui-winit 0.37.0 的 src/lib.rs；已发布 winit 0.30.13 的 src/platform_impl/windows/window.rs，set_min_inner_size。

官方示例调用了 with_min_inner_size([400.0, 100.0])。

来源：https://github.com/emilk/egui/blob/0.37.0/examples/custom_window_frame/src/main.rs

### 默认不置顶

with_window_level 的 docs.rs 原文：Control if window is always-on-top, always-on-bottom, or neither. with_always_on_top 的原文是 This window is always on top。

来源：https://docs.rs/egui/0.37.0/egui/viewport/struct.ViewportBuilder.html#method.with_window_level
来源：https://docs.rs/egui/0.37.0/egui/viewport/struct.ViewportBuilder.html#method.with_always_on_top

winit WindowAttributes::with_window_level 写明默认是 WindowLevel::Normal，并且只是给 OS 的 hint。

来源：https://docs.rs/winit/0.30.13/winit/window/struct.WindowAttributes.html#method.with_window_level

Windows 实现只在 WindowLevel::AlwaysOnTop 时设置 ALWAYS_ON_TOP，对应 WS_EX_TOPMOST。不调用 with_always_on_top 时不走这条。

来源：已发布 winit 0.30.13 的 src/platform_impl/windows/window.rs，set_window_level；同 crate 的 src/platform_impl/windows/window_state.rs，to_window_styles。

### 创建时可以不抢焦点

这不是库默认。egui-winit 创建窗口时使用 active.unwrap_or(true)。要创建时不激活，必须 ViewportBuilder::with_active(false)。

docs.rs 原文：Whether the window will be initially focused or not. 并写 The window should be assumed as not focused by default。不支持列表是 Android / iOS / X11 / Wayland / Orbital，没有 Windows。

来源：https://docs.rs/egui/0.37.0/egui/viewport/struct.ViewportBuilder.html#method.with_active
来源：https://docs.rs/winit/0.30.13/winit/window/struct.WindowAttributes.html#method.with_active

winit Windows 在 MARKER_ACTIVATE 为假时，第一次显示用 SW_SHOWNOACTIVATE，然后把该标记设为真。

来源：已发布 winit 0.30.13 的 src/platform_impl/windows/window_state.rs，apply_diff。

### per-monitor DPI 下的物理客户区尺寸

ViewportInfo 文档：All units are in ui points。换算是 pixels_per_point = zoom_factor * native_pixels_per_point。native_pixels_per_point 是 OS native pixels-per-point。inner_rect 是原生窗口内容矩形，单位是 ui points。

来源：已发布 egui 0.37.0 的 src/data/input/viewport_info.rs。发布包：https://static.crates.io/crates/egui/egui-0.37.0.crate
来源：https://docs.rs/egui/0.37.0/egui/viewport/struct.ViewportInfo.html

egui-winit 的 update_viewport_info 写 viewport_info.native_pixels_per_point = Some(window.scale_factor() as f32)。inner_rect_in_points 先取 window.inner_size() 的物理像素，再除以 pixels_per_point(egui_ctx, window)。

来源：已发布 egui-winit 0.37.0 的 src/lib.rs，inner_rect_in_points 与 update_viewport_info。

Window::inner_size 在 Windows 上由 GetClientRect 得到 PhysicalSize<u32>。

来源：已发布 winit 0.30.13 的 src/platform_impl/windows/window.rs，inner_size。

窗口未最小化，且乘回同一次 pixels_per_point 时，物理客户区尺寸可以还原。默认 zoom 为 1 时，这等于 inner_rect 尺寸乘 native_pixels_per_point。

Windows 10 1607 及以后，hwnd_dpi 走 GetDpiForWindow。become_dpi_aware() 先尝试 SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)，失败再试 DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE。进程若是 DPI unaware，缩放系数退回 BASE_DPI 96，也就是 1.0。

来源：已发布 winit 0.30.13 的 src/platform_impl/windows/dpi.rs。

winit scale_factor 文档链接 Microsoft 的 High DPI 说明，并写到 monitor scale factor 可以和 window scale factor 不同。

来源：https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.scale_factor
来源：https://docs.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows

## 做不到

### 只靠 with_resizable(true)，无边框窗口没有可拖的系统边缘或四角

with_resizable 的 docs.rs 原文：Sets whether the window is resizable or not. 默认 true。

来源：https://docs.rs/egui/0.37.0/egui/viewport/struct.ViewportBuilder.html#method.with_resizable

这不会在 Windows 无装饰窗口上留下可抓取的系统边框。to_window_styles 在 RESIZABLE 时加上 WS_SIZEBOX，但顶层窗口不会因为 MARKER_DECORATIONS 为假就把 caption 从实际样式里去掉。视觉上的无边框靠 WM_NCCALCSIZE 把非客户区算没。已发布 winit 0.30.13 的 Windows event_loop.rs 没有 WM_NCHITTEST 处理函数把边缘返回成 HTLEFT、HTRIGHT、HTTOP、HTBOTTOM 或四角。WM_NCHITTEST 只出现在别的消息的注释里。

来源：已发布 winit 0.30.13 的 src/platform_impl/windows/window_state.rs，to_window_styles；同 crate 的 src/platform_impl/windows/event_loop.rs。

官方 issue 与此一致：

- https://github.com/emilk/egui/issues/4345 仍 open。标题是 decorations false、resizable true 时原生窗口不能缩放。评论写明 with_resizable(true) 加 with_decorations(false) 在 Windows 上得到不可缩放窗口，macOS 和 Linux 上可以。
- https://github.com/emilk/egui/issues/5496 仍 open。custom_window_frame 上 with_decorations(false) 加 with_resizable(true) 后不能缩放原生窗口。
- https://github.com/emilk/egui/issues/2548 已 closed。正文写无装饰或自定义装饰的窗口，除非用 Super + 右键拖动这类快捷方式，否则看起来不能缩放。
- https://github.com/emilk/egui/issues/7576 本次抓取的页面未标 closed。正文：Using the edges and corners to resize the window for the custom_window_frame example does not work on Windows. macOS 可以。OS 写的是 Windows 11。正文没有提到 BeginResize。

tag 0.37.0 的 custom_window_frame 没有边缘命中，也没有 BeginResize。这个示例本身不能证明拖边缘可缩放。

来源：https://github.com/emilk/egui/blob/0.37.0/examples/custom_window_frame/src/main.rs

### 库默认会抢焦点

不调用 with_active(false) 时，egui-winit 传入 active.unwrap_or(true)。默认不置顶成立。默认不抢焦点不成立。

来源：已发布 egui-winit 0.37.0 的 src/lib.rs，create window 路径里的 with_active(active.unwrap_or(true))。

### 拖图片区域不会自动移动窗口

拖动客户区移动依赖应用发送 StartDrag。已核对的 Windows 路径没有「任意客户区按下即开始窗口拖动」。

## 不确定

### BeginResize 能否在 Windows 10/11 的普通无边框窗口上稳定完成边缘或四角缩放

API 存在。ViewportCommand::BeginResize(ResizeDirection) 由 egui-winit 转成 window.drag_resize_window。winit Windows 对八个方向分别 PostMessageW(WM_NCLBUTTONDOWN, HTLEFT / HTRIGHT / HTTOP / HTBOTTOM / 四角, 当前光标点)，并总是返回 Ok(())。

来源：已发布 egui-winit 0.37.0 的 src/lib.rs，ViewportCommand::BeginResize 分支。
来源：已发布 winit 0.30.13 的 src/platform_impl/windows/window.rs，drag_resize_window。

https://github.com/rust-windowing/winit/issues/4821 仍 open，updated_at 为 2026-11-06T21:12:41Z。最小例把 winit 钉在 =0.30.13。正文描述没有原生 chrome 的窗口，深色边框用于缩放，浅色中间用于移动。它同时记录：对最大化窗口调用 drag_resize_window 没有效果；恢复后，之后的 drag_resize_window 和 drag_window 也没有效果，但调用仍返回 Ok(())。

非最大化时，这条 issue 把 drag_resize_window 当作无边框窗口的缩放手段。最大化之后，同一条 issue 记录拖动和缩放一起失效。本核对没有在 Windows 10 或 11 上运行，不能把拖边缘或四角可缩放写成已证实。

### 无装饰时是否绝对没有 1px 边

WM_NCCALCSIZE 注释写明：若 MARKER_UNDECORATED_SHADOW 打开，为了阴影会把非客户区微调，顶部会出现 1px 黑边。本结论没有引用这个标记的默认值。阴影打开时，「视觉上完全没有系统边」不成立。默认是否打开，不确定。

来源：已发布 winit 0.30.13 的 src/platform_impl/windows/event_loop.rs，WM_NCCALCSIZE 分支。

### 物理像素的使用条件

- inner_rect 的除数是含 zoom_factor 的 pixels_per_point。native_pixels_per_point 只来自 window.scale_factor()。zoom 不是 1 时，只乘 native_pixels_per_point 得不到 GetClientRect 的物理像素。
- 窗口最小化时，update_viewport_info 把 inner_rect 设为 None。
- become_dpi_aware() 在已发布的 winit 0.30.13 里。本结论没有逐行引用 eframe 哪个入口调用它。进程若已经被标成 DPI unaware，scale_factor 会是 1.0，GetClientRect 得到的是系统虚拟化后的尺寸，不是显示器物理像素。
- 没有在 Windows 10/11 x86_64 上读取过实际 DPI 数值。

### with_active(false) 之后是否还会抢焦点

已核对的是创建属性和第一次 SW_SHOWNOACTIVATE。第一次显示后 MARKER_ACTIVATE 会被设为真，之后的显示走 SW_SHOW。本结论没有把 eframe 后续所有 Focus 或 focus_window 调用读完。创建时不激活有实现。整个生命周期都不抢焦点，不确定。

来源：已发布 winit 0.30.13 的 src/platform_impl/windows/window_state.rs，apply_diff。

## 四条对照

| 条 | 判定 | 已核对的 API |
| --- | --- | --- |
| 无标题栏、无系统边框 | 做得到 | ViewportBuilder::with_decorations(false) |
| 拖图片区域移动窗口 | 做得到，不自动 | 应用命中后 ViewportCommand::StartDrag |
| 拖边缘或四角缩放 | 自动路径做不到；手动路径不确定 | with_resizable(true) 不够；ViewportCommand::BeginResize 未运行验证 |
| 设最小尺寸 | 做得到 | ViewportBuilder::with_min_inner_size |
| per-monitor DPI 物理像素尺寸 | 数据做得到，使用条件见上 | ViewportInfo.inner_rect、native_pixels_per_point、Window::scale_factor、Window::inner_size |
| 默认不置顶 | 做得到 | 默认 WindowLevel::Normal，不调用 with_always_on_top |
| 默认不抢焦点 | 库默认做不到；创建时可以关掉 | 必须 with_active(false)；默认 active.unwrap_or(true) |

没有选择别的 GUI 框架。上述缺口只说明当前 stable eframe/egui 0.37.0 加它所钉的 winit 0.30.13 在 Windows 上的事实。
