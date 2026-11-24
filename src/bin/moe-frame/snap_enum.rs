//! 实例发现的宿主枚举层（多实例吸附专题工单 02）：Win32 调用集中
//! 在这里——按标题枚举顶层窗口、取标题/所属进程 exe/矩形/所在屏，
//! 全部组装成库 snap 的候选事实后交给纯判定筛伙伴。本层不含判定
//! 逻辑（标题与 exe 两层过滤、同屏、求解都在库里）。
//!
//! 调用时机：工单 05/06 的 WM_MOVING 子类化层在拖动开始时枚举
//! 一次物理伙伴（顺带读各伙伴的基座属性组装关系表）、拖动中逐
//! 帧刷新伙伴矩形；锚边/菜单路径在已唤醒的帧上枚举逻辑伙伴
//! （含基座属性），不新增任何唤醒。工单 03 的跟随轮询已随工
//! 单 06 整段拆除。
//!
//! 坐标换算口径：Win32 返回的矩形是物理像素；每扇窗口按**它自己
//! 的** DPI（GetDpiForWindow ÷ 96）换算成逻辑像素后才进候选事实
//! （不同屏 DPI 不同，不能拿被拖窗的比例换算伙伴的矩形）；取不到
//! DPI 时退到被拖窗的 pixels_per_point。所在屏的工作区与显示器矩
//! 形按该窗口所在屏的比例换算（同一窗口与其所在屏共用一个比例）。

use moe_frame::{
    instance::process_alive,
    snap::{is_partner_frame, CandidateWindow, DragSnapPartner, PartnerFrame, WindowRect},
    snap_group::{parse_base_prop, SnapPartner},
};
use windows_sys::core::BOOL;
use windows_sys::Win32::{
    Foundation::{HANDLE, HWND, LPARAM, RECT},
    Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromRect, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    },
    System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            EnumWindows, GetPropA, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
            IsWindowVisible, PostMessageW, RemovePropA, SetPropA, WM_CLOSE,
        },
    },
};

/// 基座关系窗口属性名（工单 06）：从属窗以此属性记其基座
/// HWND。与工单 04 的退出请求标记 `MoeFrame.ExitAllRequested`
/// 错开，互不干扰。值即基座窗口句柄（HANDLE）。
const SNAP_BASE_PROP: &[u8] = b"MoeFrame.SnapBaseHwnd\0";

/// 读一扇窗口的基座属性（跨进程可读）：无属性为 None。纯解
/// 析在库 snap_group::parse_base_prop，本层只做 Win32 读写。
pub(crate) fn read_base(hwnd_value: u64) -> Option<u64> {
    if hwnd_value == 0 {
        return None;
    }
    // SAFETY: 只读该窗口的公开属性；窗口已销毁时 GetPropA 返回
    // null，按无基座处理。
    let raw = unsafe { GetPropA(hwnd_value as usize as HWND, SNAP_BASE_PROP.as_ptr()) };
    parse_base_prop(raw as usize)
}

/// 在一扇窗口上写基座属性（值 = 基座 HWND）。
pub(crate) fn write_base(hwnd_value: u64, base: u64) {
    if hwnd_value == 0 || base == 0 {
        return;
    }
    // SAFETY: 同 read_base；写入失败（窗口已销毁）静默忽略。
    unsafe {
        SetPropA(
            hwnd_value as usize as HWND,
            SNAP_BASE_PROP.as_ptr(),
            base as usize as HANDLE,
        );
    }
}

/// 清除一扇窗口的基座属性（菜单脱离 / 拖从属脱离 / 卫生清理）。
pub(crate) fn clear_base(hwnd_value: u64) {
    if hwnd_value == 0 {
        return;
    }
    // SAFETY: 同 read_base；无属性时 RemovePropA 返回 null 忽略。
    unsafe {
        let _ = RemovePropA(hwnd_value as usize as HWND, SNAP_BASE_PROP.as_ptr());
    }
}

/// 工单 06 的关系枚举（逻辑像素）：筛出带窗口身份与基座属性
/// 的伙伴相框，供锚边邻居派生与菜单出现条件组装。判定仍在库里。
pub(crate) fn relation_partners(own_hwnd: isize, own_scale: f32) -> Vec<SnapPartner> {
    if own_hwnd == 0 {
        return Vec::new();
    }
    let Some(own_exe) = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
    else {
        return Vec::new();
    };
    let mut hwnds: Vec<HWND> = Vec::new();
    // SAFETY: 同 collect_window 的回调口径。
    unsafe {
        let _ = EnumWindows(Some(collect_window), &mut hwnds as *mut Vec<HWND> as LPARAM);
    }
    hwnds
        .into_iter()
        .filter_map(|hwnd| {
            let candidate = candidate_for_window(hwnd, own_hwnd, own_scale, true)?;
            if is_partner_frame(&candidate, &own_exe) {
                let id = (hwnd as usize) as u64;
                Some(SnapPartner {
                    id,
                    frame: PartnerFrame {
                        rect: candidate.rect,
                        monitor: candidate.monitor,
                        work_area: candidate.work_area,
                    },
                    base: read_base(id),
                })
            } else {
                None
            }
        })
        .collect()
}

/// 工单 06 的拖动伙伴全事实（物理像素）：求解用矩形 + 所在屏
/// 物理矩形 + 基座属性，拖动开始时枚举一次组装关系表与临时
/// 成树判定；矩形每帧另经 partner_rect_physical 刷新。
#[derive(Clone, Copy, Debug)]
pub(crate) struct DragPartnerInfo {
    pub(crate) snap: DragSnapPartner,
    pub(crate) monitor: WindowRect,
    /// 该伙伴所在显示器的工作区（物理像素，rcWork 原值）；取
    /// 不到时退回显示器矩形本身。
    pub(crate) work_area: WindowRect,
    pub(crate) base: Option<u64>,
}

pub(crate) fn drag_partner_infos(own_hwnd: isize) -> Vec<DragPartnerInfo> {
    drag_partners(own_hwnd)
        .into_iter()
        .filter_map(|snap| {
            let monitor = monitor_rect_physical(snap.hwnd)?;
            Some(DragPartnerInfo {
                base: read_base(snap.hwnd),
                work_area: work_area_rect_physical(snap.hwnd).unwrap_or(monitor),
                monitor,
                snap,
            })
        })
        .collect()
}

/// 一扇窗口所在显示器的物理矩形（rcMonitor 原值）。
pub(crate) fn monitor_rect_physical(hwnd_value: u64) -> Option<WindowRect> {
    if hwnd_value == 0 {
        return None;
    }
    // SAFETY: MonitorFromWindow 对任意句柄返回最近显示器或 null；
    // GetMonitorInfoW 要求 cbSize 先行填好。
    unsafe {
        let monitor = MonitorFromWindow(hwnd_value as usize as HWND, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let m = info.rcMonitor;
        Some(WindowRect {
            left: m.left as f32,
            top: m.top as f32,
            right: m.right as f32,
            bottom: m.bottom as f32,
        })
    }
}

/// 启动瞬间的活伙伴位置（审查修复 S4）：级联基点的原料——枚举
/// 全部相框窗（含隐藏到托盘的：它仍是「已开相框」），按窗口所
/// 属 pid 查编号锁文件定编号，返回 (编号, 当前左上位置) 列表。
/// 本进程此时尚无窗口，枚举到的都是别家；编号对不上的（锁已
/// 释放等）跳过。选择规则在库 `instance::cascade_base_position`。
pub(crate) fn startup_partner_positions(app_dir: &std::path::Path) -> Vec<(u32, [f32; 2])> {
    let Some(own_exe) = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
    else {
        return Vec::new();
    };
    let mut hwnds: Vec<HWND> = Vec::new();
    // SAFETY: 同 collect_window 的回调口径。
    unsafe {
        let _ = EnumWindows(Some(collect_window), &mut hwnds as *mut Vec<HWND> as LPARAM);
    }
    hwnds
        .into_iter()
        .filter_map(|hwnd| {
            // SAFETY: hwnd 来自本次枚举；GetWindowThreadProcessId
            // 只写 pid，窗口中途销毁时 pid 保持 0、后续校验挡住。
            let mut pid: u32 = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
            }
            if pid == 0 {
                return None;
            }
            let number = (1..=256)
                .find(|n| moe_frame::instance::lock_holder_pid(app_dir, *n) == Some(pid))?;
            let candidate = candidate_for_window(hwnd, 0, 1.0, false)?;
            if is_partner_frame(&candidate, &own_exe) {
                Some((number, [candidate.rect.left, candidate.rect.top]))
            } else {
                None
            }
        })
        .collect()
}

/// 「退出全部相框」的窗口属性名（工单 04）：发起方在伙伴相框窗上
/// 置此属性再发 WM_CLOSE——裸 WM_CLOSE 会被对方的「关闭即隐藏」
/// 口径拦成隐藏，属性是对方区分「退出全部」与普通关闭的依据。这
/// 仍是窗口枚举 + 窗口消息 / 属性的路子，不建任何 IPC 通道。
const EXIT_ALL_PROP: &[u8] = b"MoeFrame.ExitAllRequested\0";

/// 给全部伙伴相框窗发退出请求（工单 04「退出全部相框」）：枚举口
/// 径同工单 02（标题 + exe 校验），但含隐藏到托盘的相框（不可见
/// 窗口也要退）。逐窗置退出属性 + 投递 WM_CLOSE（唤醒对方事件循
/// 环，对方读到属性后走自身正常退出路径）；返回发出请求的伙伴数。
/// 发起方在本函数之后才退出自己（本实例最后退出）。
pub(crate) fn exit_all_partners(own_hwnd: isize, own_scale: f32) -> usize {
    if own_hwnd == 0 {
        return 0;
    }
    let Some(own_exe) = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
    else {
        return 0;
    };
    let mut hwnds: Vec<HWND> = Vec::new();
    // SAFETY: 同 collect_window 的回调口径。
    unsafe {
        let _ = EnumWindows(Some(collect_window), &mut hwnds as *mut Vec<HWND> as LPARAM);
    }
    let mut sent = 0;
    for hwnd in hwnds {
        let Some(candidate) = candidate_for_window(hwnd, own_hwnd, own_scale, false) else {
            continue;
        };
        if !is_partner_frame(&candidate, &own_exe) {
            continue;
        }
        // SAFETY: hwnd 来自本次枚举；SetPropA / PostMessageW 对已销毁
        // 窗口按失败返回处理，不解引用任何指针。属性名是纯 ASCII，
        // 用 A 版属性 API 与字节串指针正好对上。
        unsafe {
            if SetPropA(hwnd, EXIT_ALL_PROP.as_ptr(), 1 as HANDLE) != 0 {
                let _ = PostMessageW(hwnd, WM_CLOSE, 0, 0);
                sent += 1;
            }
        }
    }
    sent
}

/// 本窗是否收到「退出全部」请求（工单 04）：读并清除退出属性，
/// 读到即 true（宿主据此走自身正常退出路径）。每帧顺手查一次，
/// 无请求时只是一次 GetPropW 查询。
pub(crate) fn take_exit_all_request(own_hwnd: isize) -> bool {
    if own_hwnd == 0 {
        return false;
    }
    // SAFETY: 只查本进程自己窗口的属性；取到后立即移除，防重复触发。
    unsafe {
        let hwnd = own_hwnd as HWND;
        if GetPropA(hwnd, EXIT_ALL_PROP.as_ptr()).is_null() {
            false
        } else {
            let _ = RemovePropA(hwnd, EXIT_ALL_PROP.as_ptr());
            true
        }
    }
}

/// 本窗所在屏的（显示器矩形，工作区矩形），逻辑像素：跟随引擎组
/// 装本窗事实时用。
pub(crate) fn own_monitor_work_area(
    own_hwnd: isize,
    own_scale: f32,
) -> Option<(WindowRect, WindowRect)> {
    if own_hwnd == 0 {
        return None;
    }
    monitor_rects(own_hwnd as HWND, own_scale)
}

/// 工单 05 的拖动伙伴（物理像素口径）：拖动开始时枚举一次——
/// 校验判定复用逻辑口径的候选事实（标题 + exe + 存活 + 非自己），
/// 矩形另取 GetWindowRect 的物理像素原值并带上句柄身份，供
/// WM_MOVING 求解与工单 06 的命中接缝消费。
pub(crate) fn drag_partners(own_hwnd: isize) -> Vec<DragSnapPartner> {
    if own_hwnd == 0 {
        return Vec::new();
    }
    let Some(own_exe) = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
    else {
        return Vec::new();
    };
    let mut hwnds: Vec<HWND> = Vec::new();
    // SAFETY: 同 collect_window 的回调口径。
    unsafe {
        let _ = EnumWindows(Some(collect_window), &mut hwnds as *mut Vec<HWND> as LPARAM);
    }
    hwnds
        .into_iter()
        .filter_map(|hwnd| {
            let candidate = candidate_for_window(hwnd, own_hwnd, 1.0, true)?;
            if !is_partner_frame(&candidate, &own_exe) {
                return None;
            }
            let rect = partner_rect_physical((hwnd as usize) as u64)?;
            Some(DragSnapPartner {
                hwnd: (hwnd as usize) as u64,
                rect,
            })
        })
        .collect()
}

/// 一扇窗口当前的物理像素矩形（GetWindowRect 原值，不做 DPI
/// 换算）：WM_MOVING 每帧刷新伙伴位置用；窗口已销毁时返回 None。
pub(crate) fn partner_rect_physical(hwnd_value: u64) -> Option<WindowRect> {
    if hwnd_value == 0 {
        return None;
    }
    let mut rect = RECT::default();
    // SAFETY: hwnd_value 是枚举/会话里记录的窗口句柄值；GetWindowRect
    // 只写出矩形，窗口已销毁时按失败返回处理。
    if unsafe { GetWindowRect(hwnd_value as usize as HWND, &mut rect) } == 0 {
        return None;
    }
    Some(WindowRect {
        left: rect.left as f32,
        top: rect.top as f32,
        right: rect.right as f32,
        bottom: rect.bottom as f32,
    })
}

/// 一扇窗口所在显示器的工作区（物理像素，rcWork 原值）：
/// MonitorFromWindow + GetMonitorInfoW，与 monitor_rect_physical
/// 同路、只换取 rcWork。
pub(crate) fn work_area_rect_physical(hwnd_value: u64) -> Option<WindowRect> {
    if hwnd_value == 0 {
        return None;
    }
    // SAFETY: 同 monitor_rect_physical 的口径。
    unsafe {
        let monitor = MonitorFromWindow(hwnd_value as usize as HWND, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let w = info.rcWork;
        Some(WindowRect {
            left: w.left as f32,
            top: w.top as f32,
            right: w.right as f32,
            bottom: w.bottom as f32,
        })
    }
}

/// 拟拖矩形所在显示器的矩形（物理像素，rcMonitor 原值）：
/// MonitorFromRect 取与拟拖矩形交叠最多的显示器。WM_MOVING
/// 每帧的同屏过滤以它为准（审查修复 F3）。
pub(crate) fn monitor_rect_for_physical_rect(rect: WindowRect) -> Option<WindowRect> {
    let raw = RECT {
        left: rect.left as i32,
        top: rect.top as i32,
        right: rect.right as i32,
        bottom: rect.bottom as i32,
    };
    // SAFETY: 同 work_area_for_physical_rect 的口径，只换取 rcMonitor。
    unsafe {
        let monitor = MonitorFromRect(&raw, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let m = info.rcMonitor;
        Some(WindowRect {
            left: m.left as f32,
            top: m.top as f32,
            right: m.right as f32,
            bottom: m.bottom as f32,
        })
    }
}

/// 拟拖矩形所在显示器的工作区（物理像素）：MonitorFromRect 取
/// 与拟拖矩形交叠最多的显示器，rcWork 原值返回。
pub(crate) fn work_area_for_physical_rect(rect: WindowRect) -> Option<WindowRect> {
    let raw = RECT {
        left: rect.left as i32,
        top: rect.top as i32,
        right: rect.right as i32,
        bottom: rect.bottom as i32,
    };
    // SAFETY: MonitorFromRect 对任意矩形返回最近显示器或 null；
    // GetMonitorInfoW 要求 cbSize 先行填好。
    unsafe {
        let monitor = MonitorFromRect(&raw, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let work = info.rcWork;
        Some(WindowRect {
            left: work.left as f32,
            top: work.top as f32,
            right: work.right as f32,
            bottom: work.bottom as f32,
        })
    }
}

/// 本窗 DPI（GetDpiForWindow 原值，96 = 100%）：阈值物理换算用。
pub(crate) fn own_dpi(own_hwnd: isize) -> u32 {
    if own_hwnd == 0 {
        return 0;
    }
    // SAFETY: 只读本窗 DPI。
    unsafe { GetDpiForWindow(own_hwnd as HWND) }
}

unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam 由各枚举调用方传入，指向其栈上的 Vec，
    // 枚举期间始终存活。
    let hwnds = unsafe { &mut *(lparam as *mut Vec<HWND>) };
    hwnds.push(hwnd);
    1
}

/// 一扇窗口的候选事实；不可见或矩形/所在屏取不到的窗口跳过。
fn candidate_for_window(
    hwnd: HWND,
    own_hwnd: isize,
    own_scale: f32,
    require_visible: bool,
) -> Option<CandidateWindow> {
    // SAFETY: hwnd 来自 EnumWindows 的本次枚举结果；以下调用都只读
    // 该窗口的公开属性，窗口在调用间被销毁时各 API 按失败返回处理。
    unsafe {
        if require_visible && IsWindowVisible(hwnd) == 0 {
            return None;
        }
        let scale = window_scale(hwnd, own_scale);
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return None;
        }
        let (monitor, work_area) = monitor_rects(hwnd, scale)?;
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        Some(CandidateWindow {
            title: window_title(hwnd),
            exe_path: process_exe_path(pid),
            owner_alive: pid != 0 && process_alive(pid),
            is_self_window: hwnd as isize == own_hwnd,
            rect: logical_rect(rect, scale),
            monitor,
            work_area,
        })
    }
}

/// 窗口标题原文（取不到时为空串，判定层自然挡下）。
unsafe fn window_title(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    // SAFETY: buffer 长度即第三参，写入不超过 buffer。
    let len = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..len as usize])
}

/// 所属进程的 exe 完整路径（取不到时为 None，判定层不入列）。
fn process_exe_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    // SAFETY: 只读查询目标进程的镜像路径；句柄用完即关。
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size);
        let _ = windows_sys::Win32::Foundation::CloseHandle(handle);
        if ok == 0 || size == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..size as usize]))
    }
}

/// 一扇窗口所在屏的（显示器矩形，工作区矩形），均已换算为逻辑像素。
fn monitor_rects(hwnd: HWND, scale: f32) -> Option<(WindowRect, WindowRect)> {
    // SAFETY: MonitorFromWindow 对任意句柄返回最近显示器或 null；
    // GetMonitorInfoW 要求 cbSize 先行填好。
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        Some((
            logical_rect(info.rcMonitor, scale),
            logical_rect(info.rcWork, scale),
        ))
    }
}

/// 窗口自身的 DPI 比例（物理 ÷ 逻辑）；取不到时退到被拖窗的比例。
unsafe fn window_scale(hwnd: HWND, fallback: f32) -> f32 {
    // SAFETY: GetDpiForWindow 只读该窗口的 DPI。
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi > 0 {
        dpi as f32 / 96.0
    } else if fallback > 0.0 {
        fallback
    } else {
        1.0
    }
}

/// 物理像素矩形 → 逻辑像素矩形（本模块唯一的换算点）。
fn logical_rect(rect: RECT, scale: f32) -> WindowRect {
    WindowRect {
        left: rect.left as f32 / scale,
        top: rect.top as f32 / scale,
        right: rect.right as f32 / scale,
        bottom: rect.bottom as f32 / scale,
    }
}
