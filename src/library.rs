//! 图库扫描：从根文件夹递归枚举图片路径，只保存路径，不解码任何图片（父规格第 2 节）。
//! 结果按完整路径自然序排列（数字段按数值、其余按 Unicode 码点）。

use std::{cmp::Ordering, path::PathBuf};
use walkdir::WalkDir;

const IMAGE_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "bmp", "gif"];

/// 递归扫描根文件夹。根不存在、是文件或为空时返回空集合，不报错——调用方据此进入无图状态。
pub fn scan_library(root: &std::path::Path) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut paths: Vec<PathBuf> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| is_image_path(path))
        .collect();
    paths.sort_by(|a, b| natural_cmp(&path_key(a), &path_key(b)));
    paths
}

/// 在自然序列表中找一个缺失路径的后继位置：第一个自然序严格大于它的路径下标，
/// 没有时回到开头（工单 13 顺序模式恢复：当前图缺失时从它的下一张起循环寻找）。
pub fn natural_successor_index(sorted: &[PathBuf], missing: &std::path::Path) -> usize {
    let key = path_key(missing);
    sorted
        .iter()
        .position(|path| natural_cmp(&path_key(path), &key) == Ordering::Greater)
        .unwrap_or(0)
}

/// 从给定下标起绕一圈的全部路径（顺序模式缺失恢复的候选顺序）。
pub fn rotation_from(sorted: &[PathBuf], start: usize) -> Vec<PathBuf> {
    (0..sorted.len())
        .map(|step| sorted[(start + step) % sorted.len()].clone())
        .collect()
}

fn is_image_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

fn path_key(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

/// 完整路径自然序：连续数字按数值比较，其余字符按 Unicode 码点比较。
/// 数值相等（仅前导零不同）时继续比较，最终以普通字符串序打破平局，保证全序确定。
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();
    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&ca), Some(&cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let run_a = take_digit_run(&mut a_chars);
                    let run_b = take_digit_run(&mut b_chars);
                    let ord = cmp_digit_runs(&run_a, &run_b);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                } else if ca != cb {
                    return ca.cmp(&cb);
                } else {
                    a_chars.next();
                    b_chars.next();
                }
            }
        }
    }
}

fn take_digit_run(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut run = String::new();
    while let Some(&c) = chars.peek() {
        if !c.is_ascii_digit() {
            break;
        }
        run.push(c);
        chars.next();
    }
    run
}

fn cmp_digit_runs(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}
