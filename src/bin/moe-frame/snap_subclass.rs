//! WM_MOVING 实时吸附与领队整组搬运的子类化薄壳（多实例吸附
//! 专题工单 05 / 06）：只给相框窗口 HWND 挂 SetWindowSubclass，
//! 在原生模态拖动循环的 WM_MOVING 阶段以拟拖矩形 + 伙伴矩形 +
//! 工作区做吸附求解，命中就地改写 lParam 指向的 RECT；同时由
//! 被拖进程（领队）以 HDWP 按帧增量原子搬运从属子树，并把贴
//! 合命中记录为方向关系（从属窗上的基座窗口属性，见库
//! snap_group 文件头的时机口径）。纯判定在库 snap / snap_group，
//! 本层只做枚举供给、属性读写、HDWP 与改矩形。
//!
//! 线程模型：子类化回调没有 &mut FrameApp 访问——回调由窗口
//! 所在线程（即 winit 事件循环的 UI 线程）在消息泵里触发。故
//! 回调需要的数据走进程级静态：开关值与 Shift 独走旗标用
//! AtomicBool（宿主同步写入；Shift 采样时机沿工单 03 的手势
//! 开始采样——宿主在 PointerDown 时采样并写入，本层在
//! WM_ENTERSIZEMOVE 读），拖动会话用 Mutex<Option<DragSession>>
//! （每进程只有一扇相框窗，会话至多一份；Mutex 只为静态可
//! Sync，实际访问都在 UI 线程、无竞争）。
//!
//! 与既有原生手势机器的分工：本层不碰 NativeGesture 旗标、模
//! 态循环结束判定（GetAsyncKeyState）与松手重校正/回弹路径；
//! 所有消息一律经 DefSubclassProc 转发给 winit 自有窗口过程。
//! 几何机在手势中照旧沉默，松手后以实际观察吸收新位置；被
//! HDWP 搬走的伙伴进程不跑任何跟随逻辑，其几何机按「位置变
//! 了、尺寸没变」自然吸收（工单 06 审计，见库 window_geom
//! 的回归测试）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use moe_frame::snap::{
    physical_snap_threshold, solve_drag_snap, DragSnapPartner, PartnerFrame, SnapHit, WindowRect,
};
use moe_frame::snap_group::{
    base_relation_valid, choose_base, frame_delta, plan_drag_start, temporary_contact_ids,
    SnapPartner, SnapRelation,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
    UI::WindowsAndMessaging::{
        BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, SWP_NOACTIVATE, SWP_NOSIZE,
        SWP_NOZORDER, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_MOVING,
    },
};

use super::snap_enum;

/// 子类化标识（本进程内固定一份）。
const SUBCLASS_ID: usize = 1;

/// 窗口吸附开关的回调侧镜像：宿主改值时经 set_enabled 同步。
static SNAP_ENABLED: AtomicBool = AtomicBool::new(false);

/// Shift 独走旗标的回调侧镜像：宿主在手势开始（PointerDown）
/// 时采样 Shift 并经 set_solo_drag 写入。
static SOLO_DRAG: AtomicBool = AtomicBool::new(false);

/// 当前拖动会话（WM_ENTERSIZEMOVE 建、WM_EXITSIZEMOVE 清）。
static DRAG_SESSION: Mutex<Option<DragSession>> = Mutex::new(None);

/// 一次原生拖动的会话状态：开始时枚举一次的伙伴（矩形每帧经
/// GetWindowRect 刷新）、本次拖动的搬运集合（拖动开始按关系
/// 树 + 存量接触临时成树定死，拖动中不因新关系改变）、会话
/// 已记录的本窗基座、最近一帧的贴合命中与目标矩形。关系属
/// 性留在窗上、不随会话清除。
pub(crate) struct DragSession {
    pub(crate) partners: Vec<DragSnapPartner>,
    pub(crate) carry: Vec<u64>,
    pub(crate) recorded_base: Option<u64>,
    pub(crate) hits: Vec<SnapHit>,
    pub(crate) last_target: Option<WindowRect>,
}

/// 同步窗口吸附开关值给回调侧（宿主 apply_window_snap_value 与
/// 安装时调用）。关闭后回调对 WM_MOVING 直接放行、不求解。
pub(crate) fn set_enabled(enabled: bool) {
    SNAP_ENABLED.store(enabled, Ordering::Relaxed);
}

/// 同步 Shift 独走采样值给回调侧（宿主手势开始时调用）。
pub(crate) fn set_solo_drag(solo: bool) {
    SOLO_DRAG.store(solo, Ordering::Relaxed);
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
                begin_drag_session(hwnd);
            }
            WM_MOVING => {
                handle_moving(hwnd, lparam);
            }
            WM_EXITSIZEMOVE => {
                // 关系属性留在窗上（下次拖动开始再做卫生校验）；
                // 这里只清会话状态。
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

/// WM_ENTERSIZEMOVE：枚举伙伴组装本地关系表（各伙伴的基座属
/// 性 + 本窗自己的属性），顺序定死为——先卫生校验清失效的
/// 本窗基座属性，再按 Shift 独走 / 本窗有基座裁决清属性，
/// 最后算本次搬运集合（从属子树 + 存量接触临时从属）。
fn begin_drag_session(hwnd: HWND) {
    let own_id = hwnd as usize as u64;
    let infos = snap_enum::drag_partner_infos(hwnd as isize);
    // 关系表：伙伴各自的「从属 → 基座」边 + 本窗自己的边。
    let mut relations: Vec<SnapRelation> = infos
        .iter()
        .filter_map(|info| {
            info.base.map(|base| SnapRelation {
                subordinate: info.snap.hwnd,
                base,
            })
        })
        .collect();
    let mut own_base = snap_enum::read_base(own_id);
    if let Some(base) = own_base {
        relations.push(SnapRelation {
            subordinate: own_id,
            base,
        });
        // 卫生校验：基座须在本次枚举中存活、同屏、间隙不超阈值。
        let threshold = physical_snap_threshold(snap_enum::own_dpi(hwnd as isize));
        let own_rect = snap_enum::partner_rect_physical(own_id);
        let base_info = infos.iter().find(|info| info.snap.hwnd == base);
        let valid = match (
            own_rect,
            base_info,
            snap_enum::monitor_rect_physical(own_id),
        ) {
            (Some(own), Some(info), Some(own_monitor)) => base_relation_valid(
                true,
                own,
                info.snap.rect,
                info.monitor == own_monitor,
                threshold,
            ),
            _ => false,
        };
        if !valid {
            snap_enum::clear_base(own_id);
            relations.retain(|r| r.subordinate != own_id);
            own_base = None;
        }
    }
    // 存量接触临时成树（物理像素口径的接触判定）：无属性关系
    // 的直接接触伙伴临时纳入本次搬运，不写属性。
    let temporary: Vec<u64> = match (
        snap_enum::partner_rect_physical(own_id),
        snap_enum::monitor_rect_physical(own_id),
    ) {
        (Some(own_rect), Some(own_monitor)) => {
            let own_frame = PartnerFrame {
                rect: own_rect,
                monitor: own_monitor,
                work_area: own_monitor,
            };
            let partners: Vec<SnapPartner> = infos
                .iter()
                .map(|info| SnapPartner {
                    id: info.snap.hwnd,
                    frame: PartnerFrame {
                        rect: info.snap.rect,
                        monitor: info.monitor,
                        work_area: info.monitor,
                    },
                    base: info.base,
                })
                .collect();
            temporary_contact_ids(own_id, own_frame, &partners, &relations)
        }
        _ => Vec::new(),
    };
    let solo = SOLO_DRAG.load(Ordering::Relaxed);
    let plan = plan_drag_start(own_id, &relations, own_base.is_some(), solo, &temporary);
    if plan.clear_own_base {
        // Shift 独走或拖从属：先脱离基座（清本窗基座属性）。
        snap_enum::clear_base(own_id);
        own_base = None;
    }
    if let Ok(mut session) = DRAG_SESSION.lock() {
        *session = Some(DragSession {
            partners: infos.iter().map(|info| info.snap).collect(),
            carry: plan.carry,
            recorded_base: own_base,
            hits: Vec::new(),
            last_target: None,
        });
    }
}

/// WM_MOVING 一帧：刷新伙伴矩形、求解吸附并改写拟拖 RECT、
/// 按命中记录关系，最后以帧增量用 HDWP 搬运搬运集合。
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
    let own_id = hwnd as usize as u64;
    // 本窗当前实际左上（帧首以自家 HWND 的 GetWindowRect 取得，
    // 物理像素）：帧增量的减数。
    let own_actual = snap_enum::partner_rect_physical(own_id).map(|rect| rect.pos());
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
    let outcome = solve_drag_snap(proposed, &session.partners, work_area, threshold);
    let final_rect = match &outcome {
        Some(outcome) => {
            session.hits.clone_from(&outcome.hits);
            session.last_target = Some(outcome.target);
            // 关系记录：命中非空且选出的基座与已记录者不同才写
            // 属性（被拖窗成为基座的从属；其搬运集合不变，仍由
            // 本窗领队搬运，见库 snap_group 文件头）。
            if let Some(base) = choose_base(&outcome.hits) {
                if session.recorded_base != Some(base) {
                    snap_enum::write_base(own_id, base);
                    session.recorded_base = Some(base);
                }
            }
            outcome.target
        }
        None => {
            session.hits.clear();
            session.last_target = None;
            proposed
        }
    };
    if final_rect != proposed {
        // SAFETY: 同上，改写拟拖矩形即改写模态循环将采用的位置。
        unsafe {
            (*rect_ptr).left = final_rect.left as i32;
            (*rect_ptr).top = final_rect.top as i32;
            (*rect_ptr).right = final_rect.right as i32;
            (*rect_ptr).bottom = final_rect.bottom as i32;
        }
    }
    // 子树搬运：增量 = 最终落位左上 − 本窗当前实际左上；增量
    // 为零不调 HDWP。单帧一次 Begin/End 原子提交，不激活、
    // 不改尺寸、不改 Z 序。
    if let Some(actual) = own_actual {
        let delta = frame_delta(final_rect.pos(), actual);
        if (delta[0] != 0.0 || delta[1] != 0.0) && !session.carry.is_empty() {
            move_carry_by(&session.carry, delta);
        }
    }
}

/// 以 HDWP 把搬运集合按同一增量搬走（伙伴当前位置逐个现取，
/// 已销毁者跳过）。
fn move_carry_by(carry: &[u64], delta: [f32; 2]) {
    // SAFETY: 句柄来自本次拖动会话的枚举/关系表；BeginDefer-
    // WindowPos 的计数是本次要搬的窗口数上限，DeferWindowPos
    // 失败（窗口已销毁）时该项被跳过、整体仍以 End 提交。
    unsafe {
        let hdwp = BeginDeferWindowPos(carry.len() as i32);
        if hdwp.is_null() {
            return;
        }
        let mut hdwp = hdwp;
        for hwnd_value in carry {
            let Some(rect) = snap_enum::partner_rect_physical(*hwnd_value) else {
                continue;
            };
            let next = DeferWindowPos(
                hdwp,
                *hwnd_value as usize as HWND,
                std::ptr::null_mut(),
                (rect.left + delta[0]) as i32,
                (rect.top + delta[1]) as i32,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            );
            if !next.is_null() {
                hdwp = next;
            }
        }
        let _ = EndDeferWindowPos(hdwp);
    }
}
