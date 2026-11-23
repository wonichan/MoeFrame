//! 适应、填充的纯尺寸算术；解码和绘制共用同一条比例规则。

use std::{num::TryFromIntError, ops::Mul};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    Fit,
    Fill,
}

struct DerivedEdge<T> {
    axis: usize,
    numerator: T,
    denominator: T,
}

/// 给定区域与图片原始尺寸，算能完整放入（适应）或恰好覆盖（填充）的尺寸。
/// 绘制保留小数；任一输入非正或非有限时原样返回区域尺寸。
pub fn display_size(mode: DisplayMode, area: [f32; 2], image: [f32; 2]) -> [f32; 2] {
    if area
        .into_iter()
        .chain(image)
        .any(|edge| edge <= 0.0 || !edge.is_finite())
    {
        return area;
    }
    let edge = derived_edge(mode, area.map(f64::from), image.map(f64::from));
    let mut size = area;
    size[edge.axis] = (edge.numerator / edge.denominator) as f32;
    size
}

/// 解码使用整数像素：适应向下取整且至少为 1；填充向上取整，溢出则拒绝。
/// 调用方已检查目标和源尺寸均大于零。
pub(crate) fn display_size_pixels(
    mode: DisplayMode,
    area: [u32; 2],
    image: [u32; 2],
) -> Result<[u32; 2], TryFromIntError> {
    let edge = derived_edge(mode, area.map(u64::from), image.map(u64::from));
    let derived = match mode {
        DisplayMode::Fit => (edge.numerator / edge.denominator).max(1),
        DisplayMode::Fill => edge.numerator.div_ceil(edge.denominator),
    };
    let mut size = area;
    size[edge.axis] = u32::try_from(derived)?;
    Ok(size)
}

/// 一边取目标值，另一边由源比例推导。返回派生边下标及其分子、分母；
/// 用交叉乘积选边，整数解码不经浮点即可保住临界处的精确取整。
fn derived_edge<T>(mode: DisplayMode, [tw, th]: [T; 2], [sw, sh]: [T; 2]) -> DerivedEdge<T>
where
    T: Copy + PartialOrd + Mul<Output = T>,
{
    let height_numerator = tw * sh;
    let width_numerator = th * sw;
    let use_target_width = match mode {
        DisplayMode::Fit => height_numerator <= width_numerator,
        DisplayMode::Fill => height_numerator >= width_numerator,
    };
    if use_target_width {
        DerivedEdge {
            axis: 1,
            numerator: height_numerator,
            denominator: sw,
        }
    } else {
        DerivedEdge {
            axis: 0,
            numerator: width_numerator,
            denominator: sh,
        }
    }
}
