//! 实例编号认领（多实例吸附专题工单 01）：进程启动时在 app 目录用
//! 编号锁文件认领最小空缺编号，退出释放，编号即实例身份。
//!
//! 锁文件口径：文件名为 `instance-<编号>.lock`，内容为持有者 pid。
//! 认领用 `create_new` 原子占位，两进程同时抢同一编号只有一个成功。
//! 已存在的锁先读持有者 pid 做存活判断：持有者存活才跳到下一编号；
//! 持有者已死、或锁内容经短暂重读仍不是合法 pid（写一半崩溃等），
//! 一律视为陈旧锁——删掉后重试同一编号，绝不把崩溃残留当永久
//! 占用（重读是防 create_new 与写 pid 之间的空窗误抢，见 lock_is_stale）。存活判断经
//! [`process_alive`] 接缝供给：生产口径按 pid 查进程是否在世，本
//! 工单只做到「残留锁可回收」；工单 04 的自启恢复若需更强的存活
//! 口径，复用同一接缝替换判断，不改认领流程。
//!
//! 级联初始位置也在这里：无落盘几何的新实例相对上一扇已开相框
//! +24/+24（横纵各 24 逻辑 px）；有落盘几何时严格按落盘开。级联
//! 只是初始放置，仍走既有几何机的工作区钳制，不是新的定位规则。

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

/// 级联偏移（逻辑 px）：新实例相对上一扇已开相框的横纵偏移。
pub const CASCADE_OFFSET: f32 = 24.0;

/// 认领扫描的编号上限：只防极端情况下的无界循环，远超任何实际开窗数。
const MAX_INSTANCE_NUMBER: u32 = 10_000;

/// 编号锁文件名：app 目录下 `instance-<编号>.lock`。
pub fn lock_file_name(number: u32) -> String {
    format!("instance-{number}.lock")
}

/// 初始位置裁决：有落盘位置严格用落盘值；没有落盘位置且有上一扇
/// 的位置时，相对上一扇级联 +24/+24；两者皆无时返回 None，交给
/// 宿主既有默认放置。
pub fn initial_position(saved: Option<[f32; 2]>, previous: Option<[f32; 2]>) -> Option<[f32; 2]> {
    saved.or_else(|| previous.map(|pos| [pos[0] + CASCADE_OFFSET, pos[1] + CASCADE_OFFSET]))
}

/// pid 存活判断（生产口径）：pid 0 与不存在的进程为死。Unix 查
/// `/proc/<pid>`；Windows 用 OpenProcess + GetExitCodeProcess 看
/// 是否仍是 STILL_ACTIVE；其余平台保守按存活处理（宁可跳号，
/// 不抢活人的编号）。
pub fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        const STILL_ACTIVE: u32 = 259;
        // SAFETY: 两个调用都只读目标进程状态；句柄用完即关。
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            let mut code: u32 = 0;
            let ok = GetExitCodeProcess(handle, &mut code);
            let _ = windows_sys::Win32::Foundation::CloseHandle(handle);
            ok != 0 && code == STILL_ACTIVE
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

/// 一次成功的编号认领：持有期间锁文件存在；`release` 或 Drop 时
/// 删除锁文件、编号回到空缺池。删除失败不 panic（进程退出后残留
/// 锁会按陈旧锁口径被下一实例回收）。
pub struct InstanceClaim {
    number: u32,
    lock_path: PathBuf,
    released: bool,
}

impl InstanceClaim {
    /// 本实例认领到的实例编号。
    pub fn number(&self) -> u32 {
        self.number
    }

    /// 显式释放编号（删锁文件）。Drop 也会释放，这里供宿主在退出
    /// 路径上把时点写明。
    pub fn release(mut self) {
        self.remove_lock();
    }

    fn remove_lock(&mut self) {
        if !self.released {
            self.released = true;
            let _ = fs::remove_file(&self.lock_path);
        }
    }
}

impl Drop for InstanceClaim {
    fn drop(&mut self) {
        self.remove_lock();
    }
}

/// 生产认领：以本进程 pid 占位，存活判断用 [`process_alive`]。
pub fn claim_instance(dir: &Path) -> io::Result<InstanceClaim> {
    claim_instance_with(dir, std::process::id(), &process_alive)
}

/// 认领最小空缺编号：从 1 起逐个尝试 `create_new` 占位；撞上已
/// 存在的锁时按陈旧口径裁决（持有者已死或内容非法 → 删后重试同
/// 一编号；持有者存活 → 下一编号）。pid 与存活判断经参数注入，
/// 供测试以假进程复现并发与残留场景。
pub fn claim_instance_with(
    dir: &Path,
    pid: u32,
    is_alive: &dyn Fn(u32) -> bool,
) -> io::Result<InstanceClaim> {
    fs::create_dir_all(dir)?;
    let mut number = 1u32;
    while number <= MAX_INSTANCE_NUMBER {
        let lock_path = dir.join(lock_file_name(number));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                if let Err(error) = write!(file, "{pid}") {
                    let _ = fs::remove_file(&lock_path);
                    return Err(error);
                }
                return Ok(InstanceClaim {
                    number,
                    lock_path,
                    released: false,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if lock_is_stale(&lock_path, is_alive) {
                    // 陈旧锁：删掉后重试同一编号。若删除失败是因为
                    // 另一个认领者刚抢先删掉/重建，同样重试本编号，
                    // 由下一轮 create_new 与存活判断重新裁决；只有
                    // 其他删除错误才跳号，避免在本编号上空转。
                    match fs::remove_file(&lock_path) {
                        Ok(()) => continue,
                        Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                        Err(_) => number += 1,
                    }
                } else {
                    number += 1;
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("no free instance number"))
}

/// 已存在锁的陈旧裁决：持有者已死算陈旧；只有「内容是合法 pid
/// 且持有者存活」才算真占用。内容读不出合法 pid 时不能立即判
/// 陈旧——认领者是先 create_new 占位、紧接着才写 pid，中间有一段
/// 文件已存在但内容为空的窗口，误判会把活人的锁抢走（并发认领
/// 测试实测复现）。故先自旋重读一小段时间：活进程的写入在这段
/// 时间内必然落地；超时仍读不出合法 pid 的，才是写一半崩溃留下
/// 的残锁，按陈旧处理。
fn lock_is_stale(lock_path: &Path, is_alive: &dyn Fn(u32) -> bool) -> bool {
    for attempt in 0..200 {
        match fs::read_to_string(lock_path) {
            Ok(text) => {
                if let Ok(holder) = text.trim().parse::<u32>() {
                    return !is_alive(holder);
                }
            }
            // 文件刚被删（持有者释放或残锁被别人清走）：按陈旧
            // 处理，调用方删后重试同一编号会重新裁决。
            Err(error) if error.kind() == io::ErrorKind::NotFound => return true,
            Err(_) => {}
        }
        if attempt == 0 {
            // 先让一步：多数空窗在一次调度内就被写入填上。
            std::thread::yield_now();
        } else {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    true
}
