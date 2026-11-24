//! 多实例吸附专题工单 01：实例编号认领（编号锁文件）的真实文件测试。
//!
//! 处置口径（工单 Comments 同步记录）：锁文件名为 app 目录下
//! `instance-<编号>.lock`，内容为持有者 pid。认领时用 create_new
//! 原子占位；已存在的锁先读 pid 并做存活判断——持有者已死或内容
//! 不是合法 pid 一律视为陈旧锁，删除后重试同一编号（不把崩溃残留
//! 当永久占用）；持有者存活才跳到下一编号。存活判断经接缝注入，
//! 本文件用假判断保持确定性；生产口径 `process_alive` 与工单 04 的
//! 自启恢复共用同一接缝。

mod common;

use common::temp_root;
use moe_frame::instance::{
    cascade_base_position, claim_instance_with, initial_position, process_alive,
};
use std::{fs, sync::Mutex};

#[test]
fn claims_take_the_smallest_free_numbers_in_order() {
    let dir = temp_root("instance-claim", "order");
    // “锁里 pid 即存活”的判断口径模拟两个同时在跑的进程。
    let is_alive = |_pid: u32| true;
    let first = claim_instance_with(&dir, 101, &is_alive).unwrap();
    let second = claim_instance_with(&dir, 102, &is_alive).unwrap();
    assert_eq!(first.number(), 1);
    assert_eq!(second.number(), 2);
    // 锁文件内容是持有者 pid。
    assert_eq!(
        fs::read_to_string(dir.join("instance-1.lock")).unwrap(),
        "101"
    );
    assert_eq!(
        fs::read_to_string(dir.join("instance-2.lock")).unwrap(),
        "102"
    );
}

#[test]
fn a_released_number_is_reused_as_the_smallest_gap() {
    let dir = temp_root("instance-claim", "reuse");
    let is_alive = |_pid: u32| true;
    let first = claim_instance_with(&dir, 101, &is_alive).unwrap();
    let second = claim_instance_with(&dir, 102, &is_alive).unwrap();
    let third = claim_instance_with(&dir, 103, &is_alive).unwrap();
    assert_eq!((first.number(), second.number(), third.number()), (1, 2, 3));
    // 第二个实例退出释放 2 号：下一个认领复用最小空缺 2，而不是 4。
    second.release();
    assert!(!dir.join("instance-2.lock").exists());
    let fourth = claim_instance_with(&dir, 104, &is_alive).unwrap();
    assert_eq!(fourth.number(), 2);
    // 1 号释放后同样被复用。
    first.release();
    let fifth = claim_instance_with(&dir, 105, &is_alive).unwrap();
    assert_eq!(fifth.number(), 1);
}

#[test]
fn dropping_a_claim_releases_its_number() {
    let dir = temp_root("instance-claim", "drop");
    let is_alive = |_pid: u32| true;
    {
        let claim = claim_instance_with(&dir, 101, &is_alive).unwrap();
        assert_eq!(claim.number(), 1);
    }
    assert!(!dir.join("instance-1.lock").exists());
    let claim = claim_instance_with(&dir, 102, &is_alive).unwrap();
    assert_eq!(claim.number(), 1);
}

#[test]
fn a_stale_lock_from_a_dead_holder_is_reclaimed_not_forever_occupied() {
    let dir = temp_root("instance-claim", "stale");
    // 崩溃残留：锁还在，但持有者 pid 已不存在。
    fs::write(dir.join("instance-1.lock"), "999").unwrap();
    let is_alive = |pid: u32| pid == 101;
    let claim = claim_instance_with(&dir, 101, &is_alive).unwrap();
    assert_eq!(claim.number(), 1, "残留锁不得把 1 号永久占住");
    assert_eq!(
        fs::read_to_string(dir.join("instance-1.lock")).unwrap(),
        "101",
        "认领后锁内容换成新持有者"
    );
}

#[test]
fn a_corrupt_lock_file_is_treated_as_stale() {
    let dir = temp_root("instance-claim", "corrupt");
    fs::write(dir.join("instance-1.lock"), "not-a-pid").unwrap();
    let is_alive = |_pid: u32| true;
    let claim = claim_instance_with(&dir, 101, &is_alive).unwrap();
    assert_eq!(claim.number(), 1);
}

#[test]
fn a_live_holders_lock_is_never_stolen() {
    let dir = temp_root("instance-claim", "live");
    fs::write(dir.join("instance-1.lock"), "555").unwrap();
    let is_alive = |pid: u32| pid == 555 || pid == 101;
    let claim = claim_instance_with(&dir, 101, &is_alive).unwrap();
    assert_eq!(claim.number(), 2);
    assert_eq!(
        fs::read_to_string(dir.join("instance-1.lock")).unwrap(),
        "555",
        "存活持有者的锁内容不动"
    );
}

#[test]
fn simultaneous_claims_never_share_a_number() {
    let dir = temp_root("instance-claim", "race");
    let claimed = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for index in 0..8u32 {
            let dir = &dir;
            let claimed = &claimed;
            scope.spawn(move || {
                // 每个线程是一个独立“进程”：pid 各异，且彼此存活。
                let claim = claim_instance_with(dir, 1000 + index, &|_pid| true).unwrap();
                claimed.lock().unwrap().push(claim.number());
                // 保持认领到全部线程结束，避免释放后被复用干扰断言。
                std::thread::sleep(std::time::Duration::from_millis(50));
                claim.release();
            });
        }
    });
    let mut numbers = claimed.into_inner().unwrap();
    numbers.sort_unstable();
    numbers.dedup();
    assert_eq!(numbers.len(), 8, "8 个并发认领必须互不重号：{numbers:?}");
}

#[test]
fn the_real_process_alive_check_recognizes_this_process() {
    assert!(process_alive(std::process::id()));
    assert!(!process_alive(0));
    assert!(!process_alive(u32::MAX));
}

#[test]
fn initial_position_prefers_saved_then_cascades_from_the_previous_frame() {
    // 有落盘几何：严格按落盘，不偏移。
    assert_eq!(
        initial_position(Some([10.0, 20.0]), Some([100.0, 100.0])),
        Some([10.0, 20.0])
    );
    // 无落盘几何：相对上一扇 +24/+24 级联。
    assert_eq!(
        initial_position(None, Some([100.0, 100.0])),
        Some([124.0, 124.0])
    );
    // 没有上一扇可参照：交给既有默认放置。
    assert_eq!(initial_position(None, None), None);
}

#[test]
fn cascade_base_picks_the_largest_numbered_live_partner() {
    // 审查修复 S4：级联基点按实时枚举选——编号最大者（最近开启
    // 的已开相框），与输入顺序无关；无活伙伴返回 None 供兜底。
    assert_eq!(cascade_base_position(&[]), None);
    assert_eq!(
        cascade_base_position(&[(1, [100.0, 100.0])]),
        Some([100.0, 100.0])
    );
    assert_eq!(
        cascade_base_position(&[
            (3, [300.0, 300.0]),
            (1, [100.0, 100.0]),
            (2, [200.0, 200.0])
        ]),
        Some([300.0, 300.0])
    );
}

// ---------- 工单 04：认领指定编号与自启恢复裁决 ----------

use moe_frame::instance::{
    claim_specific_instance_with, lock_holder_pid, recovery_targets, RecoverySection,
};

#[test]
fn claim_specific_takes_the_requested_number_and_leaves_gaps_free() {
    let dir = temp_root("instance-claim", "specific");
    let is_alive = |_pid: u32| true;
    let third = claim_specific_instance_with(&dir, 3, 103, &is_alive).unwrap();
    assert_eq!(third.number(), 3);
    // 指定认领不占最小空缺：普通认领仍从 1 开始。
    let first = claim_instance_with(&dir, 101, &is_alive).unwrap();
    assert_eq!(first.number(), 1);
}

#[test]
fn claim_specific_refuses_a_live_holder_and_succeeds_after_release() {
    let dir = temp_root("instance-claim", "specific-live");
    let is_alive = |_pid: u32| true;
    let holder = claim_specific_instance_with(&dir, 2, 102, &is_alive).unwrap();
    let refused = claim_specific_instance_with(&dir, 2, 202, &is_alive);
    assert_eq!(
        refused.err().expect("活进程持有时应拒绝").kind(),
        std::io::ErrorKind::AlreadyExists,
        "活进程持有时绝不重复认领同一编号"
    );
    holder.release();
    let next = claim_specific_instance_with(&dir, 2, 202, &is_alive).unwrap();
    assert_eq!(next.number(), 2);
}

#[test]
fn claim_specific_recovers_a_stale_lock() {
    let dir = temp_root("instance-claim", "specific-stale");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("instance-2.lock"), "999").unwrap();
    let is_alive = |_pid: u32| false;
    let claim = claim_specific_instance_with(&dir, 2, 202, &is_alive).unwrap();
    assert_eq!(claim.number(), 2);
    assert_eq!(lock_holder_pid(&dir, 2), Some(202));
}

#[test]
fn lock_holder_pid_reads_the_holder_or_none() {
    let dir = temp_root("instance-claim", "holder-pid");
    assert_eq!(lock_holder_pid(&dir, 1), None, "无锁文件为 None");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("instance-1.lock"), "4242").unwrap();
    assert_eq!(lock_holder_pid(&dir, 1), Some(4242));
    fs::write(dir.join("instance-1.lock"), "garbage").unwrap();
    assert_eq!(lock_holder_pid(&dir, 1), None, "内容非法为 None");
}

#[test]
fn recovery_targets_only_marked_numbers_without_a_live_holder() {
    let is_alive = |pid: u32| pid == 111;
    let sections = vec![
        RecoverySection {
            number: 1,
            running: true,
            holder_pid: Some(111),
        },
        RecoverySection {
            number: 2,
            running: true,
            holder_pid: Some(222),
        },
        RecoverySection {
            number: 3,
            running: true,
            holder_pid: None,
        },
        RecoverySection {
            number: 4,
            running: false,
            holder_pid: None,
        },
        RecoverySection {
            number: 5,
            running: true,
            holder_pid: Some(111),
        },
    ];
    // 1 号是自己（排除）；2 号持有者已死、3 号锁已失效 → 拉起；
    // 4 号无在运行标记 → 不拉；5 号锁被活进程持有 → 绝不重复拉起。
    assert_eq!(recovery_targets(&sections, 1, &is_alive), vec![2, 3]);
}
