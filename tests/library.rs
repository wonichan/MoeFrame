//! 工单 10：图库扫描与自然排序的真实文件测试（临时目录）。
mod common;

use moe_frame::library::{natural_cmp, scan_library};
use std::{cmp::Ordering, fs, path::PathBuf};

fn touch(path: &PathBuf) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"not-a-real-image").unwrap();
}

#[test]
fn scan_is_recursive_and_filters_extensions_case_insensitively() {
    let root = common::temp_root("moeframe-library-test", "scan");
    touch(&root.join("a.jpg"));
    touch(&root.join("B.JPEG"));
    touch(&root.join("sub/c.PnG"));
    touch(&root.join("sub/deep/d.webp"));
    touch(&root.join("e.bmp"));
    touch(&root.join("f.gif"));
    touch(&root.join("g.txt"));
    touch(&root.join("h.avif"));
    touch(&root.join("no_extension"));
    let found = scan_library(&root);
    let names: Vec<String> = found
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    // Full-path natural order: 'B' (U+0042) sorts before 'a' (U+0061); subfolders after files.
    assert_eq!(
        names,
        ["B.JPEG", "a.jpg", "e.bmp", "f.gif", "c.PnG", "d.webp"]
    );
}

#[test]
fn scan_of_empty_missing_or_file_root_is_empty_not_an_error() {
    let root = common::temp_root("moeframe-library-test", "empty");
    assert!(scan_library(&root).is_empty());
    assert!(scan_library(&root.join("does-not-exist")).is_empty());
    let file = root.join("single.jpg");
    touch(&file);
    assert!(scan_library(&file).is_empty());
}

#[test]
fn scan_handles_chinese_space_and_special_character_paths() {
    let root = common::temp_root("moeframe-library-test", "special");
    touch(&root.join("我的 图库 [1]/封面 & 插图.png"));
    touch(&root.join("我的 图库 [1]/第二张.jpg"));
    let found = scan_library(&root);
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|p| p.is_file()));
}

#[test]
fn natural_sort_orders_digit_runs_numerically() {
    let root = common::temp_root("moeframe-library-test", "natural");
    for name in ["img10.jpg", "img2.jpg", "img1.jpg", "img02b.jpg"] {
        touch(&root.join(name));
    }
    let found = scan_library(&root);
    let names: Vec<String> = found
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["img1.jpg", "img2.jpg", "img02b.jpg", "img10.jpg"]);
}

#[test]
fn natural_cmp_compares_non_digits_by_code_point() {
    assert_eq!(natural_cmp("a2", "a10"), Ordering::Less);
    assert_eq!(natural_cmp("a10", "a2"), Ordering::Greater);
    assert_eq!(natural_cmp("abc", "abc"), Ordering::Equal);
    assert_eq!(natural_cmp("img9", "img10"), Ordering::Less);
    // Non-digit parts compare by Unicode code point, not locale or pinyin.
    assert_eq!(natural_cmp("图a", "图b"), Ordering::Less);
    assert_eq!(natural_cmp("a", "图"), Ordering::Less);
    // Leading zeros do not change the numeric value; the plain string breaks the tie.
    assert_eq!(natural_cmp("a01", "a1"), Ordering::Less);
}
