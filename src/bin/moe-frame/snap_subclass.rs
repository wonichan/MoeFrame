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
//!
//! 窗口投影工单 02 起本层多一项接管：相框窗口的边框样式在装
//! 配时被剥掉后，DefWindowProc 的移动/缩放模态环随之失效，
//! 本层拦截 WM_NCLBUTTONDOWN 的 HTCAPTION 与八个 HT 缩放
//! 码（egui 的 StartDrag / BeginResize 经 winit 投递至此），
//! 吞掉消息自行跑模态环：SetCapture 后泵消息、按光标位移算
//! 拟议矩形（库 modal_drag 纯判定）、每帧流经与 WM_MOVING
//! 同一条吸附调整、SetWindowPos 落位；Esc 恢复起始矩形，
//! WM_CANCELMODE 同样恢复，捕获被夺按现位结束。环的进入与
//! 退出经子类链送 WM_ENTERSIZEMOVE / WM_EXITSIZEMOVE，与
//! 原生环同构——吸附会话的组装/收尾走既有分支，winit 的
//! dragging 旗标复位与其合成 WM_LBUTTONUP（应用层的松手
//! 感知来源）也原样发生；真实的 WM_LBUTTONUP 被环消费、
//! 不分发，与 DefWindowProc 消费真实抬键同构。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use moe_frame::modal_drag::{drag_rect, esc_press_edge, DragHit};
use moe_frame::snap::{
    drag_partners_on_monitor, physical_snap_threshold, solve_drag_snap, DragSnapPartner,
    PartnerFrame, SnapHit, WindowRect,
};
use moe_frame::snap_group::{
    base_relation_valid, base_update_decision, frame_delta, plan_drag_start, temporary_contact_ids,
    SnapPartner, SnapRelation,
};
use moe_frame::window_geom::MIN_WINDOW_SIDE;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, VK_ESCAPE,
    },
    UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
    UI::WindowsAndMessaging::{
        BeginDeferWindowPos, DeferWindowPos, DispatchMessageW, EndDeferWindowPos, GetCursorPos,
        GetMessageW, PostQuitMessage, SendMessageW, SetWindowPos, TranslateMessage, MSG,
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, WM_CANCELMODE, WM_CAPTURECHANGED,
        WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOVING, WM_NCLBUTTONDOWN,
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

/// 自研模态环进行中旗标（工单 02）：环内再来的
/// WM_NCLBUTTONDOWN 直接忽略，与 DefWindowProc 模态环不
/// 重入同构。访问都在 UI 线程，AtomicBool 只为静态可 Sync。
static MODAL_DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);

/// 一次原生拖动的会话状态：开始时枚举一次的伙伴（矩形每帧经
/// GetWindowRect 刷新，与伙伴各自的显示器矩形事实成对保存，
/// 供 WM_MOVING 每帧同屏过滤）、本次拖动的搬运集合（拖动开
/// 始按关系树 + 存量接触临时成树定死，拖动中不因新关系改
/// 变）、会话已记录的本窗基座、基座记录的排除集与独走禁记
/// 旗标（审查修复 F1/F2：排除集 = 搬运集合 ∪ 拖动开始被清
/// 掉的基座，WM_ENTERSIZEMOVE 组装、WM_EXITSIZEMOVE 随会
/// 话销毁）、最近一帧的贴合命中与目标矩形。关系属性留在窗
/// 上、不随会话清除。
pub(crate) struct DragSession {
    pub(crate) partners: Vec<(DragSnapPartner, WindowRect)>,
    pub(crate) carry: Vec<u64>,
    pub(crate) recorded_base: Option<u64>,
    pub(crate) recording_exclusions: Vec<u64>,
    pub(crate) block_base_recording: bool,
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
    // handle 取得）。snap_subclass_proc 处理拖动消息
    // （WM_ENTERSIZEMOVE / WM_MOVING / WM_EXITSIZEMOVE）并
    // 拦截 WM_NCLBUTTONDOWN 的拖动/缩放命中码跑自研模态环
    // （工单 02），其余消息一律经 DefSubclassProc 转发给
    // winit 自有链；整体安全性不来自回调本身无 unsafe，而来
    // 自各 unsafe 点的局部论证（RECT 改写、HDWP、属性读写、
    // 模态环泵消息各自的 SAFETY 注记）。
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
/// 三条消息做吸附处理；WM_NCLBUTTONDOWN 的拖动/缩放命中码被
/// 拦截跑自研模态环（工单 02，吞掉不转发——转发给
/// DefWindowProc 正是要避开的失效原生环）；其余一律转发。
unsafe extern "system" fn snap_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid: usize,
    _ref_data: usize,
) -> LRESULT {
    if msg == WM_NCLBUTTONDOWN {
        if let Some(hit) = DragHit::from_hit_code(wparam as i32) {
            run_modal_drag(hwnd, hit);
            return 0;
        }
    }
    if SNAP_ENABLED.load(Ordering::Relaxed) {
        match msg {
            WM_ENTERSIZEMOVE => {
                begin_drag_session(hwnd);
            }
            WM_MOVING => {
                handle_moving(hwnd, lparam);
            }
            _ => {}
        }
    }
    if msg == WM_EXITSIZEMOVE {
        // 会话收尾（开关启用与否都执行）：清拖动会话，并复位
        // Shift 独走旗标（审查修复 F6——旗标只活在一次拖动内，
        // 不复位会污染下一次拖动的独走判定）。关系属性留在窗
        // 上（卫生规则在拖动开始与拖动中校验），不在此清。
        end_drag_session();
    }
    // SAFETY: 转发参数原样来自系统分发；DefSubclassProc 是子类化
    // 链的标准下一环（winit 自有过程）。
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// 拖动会话收尾（WM_EXITSIZEMOVE 分支专用）：清会话并复位
/// Shift 独走旗标，语义见 proc 的 EXITSIZEMOVE 注记。自研模
/// 态环不直接调本函数——它在收尾时送 WM_EXITSIZEMOVE 走同
/// 一分支，保持单一收尾路径。
fn end_drag_session() {
    if let Ok(mut session) = DRAG_SESSION.lock() {
        *session = None;
    }
    SOLO_DRAG.store(false, Ordering::Relaxed);
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
    // 拖动开始被清掉的基座（卫生清理或 plan 裁决脱离）：进入
    // 本次会话的记录排除集，整个会话内不得被命中写回（F2）。
    let mut cleared_base: Option<u64> = None;
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
            cleared_base = own_base;
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
            // work_area 填各窗所在显示器的工作区（rcWork），不
            // 是显示器矩形（审查修复 S6）：本窗经自家 HWND 取，
            // 伙伴的已在 drag_partner_infos 组装时取好。
            let own_frame = PartnerFrame {
                rect: own_rect,
                monitor: own_monitor,
                work_area: snap_enum::work_area_rect_physical(own_id).unwrap_or(own_monitor),
            };
            let partners: Vec<SnapPartner> = infos
                .iter()
                .map(|info| SnapPartner {
                    id: info.snap.hwnd,
                    frame: PartnerFrame {
                        rect: info.snap.rect,
                        monitor: info.monitor,
                        work_area: info.work_area,
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
        cleared_base = own_base;
        own_base = None;
    }
    // 基座记录排除集（F1/F2）：搬运集合（我的从属 / 临时从属，
    // 记成基座会让属性树成环）∪ 拖动开始被清掉的基座（脱离 /
    // 卫生清理，同一拖动内不得被首帧命中写回）。solo 会话另以
    // block_base_recording 整体禁记。两者都只活在本会话内。
    let mut recording_exclusions = plan.carry.clone();
    if let Some(cleared) = cleared_base {
        if !recording_exclusions.contains(&cleared) {
            recording_exclusions.push(cleared);
        }
    }
    if let Ok(mut session) = DRAG_SESSION.lock() {
        *session = Some(DragSession {
            partners: infos.iter().map(|info| (info.snap, info.monitor)).collect(),
            carry: plan.carry,
            recorded_base: own_base,
            recording_exclusions,
            block_base_recording: solo,
            hits: Vec::new(),
            last_target: None,
        });
    }
}

/// WM_MOVING 一帧：读 lParam 的拟拖 RECT、走吸附调整、把
/// 最终矩形写回（原生模态循环据此落位）。
fn handle_moving(hwnd: HWND, lparam: LPARAM) {
    if lparam == 0 {
        return;
    }
    // SAFETY: WM_MOVING 的 lparam 按 Win32 约定指向系统持有
    // 的 RECT（拟拖窗口矩形，物理像素），在本次回调内读写
    // 合法。
    let rect_ptr = lparam as *mut RECT;
    let proposed = unsafe {
        WindowRect {
            left: (*rect_ptr).left as f32,
            top: (*rect_ptr).top as f32,
            right: (*rect_ptr).right as f32,
            bottom: (*rect_ptr).bottom as f32,
        }
    };
    let Some(final_rect) = adjust_drag_frame(hwnd, proposed) else {
        return;
    };
    if final_rect != proposed {
        // SAFETY: 同上，改写拟拖矩形即改写模态循环将采用的
        // 位置。
        unsafe {
            (*rect_ptr).left = final_rect.left as i32;
            (*rect_ptr).top = final_rect.top as i32;
            (*rect_ptr).right = final_rect.right as i32;
            (*rect_ptr).bottom = final_rect.bottom as i32;
        }
    }
}

/// 一帧拖动的吸附调整（WM_MOVING 路径与自研模态环共用）：
/// 刷新伙伴矩形、按拟拖矩形所在显示器过滤伙伴后求解吸附、
/// 逐帧卫生校验已记录基座、按命中记录关系，最后以帧增量用
/// HDWP 搬运搬运集合。返回最终落位矩形；无拖动会话时返回
/// None（调用方按未调整处理）。
fn adjust_drag_frame(hwnd: HWND, proposed: WindowRect) -> Option<WindowRect> {
    let mut guard = DRAG_SESSION.lock().ok()?;
    let session = guard.as_mut()?;
    let own_id = hwnd as usize as u64;
    // 本窗当前实际左上（帧首以自家 HWND 的 GetWindowRect 取得，
    // 物理像素）：帧增量的减数。
    let own_actual = snap_enum::partner_rect_physical(own_id).map(|rect| rect.pos());
    session.partners.retain_mut(|(partner, _monitor)| {
        match snap_enum::partner_rect_physical(partner.hwnd) {
            Some(rect) => {
                partner.rect = rect;
                true
            }
            None => false,
        }
    });
    let work_area = snap_enum::work_area_for_physical_rect(proposed);
    let threshold = physical_snap_threshold(snap_enum::own_dpi(hwnd as isize));
    // 同屏过滤（F3）：只让与拟拖矩形同屏的伙伴参与本帧求解；
    // 所在屏取不到时不丢伙伴（MonitorFromRect 带 NEAREST 兜底，
    // 实践中恒有值），按全量求解。
    let proposed_monitor = snap_enum::monitor_rect_for_physical_rect(proposed);
    let solve_partners = match proposed_monitor {
        Some(monitor) => drag_partners_on_monitor(&session.partners, monitor),
        None => session
            .partners
            .iter()
            .map(|(partner, _)| *partner)
            .collect(),
    };
    let outcome = solve_drag_snap(proposed, &solve_partners, work_area, threshold);
    // 逐帧卫生（F4）：记录新基座前先校验会话已记录的基座——
    // 基座已不在伙伴集（已销毁被 retain 剔除）或与本帧最终落
    // 位已不接触（含跨屏），则清属性并清会话记录。与拖动开
    // 始的卫生校验共用库 snap_group::base_relation_valid。
    if let Some(recorded) = session.recorded_base {
        let base_frame = session
            .partners
            .iter()
            .find(|(partner, _)| partner.hwnd == recorded);
        let final_for_hygiene = outcome.as_ref().map_or(proposed, |o| o.target);
        let valid = match (base_frame, proposed_monitor) {
            (Some((partner, partner_monitor)), Some(monitor)) => base_relation_valid(
                true,
                final_for_hygiene,
                partner.rect,
                *partner_monitor == monitor,
                threshold,
            ),
            _ => false,
        };
        if !valid {
            snap_enum::clear_base(own_id);
            session.recorded_base = None;
        }
    }
    let final_rect = match &outcome {
        Some(outcome) => {
            session.hits.clone_from(&outcome.hits);
            session.last_target = Some(outcome.target);
            // 关系记录：由库 base_update_decision 裁决（被拖窗成
            // 为基座的从属；其搬运集合不变，仍由本窗领队搬运，
            // 见库 snap_group 文件头）。排除集与 solo 禁记在拖
            // 动开始组装进会话（F1/F2），命中 carry 内伙伴或刚
            // 脱离的旧基座都不写回。
            if let Some(base) = base_update_decision(
                session.recorded_base,
                &outcome.hits,
                &session.recording_exclusions,
                session.block_base_recording,
            ) {
                snap_enum::write_base(own_id, base);
                session.recorded_base = Some(base);
            }
            outcome.target
        }
        None => {
            session.hits.clear();
            session.last_target = None;
            proposed
        }
    };
    // 子树搬运：增量 = 最终落位左上 − 本窗当前实际左上；增量
    // 为零不调 HDWP。单帧一次 Begin/End 原子提交，不激活、
    // 不改尺寸、不改 Z 序。
    if let Some(actual) = own_actual {
        let delta = frame_delta(final_rect.pos(), actual);
        if (delta[0] != 0.0 || delta[1] != 0.0) && !session.carry.is_empty() {
            move_carry_by(&session.carry, delta);
        }
    }
    Some(final_rect)
}

/// 拦截到的拖动/缩放自研模态环入口（工单 02）：环内重入直
/// 接忽略；进入与退出经子类链送 WM_ENTERSIZEMOVE /
/// WM_EXITSIZEMOVE——前者让吸附会话按既有分支组装、winit
/// 置 IN_SIZE_MOVE 旗标，后者让会话按既有分支收尾、winit
/// 复位 dragging 旗标并合成 WM_LBUTTONUP（应用层的松手感
/// 知来源），与 DefWindowProc 原生环的进入/退出通知同构。
fn run_modal_drag(hwnd: HWND, hit: DragHit) {
    if MODAL_DRAG_ACTIVE.swap(true, Ordering::Relaxed) {
        return;
    }
    // SAFETY: hwnd 是本进程相框窗口。SendMessageW 在本线程同
    // 步走子类链送达这两条通知：回调会以不同消息重入本
    // proc，无状态冲突（会话锁只在各分支内短暂持有）；模态
    // 环本体在两条通知之间运行，期间本 proc 只经消息分发以
    // 其他消息重入。
    unsafe {
        SendMessageW(hwnd, WM_ENTERSIZEMOVE, 0, 0);
        modal_drag_loop(hwnd, hit);
        SendMessageW(hwnd, WM_EXITSIZEMOVE, 0, 0);
    }
    MODAL_DRAG_ACTIVE.store(false, Ordering::Relaxed);
}

/// 模态环本体：SetCapture 后泵线程消息，光标移动时按位移算
/// 拟议矩形（库 modal_drag）、流经吸附调整（开关启用且会话
/// 在时）、SetWindowPos 落位。终止条件与 DefWindowProc 语
/// 义对齐：本窗 WM_LBUTTONUP 落位结束（消息被消费，松手通
/// 知由收尾的 WM_EXITSIZEMOVE 触发 winit 合成）；Esc 恢复
/// 起始矩形后结束——不走 WM_KEYDOWN 消息（那依赖本窗持有
/// 焦点），改逐轮轮询 GetAsyncKeyState 的新按下沿，与原生
/// 环逐轮查键态同构；WM_CANCELMODE 同样恢复起始矩形后结
/// 束（消息被消费）；捕获被夺（WM_CAPTURECHANGED）按现位
/// 结束（消息照常分发）。其余消息一律翻译 + 分发，设置窗
/// 与定时器/绘制消息照常流转——与原生环在同一线程泵消息
/// 同构。捕获没拿到手就直接放弃本次环（鼠标消息不会投向
/// 本窗，环无从推进）。
fn modal_drag_loop(hwnd: HWND, hit: DragHit) {
    let own_id = hwnd as usize as u64;
    // 起始矩形与起始光标（物理像素）：环内所有拟议矩形都从
    // 这组初值按光标位移推导，不累积帧间误差。
    let Some(start_rect) = snap_enum::partner_rect_physical(own_id) else {
        return;
    };
    let Some(start_cursor) = cursor_pos() else {
        return;
    };
    // 最小尺寸（物理像素）：库的逻辑常量 MIN_WINDOW_SIDE 按
    // 本窗 DPI 换算——DefWindowProc 曾以最小跟踪尺寸代劳的
    // 钳制在环里按此补齐；最大跟踪尺寸仓库无既有常量，不复
    // 刻（拟议矩形本就只是提议，应用层派生在后续帧收敛）。
    let min_side = MIN_WINDOW_SIDE * snap_enum::own_dpi(hwnd as isize) as f32 / 96.0;
    let snap_enabled = SNAP_ENABLED.load(Ordering::Relaxed);
    // SAFETY: 本回调运行在窗口所在线程。SetCapture 后鼠标消
    // 息都投本窗；环的每条退出路径都 ReleaseCapture。泵消息
    // 的 GetMessageW 以空窗口句柄取本线程全部消息。
    unsafe {
        SetCapture(hwnd);
        if GetCapture() != hwnd {
            // 没拿到手：捕获在别人手里，不碰它，直接放弃。
            return;
        }
        // Esc 轮询的基线：入环这一刻的键态作首轮的 was_down
        // ——入环前已按住的 Esc 不触发取消（按下沿判定在库
        // esc_press_edge）。
        let mut esc_was_down = esc_is_down();
        let mut restore = false;
        let mut finished = false;
        while !finished {
            let mut msg: MSG = std::mem::zeroed();
            let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if ret == -1 {
                // 取消息出错：按现位结束，不恢复。
                break;
            }
            if ret == 0 {
                // WM_QUIT：把退出消息重新投回队列，让外层事件
                // 循环照常收尾；拖动按现位结束。
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            // Esc 取消逐轮轮询：命中新按下沿即恢复起始矩形结
            // 束，本条消息随环结束被消费、不再分发。Esc 的键
            // 消息本身会唤醒 GetMessageW，光标静止时按下也能
            // 在本轮命中，不必等下一条鼠标消息。
            let esc_down = esc_is_down();
            if esc_press_edge(esc_was_down, esc_down) {
                restore = true;
                break;
            }
            esc_was_down = esc_down;
            match msg.message {
                WM_LBUTTONUP if msg.hwnd == hwnd => {
                    finished = true;
                }
                WM_CANCELMODE if msg.hwnd == hwnd => {
                    restore = true;
                    finished = true;
                }
                WM_CAPTURECHANGED if msg.hwnd == hwnd => {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    finished = true;
                }
                WM_MOUSEMOVE if msg.hwnd == hwnd => {
                    // 先翻译 + 分发再算落位：让 egui 经自身
                    // WndProc 先同步到最新指针位置，后续帧的
                    // 命中判定与光标形状不滞后一帧。
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    if let Some(cursor) = cursor_pos() {
                        let proposed =
                            drag_rect(hit, start_rect, start_cursor, cursor, [min_side, min_side]);
                        let final_rect = if snap_enabled {
                            adjust_drag_frame(hwnd, proposed).unwrap_or(proposed)
                        } else {
                            proposed
                        };
                        apply_drag_rect(hwnd, final_rect);
                    }
                }
                _ => {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        if restore {
            apply_drag_rect(hwnd, start_rect);
        }
        ReleaseCapture();
    }
}

/// Esc 当前是否按住（GetAsyncKeyState 返回值的高位即当前
/// 键态）；模态环逐轮轮询用。低位「上次查询以来是否按过」
/// 不读——取消只认按下沿，按键基线由环维护（库
/// esc_press_edge）。
fn esc_is_down() -> bool {
    // SAFETY: 纯读全局键态，无指针参数。
    unsafe { GetAsyncKeyState(VK_ESCAPE as i32) < 0 }
}

/// 当前光标位置（物理像素，屏幕坐标）；取不到返回 None。
fn cursor_pos() -> Option<[f32; 2]> {
    let mut point: POINT = unsafe { std::mem::zeroed() };
    // SAFETY: point 是栈上 POINT，调用同步填充后即读。
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return None;
    }
    Some([point.x as f32, point.y as f32])
}

/// 把矩形落到相框窗口上（模态环专用）：只改位置与尺寸，不
/// 动 Z 序、不激活。
fn apply_drag_rect(hwnd: HWND, rect: WindowRect) {
    // SAFETY: hwnd 是本进程相框窗口（模态环持有捕获期间）。
    unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            rect.left as i32,
            rect.top as i32,
            rect.width() as i32,
            rect.height() as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
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
