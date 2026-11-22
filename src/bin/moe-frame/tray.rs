//! 托盘：图标、菜单与点击意图。

use super::*;

/// 托盘菜单项句柄（工单 17）：留住菜单项以便按核心状态更新文案。
pub(crate) struct TrayHandles {
    pub(crate) _icon: tray_icon::TrayIcon,
    pub(crate) show_hide: tray_icon::menu::MenuItem,
    pub(crate) play_pause: tray_icon::menu::MenuItem,
    pub(crate) prev: tray_icon::menu::MenuItem,
    pub(crate) next: tray_icon::menu::MenuItem,
    pub(crate) settings: tray_icon::menu::MenuItem,
    pub(crate) exit: tray_icon::menu::MenuItem,
}
/// 托盘图标（工单 17）：内置 PNG（仓库 assets/tray-icon.png，48×48），
/// 编译时嵌入，运行时解码为 RGBA。像素内容与应用图标同一母图
/// （.scratch/app-icon 工单 01：粉色蝴蝶结相框 + 少女侧颜）。
pub(crate) fn tray_icon_image() -> tray_icon::Icon {
    let image = image::load_from_memory(include_bytes!("../../../assets/tray-icon.png"))
        .expect("内置托盘图标 PNG 固定可解码")
        .to_rgba8();
    let (width, height) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), width, height).expect("托盘图标像素格式固定")
}
/// 窗口图标（.scratch/app-icon 工单 01）：应用图标母图
/// （assets/icon.png，512×512），主视口与设置视口共用；exe 的文件
/// 图标由 build.rs 把同一母图的 assets/icon.ico 编入资源。
pub(crate) fn app_icon_data() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../../../assets/icon.png"))
        .expect("内置应用图标 PNG 固定可解码")
        .to_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}
/// 建托盘图标与右键菜单（工单 17）：菜单项与相框菜单走同一套核心意图；
/// 「设置」打开工单 18 的设置窗（宿主窗口，不经核心）。失败返回 None，
/// 相框本体照常运行（托盘是常驻入口，不是相框的前置条件）。
pub(crate) fn build_tray(frame_visible: bool, playing: bool) -> Option<TrayHandles> {
    use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::new();
    // 文案用词汇表的动作名：可见时是「关闭」（只隐藏不退出），播放时是「暂停」。
    let show_hide = MenuItem::new(
        if frame_visible {
            "关闭相框"
        } else {
            "显示相框"
        },
        true,
        None,
    );
    let play_pause = MenuItem::new(if playing { "暂停" } else { "播放" }, true, None);
    let prev = MenuItem::new("上一张", true, None);
    let next = MenuItem::new("下一张", true, None);
    let settings = MenuItem::new("设置", true, None);
    let exit = MenuItem::new("退出", true, None);
    menu.append_items(&[
        &show_hide,
        &play_pause,
        &PredefinedMenuItem::separator(),
        &prev,
        &next,
        &PredefinedMenuItem::separator(),
        &settings,
        &exit,
    ])
    .ok()?;
    let icon = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_icon(tray_icon_image())
        .with_tooltip("萌框 MoeFrame")
        .build()
        .ok()?;
    Some(TrayHandles {
        _icon: icon,
        show_hide,
        play_pause,
        prev,
        next,
        settings,
        exit,
    })
}
pub(crate) fn tray_click_intent(
    button: tray_icon::MouseButton,
    button_state: tray_icon::MouseButtonState,
    focused: bool,
) -> Option<Intent> {
    match (button, button_state) {
        (tray_icon::MouseButton::Left, tray_icon::MouseButtonState::Up) => {
            Some(Intent::ToggleVisibilityRequested { focused })
        }
        _ => None,
    }
}
/// 相框视口当前是否有焦点（tray-only 工单 01）：托盘召回三分支的采样
/// 口径。egui 未报告（None）按无焦点处理——只召回、不误藏。
pub(crate) fn frame_focused(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.viewport().focused.unwrap_or(false))
}
/// 用系统默认看图器打开原图（工单 15）：设置窗「当前图」按钮与双击共用这一处；
/// 失败不崩溃。路径含中文、空格与特殊字符时由系统打开命令原样处理。
pub(crate) fn open_image(path: &std::path::Path) {
    let _ = open::that(path);
}
