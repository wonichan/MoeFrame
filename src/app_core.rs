//! 相框应用行为核心（工单 10）：不依赖 GUI 与线程的用户意图 → 状态/效果转换。
//!
//! 宿主（二进制）负责执行效果（弹文件夹选择框、扫描、后台加载、保存配置、退出），
//! 并把结果作为意图送回。核心本身不做任何 IO，因此行为测试可完整驱动这条接缝。

use std::path::PathBuf;

/// 用户与宿主送入的意图/结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    /// 宿主完成启动装配后送入一次。
    Started,
    /// 菜单「选择文件夹」。
    FolderPickRequested,
    /// 文件夹选择完成。
    FolderPicked(PathBuf),
    /// 文件夹选择被取消。
    FolderPickCancelled,
    /// 宿主执行完扫描的效果回送。
    ScanCompleted(Vec<PathBuf>),
    /// 宿主执行完加载的效果回送：成功显示的路径，或全部候选失败。
    LoadCompleted(Option<PathBuf>),
    /// 菜单「退出」。
    ExitRequested,
}

/// 核心要求宿主执行的效果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    PickFolder,
    SaveFolder(PathBuf),
    Scan(PathBuf),
    LoadFirst { candidates: Vec<PathBuf> },
    Exit,
}

/// 相框当前呈现给用户的视图。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    /// 无图状态：可操作，提示右键选择文件夹。
    Empty,
    Showing(PathBuf),
}

#[derive(Default)]
pub struct FrameCore {
    folder: Option<PathBuf>,
    current: Option<PathBuf>,
}

impl FrameCore {
    pub fn new(saved_folder: Option<PathBuf>) -> Self {
        Self {
            folder: saved_folder,
            current: None,
        }
    }

    pub fn view(&self) -> View {
        match &self.current {
            Some(path) => View::Showing(path.clone()),
            None => View::Empty,
        }
    }

    pub fn folder(&self) -> Option<&PathBuf> {
        self.folder.as_ref()
    }

    pub fn handle(&mut self, intent: Intent) -> Vec<Effect> {
        match intent {
            Intent::Started => match &self.folder {
                // 已保存文件夹：直接扫描，不再引导选择。
                Some(folder) => vec![Effect::Scan(folder.clone())],
                // 首次无配置：只引导选择这一次。
                None => vec![Effect::PickFolder],
            },
            Intent::FolderPickRequested => vec![Effect::PickFolder],
            Intent::FolderPicked(folder) => {
                self.folder = Some(folder.clone());
                vec![Effect::SaveFolder(folder.clone()), Effect::Scan(folder)]
            }
            // 取消不改变任何现状：无图时停在无图状态，显示中则继续显示，且不再自动弹窗。
            Intent::FolderPickCancelled => Vec::new(),
            Intent::ScanCompleted(paths) => {
                if paths.is_empty() {
                    self.current = None;
                    Vec::new()
                } else {
                    vec![Effect::LoadFirst { candidates: paths }]
                }
            }
            Intent::LoadCompleted(shown) => {
                self.current = shown;
                Vec::new()
            }
            Intent::ExitRequested => vec![Effect::Exit],
        }
    }
}
