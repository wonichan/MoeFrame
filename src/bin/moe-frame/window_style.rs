//! 剥掉相框窗口的边框样式（窗口投影工单 02）：winit 0.30 对
//! 无边框可缩放窗口恒置 WS_CAPTION | WS_BORDER | WS_SYSMENU
//! 并按可缩放加 WS_SIZEBOX、扩展样式带 WS_EX_WINDOWEDGE，
//! DWM 因此仍按「带框窗口」给相框窗口画整圈框架影——工单 01
//! 的 DWMNCRP_DISABLED 实机回执 result=ok 而暗层依旧，证伪
//! 的正是这一路。装配后（吸附子类化挂好之后）读改写
//! GWL_STYLE / GWL_EXSTYLE 剥掉这组样式并以
//! SWP_FRAMECHANGED 生效，窗口成为真正的无框窗口；剥离集
//! 合、日志口径等纯判定在库 window_style，本模块只接线。
//! 剥样式的连带后果（DefWindowProc 的移动/缩放模态环失效）
//! 由 snap_subclass 的自研模态环接管（同票）。

use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    },
};

use super::window_style_record;

/// 给相框窗口剥边框样式（吸附子类化挂好之后、装配 FrameApp
/// 时调用一次）。句柄未就绪时不调用、只记一行 skipped 留痕；
/// 失败同样只记日志，不影响启动与任何既有行为。前后样式值
/// 以窗口实际状态入日志（写回后现读现核），实机据此对账。
pub(crate) fn strip(hwnd: isize) {
    if hwnd == 0 {
        window_style_record("skipped", "reason=no-hwnd");
        return;
    }
    let hwnd = hwnd as HWND;
    // SAFETY: hwnd 是本进程相框窗口的句柄（装配处经 raw-window-
    // handle 取得）；读写的是本窗自己的样式位，SetWindowPos
    // 只以 SWP_FRAMECHANGED 让新样式生效——不移动、不改尺
    // 寸、不动 Z 序、不激活。
    unsafe {
        let style_before = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let ex_style_before = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let style_after = moe_frame::window_style::stripped_style(style_before);
        let ex_style_after = moe_frame::window_style::stripped_ex_style(ex_style_before);
        SetWindowLongPtrW(hwnd, GWL_STYLE, style_after);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style_after);
        let applied = SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        ) != 0;
        let style_now = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let ex_style_now = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let ok = applied && style_now == style_after && ex_style_now == ex_style_after;
        window_style_record(
            "strip",
            &moe_frame::window_style::log_details(
                style_before,
                style_now,
                ex_style_before,
                ex_style_now,
                ok,
            ),
        );
    }
}
