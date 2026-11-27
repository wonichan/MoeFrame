//! 窗口实况快照的宿主接线（窗口投影工单 03）：在库
//! window_probe 判定命中的点位（首次失焦 / 外观模式切换）
//! 实读相框窗口的当前状态记一行 [window-probe]：样式与扩展
//! 样式、GetWindowRect 的窗口矩形、应用侧当帧相框绘制矩形
//! 换算到屏幕物理坐标后的矩形——实机据此对账 (甲) 样式是否
//! 被写回、(乙) 窗口矩形与绘制矩形是否错位。只读不改窗口，
//! 读取失败就不写行（宁可缺行，不写假数据）。设置窗不在此
//! 处、不读不记。

use eframe::egui;
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{GetWindowLongPtrW, GetWindowRect, GWL_EXSTYLE, GWL_STYLE},
};

use super::window_probe_record;

/// 记一行实况快照。draw_rect 是应用侧当帧的相框绘制矩形
/// （视口逻辑坐标，即 CentralPanel 的绘制矩形）；换算口径
/// 在库 window_probe 模块头（窗口左上 + 逻辑矩形 ×
/// pixels_per_point）。句柄未就绪时只记一行 skipped 留痕。
pub(crate) fn snapshot(hwnd: isize, event: &str, draw_rect: egui::Rect, pixels_per_point: f32) {
    if hwnd == 0 {
        window_probe_record("skipped", "reason=no-hwnd");
        return;
    }
    let hwnd = hwnd as HWND;
    // SAFETY: hwnd 是本进程相框窗口的句柄，在 UI 线程上读其
    // 样式与窗口矩形是纯查询，不改窗口状态。
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return;
        }
        let win_rect = [rect.left, rect.top, rect.right, rect.bottom];
        let draw_physical = moe_frame::window_probe::draw_rect_physical(
            [rect.left, rect.top],
            [
                draw_rect.min.x,
                draw_rect.min.y,
                draw_rect.max.x,
                draw_rect.max.y,
            ],
            pixels_per_point,
        );
        window_probe_record(
            event,
            &moe_frame::window_probe::probe_log_details(style, ex_style, win_rect, draw_physical),
        );
    }
}
