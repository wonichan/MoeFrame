//! 相框窗口边框样式剥离的纯判定（窗口投影工单 02）：winit 0.30
//! 对无边框可缩放窗口恒置 WS_CAPTION | WS_BORDER | WS_SYSMENU
//! 并按可缩放加 WS_SIZEBOX、扩展样式带 WS_EX_WINDOWEDGE
//! （vendored 源码 window_state.rs `to_window_styles`），无边框
//! 外观只靠 WM_NCCALCSIZE 把客户区铺满实现——DWM 因此仍按
//! 「带框窗口」给它画整圈框架影，工单 01 的
//! DWMNCRP_DISABLED 管不到这一路。宿主在装配后剥掉这组样式
//! 让窗口成为真正的无框窗口；本模块只出与 Win32 文档对齐的
//! 常量、剥离计算与日志口径，不碰窗口。

/// WS_BORDER（Win32 文档值）：单像素边框位，同时是
/// WS_CAPTION 的组成部分。
pub const WS_BORDER: u32 = 0x0080_0000;

/// WS_CAPTION（Win32 文档值）：标题栏位组合
/// （WS_BORDER | WS_DLGFRAME）。
pub const WS_CAPTION: u32 = 0x00C0_0000;

/// WS_SIZEBOX（Win32 文档值，即 WS_THICKFRAME）：可缩放粗
/// 边框位。
pub const WS_SIZEBOX: u32 = 0x0004_0000;

/// WS_EX_WINDOWEDGE（Win32 文档值）：扩展样式的窗口边缘位。
pub const WS_EX_WINDOWEDGE: u32 = 0x0000_0100;

/// 从 GWL_STYLE 剥掉的位（工单 02 定案集合）：标题栏、粗边
/// 框、单像素边框。WS_SYSMENU 等其余位不在集合内、原样保留。
const STYLE_STRIP_MASK: u32 = WS_CAPTION | WS_SIZEBOX | WS_BORDER;

/// 从 GWL_EXSTYLE 剥掉的位（工单 02 定案集合）。
const EXSTYLE_STRIP_MASK: u32 = WS_EX_WINDOWEDGE;

/// 剥离后的 GWL_STYLE：入参是 GetWindowLongPtr 的返回值（样
/// 式在低 32 位），只清定案集合的位、其余位逐位保留。
pub fn stripped_style(style: isize) -> isize {
    style & !(STYLE_STRIP_MASK as i32 as isize)
}

/// 剥离后的 GWL_EXSTYLE：口径同 [`stripped_style`]。
pub fn stripped_ex_style(ex_style: isize) -> isize {
    ex_style & !(EXSTYLE_STRIP_MASK as i32 as isize)
}

/// 样式自愈判定（窗口投影工单 03）：WM_STYLECHANGED 带来的
/// 新 GWL_STYLE 里，定案集合的位是否重现。winit 的 setter
/// （set_always_on_top 等）经 set_window_flags 以
/// to_window_styles() 整词重写样式，装配时剥掉的位会被挂
/// 回来——本判定命中即由宿主当场再剥。位已清返回 false：再
/// 剥自身触发的二次 STYLECHANGED 在此零动作收敛。
pub fn style_restrip_needed(new_style: isize) -> bool {
    new_style & (STYLE_STRIP_MASK as i32 as isize) != 0
}

/// 扩展样式自愈判定（窗口投影工单 03）：口径同
/// [`style_restrip_needed`]，集合只有 WS_EX_WINDOWEDGE；
/// WS_EX_TOPMOST 等集合外的位在场不触发。
pub fn exstyle_restrip_needed(new_ex_style: isize) -> bool {
    new_ex_style & (EXSTYLE_STRIP_MASK as i32 as isize) != 0
}

/// 再剥结果的一行日志明细（宿主经 window_style_record 以
/// event=restrip 落 moe_frame.log）：label 是被再剥的索引
/// （style / exstyle），值为触发消息带来的新值（剥前）与
/// 写回后现读的实际值（剥后），口径与 [`log_details`] 一致。
pub fn restrip_log_details(label: &str, before: isize, after: isize, applied: bool) -> String {
    let result = if applied { "ok" } else { "failed" };
    format!(
        "{label}=0x{:08X}->0x{:08X} result={result}",
        before as u32, after as u32,
    )
}

/// 剥离结果的一行日志明细（宿主经 window_style_record 落
/// moe_frame.log）：剥前/剥后样式值（低 32 位八位十六进制）
/// 与成败，实机据此对账样式是否真被剥掉。
pub fn log_details(
    style_before: isize,
    style_after: isize,
    ex_style_before: isize,
    ex_style_after: isize,
    applied: bool,
) -> String {
    let result = if applied { "ok" } else { "failed" };
    format!(
        "style=0x{:08X}->0x{:08X} exstyle=0x{:08X}->0x{:08X} result={result}",
        style_before as u32, style_after as u32, ex_style_before as u32, ex_style_after as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_constants_match_win32_doc() {
        assert_eq!(WS_BORDER, 0x0080_0000);
        assert_eq!(WS_CAPTION, 0x00C0_0000);
        assert_eq!(WS_SIZEBOX, 0x0004_0000);
        assert_eq!(WS_EX_WINDOWEDGE, 0x0000_0100);
    }

    #[test]
    fn strips_frame_bits_from_winit_style() {
        // winit 无边框可缩放窗口的典型样式组合（vendored
        // to_window_styles）：WS_POPUP | WS_CLIPSIBLINGS |
        // WS_CLIPCHILDREN | WS_VISIBLE + WS_CAPTION |
        // WS_SYSMENU | WS_SIZEBOX 等。
        let style: isize = (0x8000_0000u32
            | 0x0400_0000
            | 0x0200_0000
            | 0x1000_0000
            | WS_CAPTION
            | 0x0008_0000
            | WS_SIZEBOX) as i32 as isize;
        let stripped = stripped_style(style);
        // 定案集合的位全清。
        assert_eq!(stripped & (WS_CAPTION as isize), 0);
        assert_eq!(stripped & (WS_SIZEBOX as isize), 0);
        assert_eq!(stripped & (WS_BORDER as isize), 0);
        // 集合外的位逐位保留（含不在剥除集合的 WS_SYSMENU）。
        assert_eq!(
            stripped,
            (0x8000_0000u32 | 0x0400_0000 | 0x0200_0000 | 0x1000_0000 | 0x0008_0000) as i32
                as isize
        );
    }

    #[test]
    fn stripped_style_is_idempotent() {
        let style: isize = (0x8000_0000u32 | WS_CAPTION | WS_SIZEBOX) as i32 as isize;
        let once = stripped_style(style);
        assert_eq!(stripped_style(once), once);
    }

    #[test]
    fn strips_windowedge_from_ex_style() {
        // WS_EX_TOPMOST（0x8）在集合外，必须保留。
        let ex_style: isize = (WS_EX_WINDOWEDGE | 0x0000_0008) as isize;
        assert_eq!(stripped_ex_style(ex_style), 0x0000_0008isize);
    }

    #[test]
    fn style_restrip_needed_detects_returned_frame_bits() {
        // winit 整词重写回的典型样式：定案集合任一位在场即需再剥。
        let full: isize = (0x8000_0000u32 | WS_CAPTION | 0x0008_0000 | WS_SIZEBOX) as i32 as isize;
        assert!(style_restrip_needed(full));
        assert!(style_restrip_needed(WS_CAPTION as isize));
        assert!(style_restrip_needed(WS_SIZEBOX as isize));
        assert!(style_restrip_needed(WS_BORDER as isize));
    }

    #[test]
    fn style_restrip_needed_quiet_when_bits_clear() {
        // 剥后样式（自触发的二次 STYLECHANGED 带来的正是它）：
        // 零动作收敛的判定基础。
        let style: isize = (0x8000_0000u32 | WS_CAPTION | WS_SIZEBOX) as i32 as isize;
        assert!(!style_restrip_needed(stripped_style(style)));
        assert!(!style_restrip_needed(0));
        // 集合外的位（WS_POPUP / WS_SYSMENU）在场不触发。
        assert!(!style_restrip_needed(
            (0x8000_0000u32 | 0x0008_0000) as i32 as isize
        ));
    }

    #[test]
    fn exstyle_restrip_needed_detects_windowedge_only() {
        assert!(exstyle_restrip_needed(
            (WS_EX_WINDOWEDGE | 0x0000_0008) as isize
        ));
        // WS_EX_TOPMOST（0x8）是守擂的命脉、不在剥离集合：单独
        // 在场不触发再剥。
        assert!(!exstyle_restrip_needed(0x0000_0008isize));
        assert!(!exstyle_restrip_needed(0));
    }

    #[test]
    fn restrip_log_details_format() {
        let before: isize = (0x8000_0000u32 | WS_CAPTION | WS_SIZEBOX) as i32 as isize;
        let after = stripped_style(before);
        assert_eq!(
            restrip_log_details("style", before, after, true),
            "style=0x80C40000->0x80000000 result=ok"
        );
        assert_eq!(
            restrip_log_details("exstyle", 0x0000_0100, 0, false),
            "exstyle=0x00000100->0x00000000 result=failed"
        );
    }

    #[test]
    fn log_details_format() {
        let before: isize = (0x8000_0000u32 | WS_CAPTION | WS_SIZEBOX) as i32 as isize;
        let after = stripped_style(before);
        assert_eq!(
            log_details(before, after, 0x0000_0100, 0, true),
            "style=0x80C40000->0x80000000 exstyle=0x00000100->0x00000000 result=ok"
        );
        assert_eq!(
            log_details(before, before, 0x0000_0100, 0x0000_0100, false),
            "style=0x80C40000->0x80C40000 exstyle=0x00000100->0x00000100 result=failed"
        );
    }
}
