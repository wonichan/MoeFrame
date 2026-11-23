//! 设置窗：会话动作执行、控件渲染与草稿跟踪。

use super::*;

/// 间隔六档预设（工单 11/18）：设置窗间隔单选的数据来源。
pub(crate) const INTERVAL_PRESETS: [(&str, i64); 6] = [
    ("10 秒", 10),
    ("30 秒", 30),
    ("1 分钟", 60),
    ("5 分钟", 300),
    ("10 分钟", 600),
    ("30 分钟", 1800),
];
/// 设置窗尺寸（工单 18；V2 相框外观工单 01 加「外观」组后高 700→750，
/// 工单 02 加内置画框参数块后 750→830，切换动画专题工单 04 在播放组
/// 加「切换动画」两行单选后 830→910）：
/// 建窗与屏幕居中计算共用。
pub(crate) const SETTINGS_SIZE: [f32; 2] = [420.0, 910.0];
/// 设置窗的视口标识（工单 18）：全程只有一块。
pub(crate) fn settings_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("moe-frame-settings")
}
/// 输入字段的焦点跟踪（工单 19）：返回（回车，失焦）——回车记 enter
/// 并把焦点抢回框内（回车提交后焦点留在输入框）；焦点离开记 blurred。
/// 事件的时序裁决在库 settings_session，这里只做 egui 焦点机制。
pub(crate) fn track_draft_field(
    response: &egui::Response,
    was_focused: &mut bool,
    enter: bool,
) -> (bool, bool) {
    if enter && (*was_focused || response.lost_focus()) {
        response.request_focus();
        *was_focused = true;
        (true, false)
    } else {
        let blurred = (*was_focused && !response.has_focus()) || response.lost_focus();
        *was_focused = response.has_focus();
        (false, blurred)
    }
}
impl FrameApp {
    pub(crate) fn request_settings(&mut self, ctx: &egui::Context) {
        match self.session.open_requested(&self.effective_values()) {
            OpenDecision::FocusOnly => {
                ctx.send_viewport_cmd_to(settings_viewport_id(), egui::ViewportCommand::Focus);
            }
            OpenDecision::Opened => {
                self.settings_open = true;
                self.settings_positioned = false;
            }
        }
    }
    pub(crate) fn effective_values(&self) -> EffectiveValues {
        EffectiveValues {
            folder_text: self.effective_folder_text(),
            interval_secs: self.core.interval_secs(),
            mode: self.core.playback_mode(),
            animation: self.core.animation(),
            appearance: self.appearance,
            frame: self.frame,
        }
    }
    pub(crate) fn effective_folder_text(&self) -> String {
        self.core
            .folder()
            .map(|folder| folder.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
    pub(crate) fn execute_session_actions(
        &mut self,
        ctx: &egui::Context,
        actions: Vec<SessionAction>,
        now: u64,
    ) {
        for action in actions {
            match action {
                SessionAction::CommitFolder(text) => {
                    let effects = self.core.handle(Intent::FolderPicked(PathBuf::from(text)));
                    self.dispatch(ctx, effects);
                }
                SessionAction::CommitInterval(secs) | SessionAction::ApplyIntervalPreset(secs) => {
                    let effects = self
                        .core
                        .handle(Intent::IntervalSelected { secs, now_ms: now });
                    self.dispatch(ctx, effects);
                }
                SessionAction::ApplyMode(mode) => {
                    let effects = self.core.handle(Intent::PlaybackModeSelected(mode));
                    self.dispatch(ctx, effects);
                }
                SessionAction::ApplyAnimation(animation) => {
                    // 切换动画档位归核心持有（工单 01 路线 A）：核心
                    // 改档并回 SaveAnimation 效果落盘；档位只在起转场
                    // 时读取，动画中改档自下一次切换生效。
                    let effects = self.core.handle(Intent::AnimationSelected(animation));
                    self.dispatch(ctx, effects);
                }
                SessionAction::ApplyAppearance(mode) => {
                    // 外观是宿主状态：改当前值并经 config_sync 写回，
                    // 几何与解码口径下一帧起按新模式的内距工作，
                    // 不重启进程（V2 相框外观专题工单 01）。
                    self.appearance = mode;
                    self.sync.save_appearance_mode(mode);
                    if mode == AppearanceMode::Custom {
                        // 每次切到自定义都重新分析（工单 03）：用户
                        // 修好图或换了文件内容，切回即恢复。
                        self.reload_frame_art();
                    }
                }
                SessionAction::PickFrameArt => {
                    // 选相框图（工单 03）：只接受 PNG；选定即保存
                    // 路径并重新分析，取消则什么都不变。分析失败
                    // 置回退态（原因在设置窗显示），不弹窗。
                    let picked = rfd::FileDialog::new()
                        .set_title("选择相框图")
                        .add_filter("PNG 图片", &["png"])
                        .pick_file();
                    if let Some(path) = picked {
                        self.frame_art_path = Some(path.clone());
                        self.sync.save_frame_art_path(Some(&path));
                        self.reload_frame_art();
                    }
                }
                SessionAction::ApplyFramePreset(preset) => {
                    // 内置画框参数同为宿主状态：改当前值并经
                    // config_sync 整组写回，绘制与内距下一帧起按
                    // 新值工作（V2 相框外观专题工单 02）。
                    self.frame.preset = preset;
                    self.sync.save_builtin_frame(self.frame);
                }
                SessionAction::ApplyFrameWidth(width) => {
                    self.frame.frame_width = width;
                    self.sync.save_builtin_frame(self.frame);
                }
                SessionAction::ApplyMatWidth(width) => {
                    self.frame.mat_width = width;
                    self.sync.save_builtin_frame(self.frame);
                }
                SessionAction::RestoreDefaultGeometry => {
                    self.restore_default_geometry(ctx);
                }
                SessionAction::ApplyAutostart(target) => {
                    self.apply_autostart(target);
                }
                SessionAction::Rescan => {
                    let effects = self.core.handle(Intent::RescanRequested);
                    self.dispatch(ctx, effects);
                }
                SessionAction::PickFolder => {
                    let effects = self.core.handle(Intent::FolderPickRequested);
                    self.dispatch(ctx, effects);
                }
                SessionAction::OpenCurrentImage => {
                    if let View::Showing(path) = self.core.view() {
                        open_image(&path);
                    }
                }
                SessionAction::OpenCurrentFolder => {
                    if let View::Showing(path) = self.core.view() {
                        if let Some(parent) = path.parent() {
                            let _ = open::that(parent);
                        }
                    }
                }
                SessionAction::CopyCurrentPath => {
                    if let View::Showing(path) = self.core.view() {
                        ctx.copy_text(path.to_string_lossy().into_owned());
                    }
                }
                SessionAction::CloseWindow => {
                    self.settings_open = false;
                }
            }
        }
    }
    pub(crate) fn show_settings(&mut self, ctx: &egui::Context, now: u64) {
        self.session.begin_frame(&self.effective_values());
        let mut builder = egui::ViewportBuilder::default()
            .with_title("萌框设置")
            .with_inner_size(SETTINGS_SIZE)
            .with_taskbar(false)
            .with_icon(app_icon_data())
            .with_resizable(false);
        if !self.settings_positioned {
            self.settings_positioned = true;
            // 默认在相框所在屏幕中央：egui 只给得到相框所在屏的尺寸，原点
            // 按该屏左上角计（单屏精确）；拿不到尺寸就不指定位置。
            if let Some(monitor) = ctx.input(|input| input.viewport().monitor_size) {
                builder = builder.with_position([
                    ((monitor.x - SETTINGS_SIZE[0]) / 2.0).max(0.0),
                    ((monitor.y - SETTINGS_SIZE[1]) / 2.0).max(0.0),
                ]);
            }
        }
        let mode_now = self.core.playback_mode();
        let animation_now = self.core.animation();
        let appearance_now = self.appearance;
        let frame_now = self.frame;
        // 相框图状态（工单 03）：路径与分析结论在渲染前取快照，
        // 设置窗「外观」组的自定义区块照单显示。
        let frame_art_path_now = self
            .frame_art_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        let frame_art_status_now = match &self.frame_art {
            FrameArtHost::NotSelected => None,
            FrameArtHost::Loaded { analysis, .. } => {
                let insets = analysis.insets();
                Some(format!(
                    "检测成功：洞 {}×{}，内距 左{} 上{} 右{} 下{}",
                    analysis.hole.width,
                    analysis.hole.height,
                    insets.left,
                    insets.top,
                    insets.right,
                    insets.bottom
                ))
            }
            FrameArtHost::Fallback { reason } => Some(format!("不可用，已回退内置画框：{reason}")),
        };
        let interval_now = self.core.interval_secs();
        let autostart_now = self.autostart.enabled();
        let current_showing = match self.core.view() {
            View::Showing(path) => Some(path),
            View::Empty => None,
        };
        let mut picks = Picks::default();
        let mut viewport_focused_now = true;
        let session = &mut self.session;
        let picks_ref = &mut picks;
        let focused_ref = &mut viewport_focused_now;
        ctx.show_viewport_immediate(settings_viewport_id(), builder, |ui, _class| {
            let sctx = ui.ctx().clone();
            if sctx.input(|input| input.viewport().close_requested())
                || sctx.input(|input| input.key_pressed(egui::Key::Escape))
            {
                picks_ref.close = true;
            }
            let enter = sctx.input(|input| input.key_pressed(egui::Key::Enter));
            *focused_ref = sctx.input(|input| input.viewport().focused).unwrap_or(true);
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label("图库文件夹");
                let path_resp = ui.add(
                    egui::TextEdit::singleline(session.path_text_mut())
                        .desired_width(f32::INFINITY),
                );
                let (entered, blurred) =
                    track_draft_field(&path_resp, session.path_focus_mut(), enter);
                if entered {
                    session.note_enter(Field::Path);
                }
                if blurred {
                    session.note_blur(Field::Path);
                }
                ui.horizontal(|ui| {
                    if ui.button("选择文件夹…").clicked() {
                        picks_ref.folder_pick = true;
                    }
                    if ui.button("重新扫描").clicked() {
                        picks_ref.rescan = true;
                    }
                });
                ui.separator();
                ui.label("播放模式");
                ui.horizontal(|ui| {
                    if ui
                        .radio(mode_now == PlaybackMode::Sequential, "顺序")
                        .clicked()
                    {
                        picks_ref.mode = Some(PlaybackMode::Sequential);
                    }
                    if ui.radio(mode_now == PlaybackMode::Random, "随机").clicked() {
                        picks_ref.mode = Some(PlaybackMode::Random);
                    }
                });
                // —— 播放组：切换动画（切换动画专题工单 04）——
                // 与播放模式、切换间隔同组排布：模式 → 动画 → 间隔，
                // 间隔块由外观组之后挪到此处，播放三项自此相邻。
                ui.label("切换动画");
                ui.horizontal(|ui| {
                    for (tier, label) in [
                        (Animation::None, "无"),
                        (Animation::Curl, "卷页"),
                        (Animation::Blinds, "百叶帘"),
                        (Animation::Ink, "墨晕"),
                    ] {
                        if ui.radio(animation_now == tier, label).clicked() {
                            picks_ref.animation = Some(tier);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    for (tier, label) in [
                        (Animation::Card, "抽卡"),
                        (Animation::Pixel, "像素故障"),
                        (Animation::Fade, "淡化"),
                        (Animation::Random, "随机"),
                    ] {
                        if ui.radio(animation_now == tier, label).clicked() {
                            picks_ref.animation = Some(tier);
                        }
                    }
                });
                ui.label("切换间隔");
                for (label, secs) in INTERVAL_PRESETS {
                    if ui.radio(interval_now == secs, label).clicked() {
                        picks_ref.interval = Some(secs);
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("自定义");
                    let seconds_resp = ui.add(
                        egui::TextEdit::singleline(session.seconds_text_mut()).desired_width(110.0),
                    );
                    ui.label("秒");
                    let (entered, blurred) =
                        track_draft_field(&seconds_resp, session.seconds_focus_mut(), enter);
                    if entered {
                        session.note_enter(Field::Seconds);
                    }
                    if blurred {
                        session.note_blur(Field::Seconds);
                    }
                });
                ui.label("外观");
                ui.horizontal(|ui| {
                    if ui
                        .radio(appearance_now == AppearanceMode::Minimal, "极简")
                        .clicked()
                    {
                        picks_ref.appearance = Some(AppearanceMode::Minimal);
                    }
                    if ui
                        .radio(appearance_now == AppearanceMode::Builtin, "内置画框")
                        .clicked()
                    {
                        picks_ref.appearance = Some(AppearanceMode::Builtin);
                    }
                    if ui
                        .radio(appearance_now == AppearanceMode::Custom, "自定义相框图")
                        .clicked()
                    {
                        picks_ref.appearance = Some(AppearanceMode::Custom);
                    }
                });
                // —— 内置画框参数（工单 02）：仅内置模式显示本块 ——
                if appearance_now == AppearanceMode::Builtin {
                    ui.horizontal(|ui| {
                        for (preset, label) in [
                            (FramePreset::Walnut, "胡桃木"),
                            (FramePreset::Oak, "原木"),
                            (FramePreset::Black, "黑"),
                            (FramePreset::White, "白"),
                        ] {
                            if ui.radio(frame_now.preset == preset, label).clicked() {
                                picks_ref.frame_preset = Some(preset);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("框宽");
                        let mut width = frame_now.frame_width;
                        if ui
                            .add(egui::Slider::new(&mut width, 0.0..=MAX_LAYER_WIDTH))
                            .changed()
                        {
                            picks_ref.frame_width = Some(width);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("卡纸宽");
                        let mut width = frame_now.mat_width;
                        if ui
                            .add(egui::Slider::new(&mut width, 0.0..=MAX_LAYER_WIDTH))
                            .changed()
                        {
                            picks_ref.mat_width = Some(width);
                        }
                    });
                }
                // —— 内置画框参数块结束 ——
                // ---- 自定义相框图（工单 03）：选图与分析状态 ----
                if appearance_now == AppearanceMode::Custom {
                    ui.horizontal(|ui| {
                        if ui.button("选择相框图…").clicked() {
                            picks_ref.frame_art_pick = true;
                        }
                        ui.label(frame_art_path_now.as_deref().unwrap_or("未选择"));
                    });
                    if let Some(status) = &frame_art_status_now {
                        ui.label(status.as_str());
                    }
                }
                // ---- 自定义相框图区块结束 ----
                if current_showing.is_some() {
                    ui.separator();
                    ui.label("当前图");
                    ui.horizontal(|ui| {
                        if ui.button("打开当前原图").clicked() {
                            picks_ref.open_image = true;
                        }
                        if ui.button("打开所在文件夹").clicked() {
                            picks_ref.open_folder = true;
                        }
                        if ui.button("复制图片路径").clicked() {
                            picks_ref.copy_path = true;
                        }
                    });
                }
                ui.separator();
                if ui.button("恢复默认位置与大小").clicked() {
                    picks_ref.restore_default = true;
                }
                ui.separator();
                let mut autostart_checked = autostart_now;
                if ui.checkbox(&mut autostart_checked, "开机自启").clicked() {
                    picks_ref.autostart = Some(autostart_checked);
                }
            });
        });
        let actions = self.session.end_frame(picks, viewport_focused_now);
        self.execute_session_actions(ctx, actions, now);
    }
}
