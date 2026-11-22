//! 宿主状态与效果执行：FrameApp 字段、核心效果分发、回复与自启落地。

use super::*;

pub(crate) struct FrameApp {
    pub(crate) core: FrameCore,
    pub(crate) sync: ConfigSync,
    pub(crate) command_tx: Sender<Command>,
    pub(crate) reply_rx: Receiver<Reply>,
    pub(crate) tray_event_rx: Receiver<tray_icon::TrayIconEvent>,
    pub(crate) menu_event_rx: Receiver<tray_icon::menu::MenuEvent>,
    pub(crate) started: bool,
    /// 首张图的纹理已上传：只用于启动耗时标记，不参与行为。
    pub(crate) first_image_marked: bool,
    pub(crate) texture: Option<egui::TextureHandle>,
    /// 加载完成、等待核心完成显示提交的目标纹理。
    pub(crate) incoming: Option<egui::TextureHandle>,
    /// 本次左键按下时窗口的外框位置：右键取消拖动时回滚到这里（工单 15）。
    pub(crate) press_window_pos: Option<egui::Pos2>,
    /// 托盘（工单 17）：首帧懒建（须在事件循环内建）；建失败为 None，相框照常。
    pub(crate) tray: Option<TrayHandles>,
    pub(crate) tray_tried: bool,
    /// 上次同步到托盘菜单的（可见性、播放中）状态：None 表示尚未同步，
    /// 首帧从核心现读一次，之后只在变化时改文案。
    pub(crate) tray_state: Option<(bool, bool)>,
    /// 已明确选择退出（工单 17）：关闭请求不再拦截，托盘图标先行移除。
    pub(crate) exiting: bool,
    /// 设置窗（工单 18）：是否打开、打开时是否已定位到屏幕中央。
    pub(crate) settings_open: bool,
    pub(crate) settings_positioned: bool,
    /// 显示数据管线（architecture-deepening 工单 01）：显示数据的记账、
    /// 转正、过期判定与刷新决策都在库 display_data 里；纹理句柄归宿主，
    /// 按它的当前/传入槽位对应持有。
    pub(crate) pipeline: DisplayPipeline,
    /// 治愈保活截止（宿主毫秒时钟）：当前图口径过期且目标刚变化时，
    /// 帧尾在此之前保持 50ms 唤醒，让管线的停稳计时与刷新回送不依赖
    /// 外部唤醒（失焦切换模糊修复，.scratch/blur-heal 工单 01）。
    /// 0 表示未激活。
    pub(crate) heal_repaint_until_ms: u64,
    /// 已为哪个过期目标设过保活：同一目标只设一次，防止刷新失败后
    /// 保活被每帧无限延长。
    pub(crate) stale_target_seen: Option<[u32; 2]>,
    /// 窗口几何状态机（工单 20 的生命周期，architecture-deepening
    /// 工单 05 起归库 window_geom）：宿主每帧喂观察、执行命令，几何
    /// 落盘经 config_sync，退出结算经它的 exit_settle 交接。
    pub(crate) geometry: GeometryMachine,
    /// 开机自启的真实状态（工单 21）：设置窗读它渲染开关；注册表实现
    /// 经接缝注入，切换失败时状态不动（回滚）。
    pub(crate) autostart: AutostartSync,
    pub(crate) autostart_store: RegAutostartStore,
    /// 当前外观模式（V2 相框外观专题工单 01）：宿主持有，几何观察
    /// 与解码口径的四边内距都由它给出；设置窗切换时更新本字段并
    /// 经 config_sync 写回。本工单三模式内距均为全 0（占位）。
    pub(crate) appearance: AppearanceMode,
    /// 设置窗会话（工单 19 的时序，architecture-deepening 工单 02 起归
    /// 库 settings_session）：草稿、基线、待结算与焦点记忆都在其中，
    /// 宿主只采样事件、渲染、执行它返回的动作。
    pub(crate) session: SettingsSession,
    pub(crate) debug_last_inner: Option<[f32; 2]>,
}
impl FrameApp {
    pub(crate) fn dispatch(&mut self, ctx: &egui::Context, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::PickFolder => {
                    let picked = rfd::FileDialog::new()
                        .set_title("选择图片文件夹")
                        .pick_folder();
                    let intent = match picked {
                        Some(folder) => Intent::FolderPicked(folder),
                        None => Intent::FolderPickCancelled,
                    };
                    let follow_up = self.core.handle(intent);
                    self.dispatch(ctx, follow_up);
                }
                Effect::SaveFolder(folder) => {
                    self.sync.save_folder(&folder);
                }
                Effect::Scan(folder) => {
                    // 新一轮扫描开始：在飞的旧加载结果从此作废；
                    self.pipeline.invalidate();
                    // 新一轮扫描开始：作废在飞加载，画面回到核心仍在显示的源图。
                    self.incoming = None;
                    let _ = self.command_tx.send(Command::Scan { folder });
                }
                Effect::StartTransition { .. } => {
                    let effects = self.core.handle(Intent::TransitionCompleted {
                        now_ms: now_ms(ctx),
                    });
                    self.dispatch(ctx, effects);
                }
                Effect::Load { candidates } => {
                    // 序号与在飞口径记账归显示数据管线：回送按序号裁决，
                    // 转正时当前槽记为这次加载的口径。
                    let target = self.physical_target(ctx);
                    let seq = self.pipeline.begin_load(DisplayMode::Fit, target);
                    let request = LoadRequest::product(target, DisplayMode::Fit);
                    let _ = self.command_tx.send(Command::Load {
                        seq,
                        candidates,
                        request,
                    });
                }
                Effect::Preload { path } => {
                    let key = DisplayKey {
                        path,
                        target: self.physical_target(ctx),
                        mode: DisplayMode::Fit,
                    };
                    let _ = self.command_tx.send(Command::Preload { key });
                }
                Effect::SaveInterval(secs) => {
                    self.sync.save_interval(secs);
                }
                Effect::SavePlaybackMode(mode) => {
                    self.sync.save_playback_mode(mode);
                }
                Effect::ClearResumeState => {
                    // 图库已空：当前图、本轮进度与历史一并清掉（工单 16）。
                    self.sync.clear_resume_state();
                }
                Effect::SaveResumeState {
                    current,
                    seen,
                    history,
                } => {
                    self.sync.save_resume_state(&current, &seen, &history);
                }
                Effect::OpenImage { path } => {
                    open_image(&path);
                }
                Effect::RollbackFrameDrag => {
                    if let Some(pos) = self.press_window_pos.take() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                    }
                }
                Effect::HideFrame => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                Effect::ShowFrame => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Effect::RaiseFrame => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                Effect::Exit => {
                    // 退出收尾归 config_sync：几何状态机交出（最近
                    // 观察值、落盘基准），shutdown 一次结清（工单 20
                    // 的退出冲刷、工单 04 的收尾职责）。
                    let (observed, saved) = self.geometry.exit_settle();
                    self.sync.shutdown(observed, saved);
                    // 明确退出（工单 17）：先摘托盘图标再关窗口，关闭请求不拦截。
                    self.exiting = true;
                    self.tray = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}
impl FrameApp {
    pub(crate) fn apply_autostart(&mut self, target: bool) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let command = autostart_command(&exe);
        if let Some(value) = self
            .autostart
            .apply(&mut self.autostart_store, target, &command)
        {
            self.sync.save_autostart_mirror(value);
        }
    }
    pub(crate) fn drain_replies(&mut self, ctx: &egui::Context) {
        while let Ok(reply) = self.reply_rx.try_recv() {
            match reply {
                Reply::ScanCompleted(paths) => {
                    let effects = self.core.handle(Intent::ScanCompleted(paths));
                    self.dispatch(ctx, effects);
                    // 重扫后图库为空：核心回到无图状态，画面同步清掉（工单 16）。
                    if self.core.view() == View::Empty {
                        self.texture = None;
                        self.incoming = None;
                        self.pipeline.drop_incoming();
                        self.pipeline.clear_current();
                    }
                }
                Reply::Loaded { seq, image } => {
                    // 序号裁决归管线：过期回送（换文件夹之前）不碰当前
                    // 画面与状态，也不通知核心。
                    let decision = self
                        .pipeline
                        .nav_reply(seq, image.as_ref().map(|image| image.path.as_path()));
                    if decision == NavDecision::Stale {
                        continue;
                    }
                    let shown = image.as_ref().map(|image| image.path.clone());
                    if let Some(image) = image {
                        self.geometry.set_image_aspect(Some(
                            image.source_pixels[0] as f32 / image.source_pixels[1] as f32,
                        ));
                        self.upload(ctx, image);
                    }
                    let effects = self.core.handle(Intent::LoadCompleted {
                        shown,
                        now_ms: now_ms(ctx),
                    });
                    self.dispatch(ctx, effects);
                    if self.incoming.is_some() {
                        self.texture = self.incoming.take();
                        self.pipeline.promote_incoming();
                    }
                    // 导航全失败时核心保留当前图，纹理不动；只有核心回到无图状态才清纹理。
                    if self.core.view() == View::Empty {
                        self.texture = None;
                        self.pipeline.clear_current();
                    }
                }
                Reply::Refreshed { image } => {
                    // 当前图刷新只替换纹理；失败或图像比例变更时保留旧图状态。
                    let showing = match self.core.view() {
                        View::Showing(path) => Some(path.clone()),
                        _ => None,
                    };
                    let decision = self.pipeline.refresh_reply(
                        image.as_ref().map(|image| image.path.as_path()),
                        showing.as_deref(),
                        false,
                    );
                    if decision == RefreshDecision::Swap {
                        if let Some(image) = image {
                            self.geometry.set_image_aspect(Some(
                                image.source_pixels[0] as f32 / image.source_pixels[1] as f32,
                            ));
                            self.upload(ctx, image);
                            self.texture = self.incoming.take();
                        }
                    }
                }
            }
        }
        self.refresh_display_data(ctx);
    }
}
