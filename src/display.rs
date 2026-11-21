//! 显示几何（工单 18）：图片在相框区域内的显示尺寸——适应完整放入、
//! 填充按比例覆盖。纯函数，与宿主绘制共用同一份计算，横竖图结果可测。

use crate::decode_validation::DisplayMode;

/// 给定区域与图片原始尺寸，算显示尺寸：适应取能完整放入的最大比例，
/// 填充取能覆盖区域的最小比例（一边必然超出，靠居中裁剪）。任一尺寸为
/// 非正数时原样返回区域尺寸，避免除零。
pub fn display_size(mode: DisplayMode, area: [f32; 2], image: [f32; 2]) -> [f32; 2] {
    let (area_w, area_h) = (area[0], area[1]);
    let (img_w, img_h) = (image[0], image[1]);
    if area_w <= 0.0 || area_h <= 0.0 || img_w <= 0.0 || img_h <= 0.0 {
        return area;
    }
    let scale = match mode {
        DisplayMode::Fit => (area_w / img_w).min(area_h / img_h),
        DisplayMode::Fill => (area_w / img_w).max(area_h / img_h),
    };
    [img_w * scale, img_h * scale]
}
