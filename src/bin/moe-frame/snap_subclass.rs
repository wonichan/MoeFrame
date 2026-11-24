//! WM_MOVING 实时吸附的子类化薄壳（多实例吸附专题工单 05）：
//! 只给相框窗口 HWND 挂 SetWindowSubclass，在原生模态拖动循环的
//! WM_MOVING 阶段以拟拖矩形 + 伙伴矩形 + 工作区做吸附求解，命中
//! 就地改写 lParam 指向的 RECT，窗口在拖动过程中就被吸住。求解
//! 纯逻辑在库 snap，本层只做枚举供给、坐标换算与改矩形。
//!
//! 线程模型（本票核心约束）：子类化回调没有 &mut FrameApp 访问
//! ——回调由窗口所在线程（即 winit 事件循环的 UI 线程）在消息泵
//! 里触发，与 FrameApp 的帧回调同线程但不同调用栈。故回调需要
//! 的数据走进程级静态：开关值用 AtomicBool（宿主在设置窗/全局
//! 同步改值时同步写入），拖动会话用 Mutex<Option<DragSession>>
//! （每进程只有一扇相框窗，会话至多一份；Mutex 只为静态可 Sync，
//! 实际访问都在 UI 线程、无竞争）。枚举与 Win32 读取直接在回调
//! 里经 snap_enum 完成，不回宿主取数据。
//!
//! 与既有原生手势机器的分工：本层不碰 NativeGesture 旗标、模态
//! 循环结束判定（GetAsyncKeyState）与松手重校正/回弹路径；所有
//! 消息一律经 DefSubclassProc 转发给 winit 自有窗口过程，本层只
//! 在 WM_MOVING 转发前改写矩形。几何机在手势中照旧沉默，松手后
//! 以实际观察吸收新位置。
//!
//! 工单 06 接缝：拖动会话里保存最近一帧求解的贴合命中
//! （DragSession::hits，库 snap 的 SnapHit 列表）与目标矩形；工
//! 单 06 在此会话上扩展「命中伙伴记录 → 属性关系 + 子树搬运」
//! 钩子（WM_EXITSIZEMOVE 清会话前消费 hits）。本票不设窗口属
//! 性、不搬伙伴。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use moe_frame::snap::{
    physical_snap_threshold, solve_drag_snap, DragSnapPartner, SnapHit, WindowRect,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
    UI::WindowsAndMessaging::{WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_MOVING},
};

use super::snap_enum;

/// 子类化标识（本进程内固定一份）。
const SUBCLASS_ID: usize = 1;

/// 窗口吸附开关的回调侧镜像：宿主改值时经 set_enabled 同步。
static SNAP_ENABLED: AtomicBool = AtomicBool::new(false);

/// 当前拖动会话（WM_ENTERSIZEMOVE 建、WM_EXITSIZEMOVE 清）。
static DRAG_SESSION: Mutex<Option<DragSession>> = Mutex::new(None);

/// 一次原生拖动的会话状态：开始时枚举一次的伙伴（矩形每帧经
/// GetWindowRect 刷新）、最近一帧的贴合命中与目标矩形。工单 06
/// 在此扩展子树搬运钩子。
pub(crate) struct DragSession {
    pub(crate) partners: Vec<DragSnapPartner>,
    pub(crate) hits: Vec<SnapHit>,
    pub(crate) last_target: Option<WindowRect>,
}

/// 同步窗口吸附开关值给回调侧（宿主 apply_window_snap_value 与
/// 安装时调用）。关闭后回调对 WM_MOVING 直接放行、不求解。
pub(crate) fn set_enabled(enabled: bool) {
    SNAP_ENABLED.store(enabled, Ordering::Relaxed);
}

/// 给相框窗口安装子类化（窗口创建后、装配 FrameApp 时调用一次）。
pub(crate) fn install(hwnd: isize, enabled: bool) {
    if hwnd == 0 {
        return;
    }
    set_enabled(enabled);
    // SAFETY: hwnd 是本进程相框窗口的句柄（装配处经 raw-window-
    // handle 取得）；子类化过程只转发消息，见 snap_subclass_proc。
    unsafe {
        SetWindowSubclass(hwnd as HWND, Some(snap_subclass_proc), SUBCLASS_ID, 0);
    }
}

/// 退出前卸载子类化（宿主正常退出路径调用）。
pub(crate) fn uninstall(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    // SAFETY: 与 install 同一 hwnd 与标识。
    unsafe {
        RemoveWindowSubclass(hwnd as HWND, Some(snap_subclass_proc), SUBCLASS_ID);
    }
    if let Ok(mut session) = DRAG_SESSION.lock() {
        *session = None;
    }
}

/// 子类化窗口过程：WM_ENTERSIZEMOVE / WM_MOVING / WM_EXITSIZEMOVE
/// 三条消息做吸附处理，其余一律转发。无论是否处理，所有消息最
/// 终都经 DefSubclassProc 交回 winit 自有链。
unsafe extern "system" fn snap_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid: usize,
    _ref_data: usize,
) -> LRESULT {
    if SNAP_ENABLED.load(Ordering::Relaxed) {
        match msg {
            WM_ENTERSIZEMOVE => {
                let partners = snap_enum::drag_partners(hwnd as isize);
                if let Ok(mut session) = DRAG_SESSION.lock() {
                    *session = Some(DragSession {
                        partners,
                        hits: Vec::new(),
                        last_target: None,
                    });
                }
            }
            WM_MOVING => {
                handle_moving(hwnd, lparam);
            }
            WM_EXITSIZEMOVE => {
                // 工单 06 接缝：清会话前在此消费 session.hits 做关
                // 系记录；本票只清会话。
                if let Ok(mut session) = DRAG_SESSION.lock() {
                    *session = None;
                }
            }
            _ => {}
        }
    } else if msg == WM_EXITSIZEMOVE {
        if let Ok(mut session) = DRAG_SESSION.lock() {
            *session = None;
        }
    }
    // SAFETY: 转发参数原样来自系统分发；DefSubclassProc 是子类化
    // 链的标准下一环（winit 自有过程）。
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// WM_MOVING 一帧：lparam 指向拟拖 RECT（物理像素）——刷新伙伴
/// 矩形、取拟拖矩形所在屏工作区、按本窗 DPI 换算阈值求解，命中
/// 就地改写 RECT 并把贴合命中存入会话。
fn handle_moving(hwnd: HWND, lparam: LPARAM) {
    if lparam == 0 {
        return;
    }
    let Ok(mut guard) = DRAG_SESSION.lock() else {
        return;
    };
    let Some(session) = guard.as_mut() else {
        return;
    };
    // 伙伴矩形逐帧刷新（伙伴可能在本窗拖动期间被外力移动）；已
    // 销毁的伙伴剔除。
    session.partners.retain_mut(
        |partner| match snap_enum::partner_rect_physical(partner.hwnd) {
            Some(rect) => {
                partner.rect = rect;
                true
            }
            None => false,
        },
    );
    // SAFETY: WM_MOVING 的 lparam 按 Win32 约定指向系统持有的
    // RECT（拟拖窗口矩形，物理像素），在本次回调内读写合法。
    let rect_ptr = lparam as *mut RECT;
    let proposed = unsafe {
        WindowRect {
            left: (*rect_ptr).left as f32,
            top: (*rect_ptr).top as f32,
            right: (*rect_ptr).right as f32,
            bottom: (*rect_ptr).bottom as f32,
        }
    };
    let work_area = snap_enum::work_area_for_physical_rect(proposed);
    let threshold = physical_snap_threshold(snap_enum::own_dpi(hwnd as isize));
    let Some(outcome) = solve_drag_snap(proposed, &session.partners, work_area, threshold) else {
        session.hits.clear();
        session.last_target = None;
        return;
    };
    session.hits.clone_from(&outcome.hits);
    session.last_target = Some(outcome.target);
    if outcome.target != proposed {
        // SAFETY: 同上，改写拟拖矩形即改写模态循环将采用的位置。
        unsafe {
            (*rect_ptr).left = outcome.target.left as i32;
            (*rect_ptr).top = outcome.target.top as i32;
            (*rect_ptr).right = outcome.target.right as i32;
            (*rect_ptr).bottom = outcome.target.bottom as i32;
        }
    }
}
