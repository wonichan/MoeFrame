//! 实例启动装配（多实例吸附专题工单 01/04，审查修复 J3 从
//! moe-frame.rs 的 run() 抽出）：命令行指定编号的解析与认领、
//! 级联基点的实时枚举、自启恢复的裁决与拉起。run() 只分发：
//! 认领 → 取级联基点 → 交 config_sync 装配 → 置运行标记 → 恢复。

use std::{io, path::Path};

use moe_frame::{
    config_sync::ConfigSync,
    instance::{
        cascade_base_position, claim_instance, claim_specific_instance, lock_holder_pid,
        process_alive, recovery_targets, InstanceClaim, RecoverySection,
    },
};

use crate::{snap_enum, startup_marker, tray};

/// 命令行 `--instance <编号>` 的解析：自启恢复拉起的进程带指定
/// 编号；无此参数（或值非法）返回 None，走最小空缺认领。
fn requested_instance_number() -> Option<u32> {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|pair| pair[0] == "--instance")
        .and_then(|pair| pair[1].parse::<u32>().ok())
        .filter(|number| *number >= 1)
}

/// 按命令行认领本进程的实例编号：指定编号走受控的指定认领，
/// 已被存活进程持有时返回 Ok(None)——调用方安静退出（恢复是
/// 幂等的，绝不重复拉起真在运行的实例）；未指定时认领最小
/// 空缺编号。
pub(crate) fn claim_from_args(directory: &Path) -> io::Result<Option<InstanceClaim>> {
    match requested_instance_number() {
        Some(number) => match claim_specific_instance(directory, number) {
            Ok(claim) => Ok(Some(claim)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(None),
            Err(error) => Err(error),
        },
        None => claim_instance(directory).map(Some),
    }
}

/// 级联基点（审查修复 S4）：启动瞬间枚举活伙伴的位置，按库的
/// 选择规则（编号最大者）定「上一扇已开相框」的当前位置；无活
/// 伙伴时返回 None，装配层兜底回落盘段口径。
pub(crate) fn live_cascade_base(directory: &Path) -> Option<[f32; 2]> {
    cascade_base_position(&snap_enum::startup_partner_positions(directory))
}

/// 自启恢复（工单 04，协调者定死口径）：任何 1 号实例启动时都
/// 尝试恢复（不区分自启 / 手动启动），且幂等——只拉起「实例段
/// 有在运行标记、且该编号当前没有活进程持有锁」的编号（编号
/// 锁 + pid 存活判断区分崩溃残留与真在运行），拉起的新进程经
/// `--instance` 认领指定编号、回到各自落盘位置。
pub(crate) fn recover_marked_instances(directory: &Path, sync: &ConfigSync) {
    let sections: Vec<RecoverySection> = sync
        .instance_sections()
        .iter()
        .map(|section| RecoverySection {
            number: section.number,
            running: section.running,
            holder_pid: lock_holder_pid(directory, section.number),
        })
        .collect();
    for target in recovery_targets(&sections, 1, &process_alive) {
        startup_marker("autostart-recovery", &format!("instance={target}"));
        tray::spawn_frame_with_number(target);
    }
}
