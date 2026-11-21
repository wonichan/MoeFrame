//! 设置窗草稿结算（工单 19）：输入框文字、播种基线与当前生效值三者的
//! 纯决策——与宿主的焦点/事件机制分开，结算语义本身可测。宿主负责在
//! 失焦/回车时调用这里，并执行返回的提交结果。

use crate::config::parse_interval_draft;

/// 一次草稿结算的结果。
#[derive(Debug, PartialEq, Eq)]
pub enum DraftSettle {
    /// 无事发生：同值提交、或秒数解析失败保留上次有效值。
    Noop,
    /// 生效值已被别处改动，草稿作废，框里改为显示生效值。
    Discarded,
    /// 路径提交：以该文件夹为新的图库根（沿用换文件夹的既定规则）。
    CommitFolder(String),
    /// 秒数提交：以该值（已钳制）为新间隔。
    CommitInterval(i64),
}

/// 路径草稿结算：返回（结果，框里应显示的文字，新基线）。
/// 基线与生效值不一致说明草稿已被覆盖，直接作废；否则去首尾空白后
/// 与生效值比较，同值不动、异值提交（无效路径也照样提交，由图库
/// 扫描落到空状态，文字保留）。
pub fn settle_path_draft(
    draft: &str,
    base: &str,
    effective: &str,
) -> (DraftSettle, String, String) {
    if base != effective {
        return (
            DraftSettle::Discarded,
            effective.to_owned(),
            effective.to_owned(),
        );
    }
    let text = draft.trim().to_owned();
    if text == effective {
        (DraftSettle::Noop, text, effective.to_owned())
    } else {
        (DraftSettle::CommitFolder(text.clone()), text.clone(), text)
    }
}

/// 秒数草稿结算：返回（结果，框里应显示的文字，新基线）。
/// 解析失败（空白、小数、非数字）保留上次有效值、框里恢复生效值，
/// 绝不回退默认 30；可解析越界则钳制，框里显示钳制后的提交结果。
pub fn settle_seconds_draft(draft: &str, base: i64, effective: i64) -> (DraftSettle, String, i64) {
    if base != effective {
        return (DraftSettle::Discarded, effective.to_string(), effective);
    }
    match parse_interval_draft(draft.trim()) {
        Some(value) => {
            if value == effective {
                (DraftSettle::Noop, value.to_string(), value)
            } else {
                (DraftSettle::CommitInterval(value), value.to_string(), value)
            }
        }
        None => (DraftSettle::Noop, effective.to_string(), effective),
    }
}
