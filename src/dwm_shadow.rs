//! 相框窗口系统投影（DWM 投影）的纯判定（窗口投影工单 01）：
//! Windows 11 的 DWM 会给无边框的相框窗口本体加一圈矩形投影，
//! 跟窗口矩形走、不跟相框异形轮廓走。宿主在窗口创建后以
//! DwmSetWindowAttribute 关掉非客户区渲染（DWMNCRP_DISABLED），
//! 本模块只出与文档对齐的常量和调用结果的日志口径，不碰窗口。

/// DwmSetWindowAttribute 的属性号 DWMWA_NCRENDERING_POLICY
/// （Win32 文档值）。windows-sys 绑定里该参数是 u32，故此处以
/// u32 定型，宿主直接传。
pub const ATTRIBUTE_NCRENDERING_POLICY: u32 = 2;

/// 非客户区渲染策略 DWMNCRP_DISABLED（Win32 文档值）：禁用
/// 非客户区渲染，相框窗口的 DWM 投影随之消失。
pub const NCRENDERING_POLICY_DISABLED: i32 = 1;

/// DwmSetWindowAttribute 返回的 HRESULT 是否成功（S_OK 及一切
/// 非负值皆成功，负值是失败码）。
pub fn succeeded(hresult: i32) -> bool {
    hresult >= 0
}

/// 调用结果的一行日志明细（宿主经 dwm_shadow_record 落
/// moe_frame.log）：成功/失败 + 八位十六进制 HRESULT，失败时
/// 据此在实机日志里对码排查。
pub fn log_details(hresult: i32) -> String {
    let result = if succeeded(hresult) { "ok" } else { "failed" };
    format!("result={result} hresult=0x{:08X}", hresult as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_constant_matches_win32_doc() {
        assert_eq!(ATTRIBUTE_NCRENDERING_POLICY, 2);
    }

    #[test]
    fn disabled_constant_matches_win32_doc() {
        assert_eq!(NCRENDERING_POLICY_DISABLED, 1);
    }

    #[test]
    fn s_ok_is_success() {
        assert!(succeeded(0));
    }

    #[test]
    fn negative_hresult_is_failure() {
        // E_INVALIDARG = 0x80070057
        assert!(!succeeded(0x8007_0057u32 as i32));
    }

    #[test]
    fn log_details_for_success() {
        assert_eq!(log_details(0), "result=ok hresult=0x00000000");
    }

    #[test]
    fn log_details_for_failure() {
        assert_eq!(
            log_details(0x8007_0057u32 as i32),
            "result=failed hresult=0x80070057"
        );
    }
}
