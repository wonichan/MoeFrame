//! 窗口几何纯函数（工单 20）：宽高比锁定修正、离屏钳制、右下角放置。
//! 只算数不碰窗口，宿主负责观察与下发。

/// 窗口最小边（逻辑尺寸，父规格第 4 节）：锁定修正与宿主最小尺寸共用。
pub const MIN_WINDOW_SIDE: f32 = 120.0;

/// 右下角内缩放置（首次启动、恢复默认、离屏钳制共用）：给定窗口尺寸
/// 与显示器尺寸，返回右下角内缩 24px 的位置（不小于 0）。
pub fn bottom_right_position(size: [f32; 2], monitor: [f32; 2]) -> [f32; 2] {
    [
        (monitor[0] - size[0] - 24.0).max(0.0),
        (monitor[1] - size[1] - 24.0).max(0.0),
    ]
}

/// 宽高比锁定下的尺寸修正（工单 20）：以刚变化的那一边为准推导另一边
/// （两边都变时以宽为准），再套 120 逻辑最小边——一边触底时按比例反推
/// 另一边，两边都触底时最小边优先、比例让步。aspect 非正/非有限时原样
/// 返回。纯函数：宿主只在缩放停稳后调用并下发结果。
pub fn aspect_locked_size(last: [f32; 2], current: [f32; 2], aspect: f32) -> [f32; 2] {
    if !aspect.is_finite() || aspect <= 0.0 {
        return current;
    }
    let width_changed = (current[0] - last[0]).abs() > 0.5;
    let height_changed = (current[1] - last[1]).abs() > 0.5;
    let (mut w, mut h) = if width_changed || !height_changed {
        (current[0], current[0] / aspect)
    } else {
        (current[1] * aspect, current[1])
    };
    if w < MIN_WINDOW_SIDE {
        w = MIN_WINDOW_SIDE;
        h = w / aspect;
    }
    if h < MIN_WINDOW_SIDE {
        h = MIN_WINDOW_SIDE;
        w = h * aspect;
    }
    if w < MIN_WINDOW_SIDE {
        w = MIN_WINDOW_SIDE;
    }
    [w, h]
}

/// 启动恢复位置的主屏钳制（工单 20）：窗口与主屏完全无交集时返回主屏
/// 右下角内缩 24px 的位置；有交集返回 None。注意：egui 只给得出相框
/// 所在屏的尺寸、没有屏幕原点与枚举，本判定把主屏当作原点在 (0,0) 的
/// 矩形——**整窗合法地摆在副屏上的位置也会被判离屏、启动时被移回主屏
/// 右下角**，即当前实现不保存副屏摆放（副屏拔掉防丢与副屏保留二者在本
/// API 下不可兼得，取防丢）。多屏用户以实机清单的点名项为准。
pub fn offscreen_position(
    position: [f32; 2],
    size: [f32; 2],
    monitor: [f32; 2],
) -> Option<[f32; 2]> {
    let intersects = position[0] < monitor[0]
        && position[1] < monitor[1]
        && position[0] + size[0] > 0.0
        && position[1] + size[1] > 0.0;
    if intersects {
        None
    } else {
        Some(bottom_right_position(size, monitor))
    }
}
