//! 窗口几何生命周期状态机（工单 20 落地、深挖工单 05 收拢）：启动离屏钳制、首次放置、按当前图像比例从窗口高度推导宽度、几何落盘防抖与恢复默认都在这里；宿主每帧只喂观察值、执行返回的抽象命令。
//!
//! V2 相框外观专题工单 01 起，观察输入带四边内距：本状态机管的
//! 尺寸是外框尺寸（即窗口内尺寸），画面区域 = 外框 − 四边内距；
//! 高度口径是外框总高度，宽度仍由当前图比例推导。内距全 0 时
//! 行为与 V1 逐项一致。
//!
//! 本状态机只产出要保存的几何值；落盘与退出收尾归 config_sync。启动初值来自配置；图像比例由宿主在新图到达时更新。
//!
//! 切图几何口径（2026-11-23 收编合并定案）：切图时尺寸瞬间变到新图
//! 比例（切换动画专题 spec §7），不做尺寸/位置平滑动画，也不移动
//! 窗口位置（锚点沿用普通切图的既有规则）；对方专题的几何平滑动画
//! 与名义中心锚定已撤除。保留的名义量只有“名义高度”：目标尺寸按
//! 它推导并按工作区等比适配（超宽临时缩高），临时值不落盘。原生
//! 拖动后的回弹修复保留：指针按下期间只更新名义量、松手后重校正。
//! 工单 06 补充条款（spec §12）：校正后的外框矩形在当前位置溢出
//! 工作区时，同帧最小平移回界内；不溢出时位置仍纹丝不动。
//!
//! 已知限制：启动离屏判定把主屏当作原点 (0,0) 的矩形。egui 只提供
//! 相框所在屏尺寸，不提供屏幕原点与屏幕枚举；正式宿主会另外从
//! Win32 取工作区（含原点），本状态机的启动钳制仍按旧口径执行。

use crate::{
    config_sync::GeometryRecord,
    snap::{anchored_resize_position, solve_snap, AnchorEdge, SnapRequest, WindowRect},
};

/// 窗口最小边（逻辑尺寸）：保持无黑边时仍满足宿主最小窗口约束。
pub const MIN_WINDOW_SIDE: f32 = 120.0;
/// 默认窗口尺寸（逻辑）：无图时使用此尺寸，有图时高度作为缩放基准。
pub const DEFAULT_FRAME_SIZE: [f32; 2] = [480.0, 320.0];
/// 切图尺寸适配工作区时四边预留的边距（逻辑像素）。
pub const WORK_AREA_MARGIN: f32 = 6.0;

/// 四边内距（逻辑像素）：画框在每一边占的厚度，左/右/上/下各自
/// 独立（V2 相框外观专题工单 01）。外框尺寸 = 画面尺寸 + 四边
/// 内距；真实值由外观模式决定（内置按滑杆、自定义按相框图分析），
/// 本状态机只消费。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    /// 四边全 0：无画框时的内距，几何行为与 V1 逐项一致。
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    /// 左右合计：外框宽与画面宽之差。
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    /// 上下合计：外框高与画面高之差。
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

/// 几何落盘防抖（工单 20）：观察值停稳满此时长才产出保存命令。
const SAVE_SETTLE_SECS: f64 = 0.6;

/// 宿主每帧喂入的观察值：窗口矩形、显示器尺寸、工作区、指针与可见性。
/// 自工单 02 起带吸附请求（内含伙伴列表），不再是 Copy。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeomObservation {
    /// 窗口外矩形的位置（左上，逻辑坐标）；取不到时为 None。
    pub outer_pos: Option<[f32; 2]>,
    /// 窗口外矩形的尺寸；取不到时为 None。
    pub outer_size: Option<[f32; 2]>,
    /// 窗口内尺寸；取不到时为 None（本帧不做修正与落盘观察）。
    pub inner: Option<[f32; 2]>,
    /// 相框所在屏的尺寸；取不到时为 None。工作区缺失时作为退化的
    /// 工作区（原点 0,0）使用，兼容既有启动钳制口径。
    pub monitor: Option<[f32; 2]>,
    /// 相框所在屏的工作区 [left, top, right, bottom]（逻辑坐标）：
    /// 切图尺寸适配的边界；取不到时为 None。
    pub work_area: Option<[f32; 4]>,
    /// 指针是否按下（拖动/缩放进行中）：按下时不做锁定修正。
    pub pointer_down: bool,
    /// 相框是否可见：隐藏时不观察待落盘几何（工单 20）。
    pub frame_visible: bool,
    /// UI 时钟（秒）：防抖计时用。
    pub now_secs: f64,
    /// 当前外观的四边内距（逻辑像素）：外框尺寸推导的输入，
    /// 变化本身也会触发一次比例校正。
    pub insets: Insets,
    /// 松手吸附请求（多实例吸附专题工单 02）：宿主只在拖动松手那
    /// 一帧组装递交（枚举伙伴 + 开关已开才组装）；本状态机只在松
    /// 手帧消费它求解落位，按下帧与静止帧一律忽略——拖动进行中
    /// 不发吸附命令、静止时不因它产生任何命令。
    pub snap: Option<SnapRequest>,
    /// 本窗当前的贴合边（多实例吸附专题工单 03）：宿主以跟随引擎
    /// 最近一轮的接触派生供给；比例校正变尺寸时以这些边为锚（共
    /// 享边不动、向空闲方向长/缩），随后工单 06 钳制照旧叠加。
    /// 空 = 无贴合，变尺寸保持既有「位置不动」口径。
    pub snap_anchor: Vec<AnchorEdge>,
}

/// 状态机返回的抽象命令：宿主翻译成 ViewportCommand 执行，保存类
/// 命令经 config_sync 落盘。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeomCommand {
    /// 设置窗口外位置（左上，逻辑坐标）。
    SetOuterPosition([f32; 2]),
    /// 设置窗口内尺寸（逻辑）。
    SetInnerSize([f32; 2]),
    /// 要保存的几何值（落盘基准由状态机同步推进，宿主不必回告）。
    SaveGeometry(GeometryRecord),
    /// 恢复默认时相框处于隐藏：宿主经行为核心显示相框。
    ShowFrame,
}

/// 持有当前图像比例、最近稳定内尺寸、落盘基准、待落盘几何与一次性的启动钳制/首次放置标记。
pub struct GeometryMachine {
    image_aspect: Option<f32>,
    aspect_applied: bool,
    /// 最近一帧已结算的外框内尺寸：指针按下期间不推进，释放后据此
    /// 触发一次比例校正。
    last_inner: [f32; 2],
    /// 最近一帧实际观察到的几何：识别用户改动与程序化落地用。
    last_observed_inner: [f32; 2],
    last_observed_pos: Option<[f32; 2]>,
    last_insets: Insets,
    last_work_area: Option<[f32; 4]>,
    pointer_was_down: bool,
    /// 名义中心：落盘位置的推导基准；只有用户拖动/缩放、首次
    /// 放置、启动钳制或恢复默认才更新（切图本身不改位置）。
    nominal_center: Option<[f32; 2]>,
    /// 名义外框高度：用户设置的高度，超宽图临时缩高不修改它。
    nominal_height: f32,
    saved_geom: GeometryRecord,
    observed_geom: Option<GeometryRecord>,
    candidate: Option<(GeometryRecord, f64)>,
    startup_clamp_done: bool,
    placed: bool,
}

impl GeometryMachine {
    /// 以启动几何装配；收到首张图后再用图像比例修正宽度。
    pub fn new(
        saved_geom: GeometryRecord,
        image_aspect: Option<f32>,
        has_saved_position: bool,
    ) -> Self {
        let image_aspect = valid_aspect(image_aspect);
        let nominal_height = if saved_geom.3 > 0 {
            saved_geom.3 as f32
        } else {
            DEFAULT_FRAME_SIZE[1]
        };
        let saved_pos = match (saved_geom.0, saved_geom.1) {
            (Some(x), Some(y)) => Some([x as f32, y as f32]),
            _ => None,
        };
        Self {
            image_aspect,
            aspect_applied: image_aspect.is_none(),
            last_inner: [saved_geom.2 as f32, saved_geom.3 as f32],
            last_observed_inner: [saved_geom.2 as f32, saved_geom.3 as f32],
            last_observed_pos: saved_pos,
            last_insets: Insets::ZERO,
            last_work_area: None,
            pointer_was_down: false,
            nominal_center: saved_pos.map(|pos| {
                [
                    pos[0] + saved_geom.2 as f32 * 0.5,
                    pos[1] + saved_geom.3 as f32 * 0.5,
                ]
            }),
            nominal_height,
            saved_geom,
            observed_geom: None,
            candidate: None,
            startup_clamp_done: false,
            placed: has_saved_position,
        }
    }

    /// 当前用于推导窗口尺寸的图像比例。
    pub fn image_aspect(&self) -> Option<f32> {
        self.image_aspect
    }

    /// 新图到达时更新比例；比例变化后下一帧按当前高度调整窗口。
    pub fn set_image_aspect(&mut self, aspect: Option<f32>) {
        let aspect = valid_aspect(aspect);
        if self.image_aspect != aspect {
            self.image_aspect = aspect;
            self.aspect_applied = aspect.is_none();
        }
    }

    /// 喂一帧观察，返回本帧命令：启动钳制 → 图像比例校正
    /// → 落盘观察 → 首次放置。
    pub fn observe(&mut self, obs: &GeomObservation) -> Vec<GeomCommand> {
        let mut commands = Vec::new();
        let mut startup_position_issued = false;
        // 启动离屏钳制：只查一次，等外矩形与显示器都取到才查。
        if !self.startup_clamp_done {
            if let (Some(pos), Some(size), Some(monitor)) =
                (obs.outer_pos, obs.outer_size, obs.monitor)
            {
                self.startup_clamp_done = true;
                if let Some(clamped) = offscreen_position(pos, size, monitor) {
                    self.nominal_center =
                        Some([clamped[0] + size[0] * 0.5, clamped[1] + size[1] * 0.5]);
                    commands.push(GeomCommand::SetOuterPosition(clamped));
                    startup_position_issued = true;
                }
            }
        }
        let Some(cur) = obs.inner else {
            return commands;
        };
        let cur_pos = obs.outer_pos;
        let work_area = observation_work_area(obs);
        if work_area.is_some() {
            self.last_work_area = work_area;
        }
        let insets_changed = obs.insets != self.last_insets;
        self.last_insets = obs.insets;
        let cur_changed = cur != self.last_observed_inner;

        let mut effective_inner = cur;
        let mut effective_pos = cur_pos;
        if obs.pointer_down {
            // 拖动/缩放中用户最大：只记名义高度，不抢方向盘。
            if cur_changed {
                self.nominal_height = cur[1].max(MIN_WINDOW_SIDE);
            }
            if let Some(pos) = cur_pos {
                self.nominal_center = Some([pos[0] + cur[0] * 0.5, pos[1] + cur[1] * 0.5]);
            }
            self.pointer_was_down = true;
        } else {
            let released = self.pointer_was_down;
            if released {
                // 松手这一帧即使尺寸没再变，也要按名义高度做一次比例校正。
                if cur_changed {
                    self.nominal_height = cur[1].max(MIN_WINDOW_SIDE);
                }
                self.aspect_applied = false;
            }
            self.pointer_was_down = false;
            // 无图时没有比例校正参与，观察到的移动就是用户移动。
            if self.image_aspect.is_none() {
                if let Some(pos) = cur_pos {
                    self.nominal_center = Some([pos[0] + cur[0] * 0.5, pos[1] + cur[1] * 0.5]);
                }
            }
            if let Some(aspect) = self.image_aspect {
                if !self.aspect_applied || cur_changed || insets_changed {
                    // 瞬时校正（收编定案）：按名义高度推导，再按工作区
                    // 等比适配（超宽临时缩高，只改显示不落盘）；位置
                    // 原则上不动——只有校正后的外框矩形在当前位置
                    // 溢出工作区时，才同帧最小平移回界内（工单 06
                    // 补充条款）。本帧启动钳制已发位置命令时不叠加，
                    // 留待下一帧按新位置观察再判。
                    let desired = self.size_from_nominal_height(aspect, obs.insets);
                    let target_size = fit_size_to_work_area(desired, work_area);
                    if size_differs(cur, target_size) {
                        commands.push(GeomCommand::SetInnerSize(target_size));
                    }
                    if !startup_position_issued {
                        if let (Some(pos), Some(outer)) = (cur_pos, obs.outer_size) {
                            // 钳制按外框矩形算：目标外尺寸 = 目标内
                            // 尺寸 + 当前窗口边框差（外 − 内）。
                            let target_outer = [
                                target_size[0] + outer[0] - cur[0],
                                target_size[1] + outer[1] - cur[1],
                            ];
                            // 先锚边、后钳制（工单 03 + 工单 06 叠
                            // 加顺序，spec 第 7 节）：有贴合边时以共
                            // 享边为锚算变尺寸位置，再把锚后位置交
                            // 既有钳制判溢出平移；钳制口径不变。
                            let base_pos = if obs.snap_anchor.is_empty() {
                                pos
                            } else {
                                anchored_resize_position(
                                    WindowRect::from_pos_size(pos, outer),
                                    target_outer,
                                    &obs.snap_anchor,
                                )
                            };
                            if (base_pos[0] - pos[0]).abs() > 0.5
                                || (base_pos[1] - pos[1]).abs() > 0.5
                            {
                                commands.push(GeomCommand::SetOuterPosition(base_pos));
                                effective_pos = Some(base_pos);
                            }
                            if let Some(clamped) = clamp_outer_position_into_work_area(
                                base_pos,
                                target_outer,
                                work_area,
                            ) {
                                commands.push(GeomCommand::SetOuterPosition(clamped));
                                effective_pos = Some(clamped);
                            }
                        }
                    }
                    effective_inner = target_size;
                    self.aspect_applied = true;
                }
            }
            // 松手吸附（多实例吸附专题工单 02）：只在松手帧、且宿主
            // 本帧递交了吸附请求时求解一次——以校正后的有效位置与
            // 外尺寸为被拖窗矩形，伙伴与工作区边缘的贴合/对齐目标经
            // 既有 SetOuterPosition 命令落位，并成为本帧落盘观察的
            // 位置。按下帧的请求在上面的按下分支里从未被读，拖动
            // 进行中不发任何吸附命令。
            if released {
                if let Some(request) = &obs.snap {
                    if let (Some(pos), Some(outer)) = (effective_pos, obs.outer_size) {
                        let moving = WindowRect::from_pos_size(pos, outer);
                        let area = work_area.map(WindowRect::from_edges);
                        if let Some(target) =
                            solve_snap(moving, &request.partners, request.own_monitor, area)
                        {
                            commands.push(GeomCommand::SetOuterPosition(target));
                            effective_pos = Some(target);
                        }
                    }
                }
            }
            // 指针释放后（或程序化尺寸落地后）以实际尺寸为新基准，
            // 只有下一次用户改尺寸才重新触发校正。
            self.last_inner = cur;
        }
        self.last_observed_inner = cur;
        self.last_observed_pos = cur_pos;

        if obs.frame_visible {
            if let Some(pos) = effective_pos {
                let geom = self.nominal_record(Some(pos), effective_inner);
                self.observed_geom = Some(geom);
                if geom == self.saved_geom {
                    self.candidate = None;
                } else {
                    match self.candidate {
                        Some((value, since)) if value == geom => {
                            if obs.now_secs - since >= SAVE_SETTLE_SECS {
                                self.saved_geom = geom;
                                self.candidate = None;
                                commands.push(GeomCommand::SaveGeometry(geom));
                            }
                        }
                        _ => {
                            self.candidate = Some((geom, obs.now_secs));
                        }
                    }
                }
            }
        } else if let Some((geom, _)) = self.candidate.take() {
            // 隐藏帧不再更新观测量，待落盘的几何就没有下一位继承者了：
            // 立刻落盘，避免"移动后马上关到托盘"丢掉位置。
            self.saved_geom = geom;
            commands.push(GeomCommand::SaveGeometry(geom));
        }
        // 首次放置：无存档位置时，第一帧取到显示器就放右下角内缩。
        if !self.placed {
            if let Some(monitor) = obs.monitor {
                let pos = bottom_right_position(effective_inner, monitor);
                self.nominal_center = Some([
                    pos[0] + effective_inner[0] * 0.5,
                    pos[1] + effective_inner[1] * 0.5,
                ]);
                commands.push(GeomCommand::SetOuterPosition(pos));
                self.placed = true;
            }
        }
        commands
    }

    /// 恢复默认位置与大小：外框高度回到 320 逻辑像素，并按当前图像
    /// 比例与四边内距推导外框宽度；位置放在当前屏右下角内缩处，
    /// 隐藏时多一条 ShowFrame。显示器取不到时不发位置命令，保存
    /// 记录沿用旧 x/y。
    pub fn restore_default(
        &mut self,
        monitor: Option<[f32; 2]>,
        frame_visible: bool,
        insets: Insets,
    ) -> Vec<GeomCommand> {
        self.nominal_height = DEFAULT_FRAME_SIZE[1];
        let size = self
            .image_aspect
            .map(|aspect| outer_size_from_height(DEFAULT_FRAME_SIZE, aspect, insets))
            .unwrap_or(DEFAULT_FRAME_SIZE);
        self.last_inner = size;
        self.aspect_applied = true;
        let mut commands = vec![GeomCommand::SetInnerSize(size)];
        let pos = monitor.map(|m| bottom_right_position(size, m));
        if let Some(pos) = pos {
            commands.push(GeomCommand::SetOuterPosition(pos));
        }
        if let Some(pos) = pos {
            self.nominal_center = Some([pos[0] + size[0] * 0.5, pos[1] + size[1] * 0.5]);
        }
        if !frame_visible {
            commands.push(GeomCommand::ShowFrame);
        }
        let (x, y) = match pos {
            Some(p) => (Some(p[0].round() as i64), Some(p[1].round() as i64)),
            None => (self.saved_geom.0, self.saved_geom.1),
        };
        let record = (x, y, size[0] as u32, size[1] as u32);
        self.saved_geom = record;
        self.candidate = None;
        commands.push(GeomCommand::SaveGeometry(record));
        commands
    }

    /// 退出结算的交接（工单 04）：把（最近观察到的几何、落盘基准）
    /// 交给 config_sync 的 shutdown 一次结清。
    /// 待落盘几何的结算时刻（UI 时钟秒）。有候选时宿主应在这一刻前再出一帧，
    /// 让"停稳 0.6 秒"真正走完；否则要等到下一次轮播唤醒才写盘，
    /// 期间退出就会丢掉用户刚拖好的位置与尺寸。
    pub fn settle_deadline_secs(&self) -> Option<f64> {
        self.candidate.map(|(_, since)| since + SAVE_SETTLE_SECS)
    }

    pub fn exit_settle(&self) -> (Option<GeometryRecord>, GeometryRecord) {
        (self.observed_geom, self.saved_geom)
    }

    /// 名义尺寸：超宽图临时缩高不写回，落盘与退出结算都用它。
    fn size_from_nominal_height(&self, aspect: f32, insets: Insets) -> [f32; 2] {
        outer_size_from_height([0.0, self.nominal_height], aspect, insets)
    }

    fn nominal_record(&self, pos: Option<[f32; 2]>, fallback_inner: [f32; 2]) -> GeometryRecord {
        let size = self
            .image_aspect
            .map(|aspect| self.size_from_nominal_height(aspect, self.last_insets))
            .unwrap_or(fallback_inner);
        // 位置以实际观察为准（收编定案：切图不移动窗口，名义中心
        // 反推的位置会把没动过的位置写错）；名义中心只在观察不到
        // 位置时兜底。尺寸仍按名义量（临时缩高不落盘）。
        let nominal_pos = self
            .nominal_center
            .map(|center| [center[0] - size[0] * 0.5, center[1] - size[1] * 0.5]);
        let (x, y) = match pos.or(nominal_pos) {
            Some(p) => (Some(p[0].round() as i64), Some(p[1].round() as i64)),
            None => (self.saved_geom.0, self.saved_geom.1),
        };
        (x, y, size[0].round() as u32, size[1].round() as u32)
    }
}

/// 右下角内缩放置（首次启动、恢复默认、离屏钳制共用）：给定窗口
/// 尺寸与显示器尺寸，返回右下角内缩 24px 的位置（不小于 0）。
fn bottom_right_position(size: [f32; 2], monitor: [f32; 2]) -> [f32; 2] {
    [
        (monitor[0] - size[0] - 24.0).max(0.0),
        (monitor[1] - size[1] - 24.0).max(0.0),
    ]
}

/// 按外框高度推导外框尺寸：画面高 = 外框高 − 上下内距，画面宽按
/// 图像比例推导，外框宽 = 画面宽 + 左右内距；画面保持最小边约束。
/// 内距全 0 时与 V1 的推导逐项一致。
fn outer_size_from_height(current: [f32; 2], aspect: f32, insets: Insets) -> [f32; 2] {
    let Some(aspect) = valid_aspect(Some(aspect)) else {
        return current;
    };
    let outer_height = if current[1].is_finite() && current[1] > 0.0 {
        current[1]
    } else {
        DEFAULT_FRAME_SIZE[1]
    };
    let mut image_height = outer_height - insets.vertical();
    if image_height < MIN_WINDOW_SIDE {
        image_height = MIN_WINDOW_SIDE;
    }
    let mut image_width = image_height * aspect;
    if image_width < MIN_WINDOW_SIDE {
        image_width = MIN_WINDOW_SIDE;
        image_height = MIN_WINDOW_SIDE / aspect;
    }
    [
        image_width + insets.horizontal(),
        image_height + insets.vertical(),
    ]
}

fn fit_size_to_work_area(size: [f32; 2], work_area: Option<[f32; 4]>) -> [f32; 2] {
    let Some([left, top, right, bottom]) = work_area else {
        return size;
    };
    let available_width = (right - left - WORK_AREA_MARGIN * 2.0).max(1.0);
    let available_height = (bottom - top - WORK_AREA_MARGIN * 2.0).max(1.0);
    let scale = (available_width / size[0])
        .min(available_height / size[1])
        .min(1.0);
    [size[0] * scale, size[1] * scale]
}

/// 切图溢出钳制（工单 06）：外框矩形在 pos 处溢出工作区时，返回
/// 平移刚好回界的位置——四边各保留 WORK_AREA_MARGIN 边距，每轴只
/// 移动越界的那一截；不溢出或工作区未知时返回 None。外框比可用区
/// 还大时贴到左上界兜底（尺寸本身已由 fit_size_to_work_area 先行
/// 适配，此处只差窗口边框差量级）。
fn clamp_outer_position_into_work_area(
    pos: [f32; 2],
    outer_size: [f32; 2],
    work_area: Option<[f32; 4]>,
) -> Option<[f32; 2]> {
    let [left, top, right, bottom] = work_area?;
    let min_x = left + WORK_AREA_MARGIN;
    let min_y = top + WORK_AREA_MARGIN;
    let max_x = (right - WORK_AREA_MARGIN - outer_size[0]).max(min_x);
    let max_y = (bottom - WORK_AREA_MARGIN - outer_size[1]).max(min_y);
    let clamped = [pos[0].clamp(min_x, max_x), pos[1].clamp(min_y, max_y)];
    if (clamped[0] - pos[0]).abs() > 0.5 || (clamped[1] - pos[1]).abs() > 0.5 {
        Some(clamped)
    } else {
        None
    }
}

fn observation_work_area(obs: &GeomObservation) -> Option<[f32; 4]> {
    obs.work_area.or_else(|| {
        obs.monitor
            .map(|monitor| [0.0, 0.0, monitor[0], monitor[1]])
    })
}

fn size_differs(current: [f32; 2], target: [f32; 2]) -> bool {
    (current[0] - target[0]).abs() > 0.5 || (current[1] - target[1]).abs() > 0.5
}

fn valid_aspect(aspect: Option<f32>) -> Option<f32> {
    aspect.filter(|value| value.is_finite() && *value > 0.0)
}

/// 启动恢复位置的主屏钳制（工单 20）：窗口与主屏完全无交集时返回
/// 主屏右下角内缩 24px 的位置；有交集返回 None。限制口径见模块头。
fn offscreen_position(position: [f32; 2], size: [f32; 2], monitor: [f32; 2]) -> Option<[f32; 2]> {
    let intersects = position[0] < monitor[0]
        && position[1] < monitor[1]
        && position[0] + size[0] > 0.0
        && position[1] + size[1] > 0.0;
    if intersects {
        None
    } else {
        Some(bottom_right_position(size, monitor))
    }
}
