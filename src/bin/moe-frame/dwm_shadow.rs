//! 关掉相框窗口的系统投影（窗口投影工单 01）：Windows 11 的
//! DWM 给无边框窗口本体加一圈矩形投影，跟窗口矩形走、不跟相
//! 框异形轮廓走，矩形内置框时藏在框边上看不见，自定义相框图
//! 的透明异形外轮廓下就露馅。窗口创建后对相框窗口 HWND 调
//! 一次 DwmSetWindowAttribute 关掉非客户区渲染落实 FR-313
//! 「不使用原生窗口阴影」；常量与日志口径等纯判定在库
//! dwm_shadow，本模块只接线。

use windows_sys::Win32::{Foundation::HWND, Graphics::Dwm::DwmSetWindowAttribute};

use super::dwm_shadow_record;

/// 给相框窗口关系统投影（窗口创建后、装配 FrameApp 时调用一
/// 次）。句柄未就绪时不调用、只记一行 skipped 留痕，便于实机
/// 对账；调用失败同样只记日志，不影响启动与任何既有行为。
pub(crate) fn disable(hwnd: isize) {
    if hwnd == 0 {
        dwm_shadow_record("skipped", "reason=no-hwnd");
        return;
    }
    let policy = moe_frame::dwm_shadow::NCRENDERING_POLICY_DISABLED;
    // SAFETY: hwnd 是本进程相框窗口的句柄（装配处经 raw-window-
    // handle 取得）；pvAttribute 指向栈上 i32、长度按其字节数
    // 传，调用同步完成，返回后指针不再被持有。
    let hresult = unsafe {
        DwmSetWindowAttribute(
            hwnd as HWND,
            moe_frame::dwm_shadow::ATTRIBUTE_NCRENDERING_POLICY,
            &policy as *const i32 as *const core::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    dwm_shadow_record("disable", &moe_frame::dwm_shadow::log_details(hresult));
}
