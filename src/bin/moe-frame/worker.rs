//! 相框后台工作线程：扫描图库与解码图片都在这里执行，UI 线程只收结果（NFR-6）。
//! 另持有一份预加载缓存：当前图的下一张（工单 11），长期只多留这一份显示数据。
//! 缓存的键口径（路径 + 物理目标 + 显示方式 = 同一份显示数据）由库的
//! display_data 定义（architecture-deepening 工单 01），本文件只管存储。

use moe_frame::{
    display_data::DisplayKey,
    library::scan_library,
    loading::{append_skip_log, load_first_available, load_image, DisplayImage, LoadRequest},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender},
};

pub enum Command {
    Scan {
        folder: PathBuf,
    },
    /// `seq` 由 display_data 分配：换文件夹后旧一轮的加载结果会被按序号丢弃。
    Load {
        seq: u64,
        candidates: Vec<PathBuf>,
        request: LoadRequest,
    },
    /// 预加载指定显示数据到缓存，命中时 Load 直接取用，不回送结果。
    Preload {
        key: DisplayKey,
    },
    /// 刷新当前图的显示数据（显示方式或物理目标变化，工单 18/20）：
    /// 与导航加载分开的独立类别，回送只换纹理、不经核心提交。
    Refresh {
        key: DisplayKey,
    },
}

pub enum Reply {
    ScanCompleted(Vec<PathBuf>),
    Loaded {
        seq: u64,
        image: Option<DisplayImage>,
    },
    Refreshed {
        image: Option<DisplayImage>,
    },
}

struct Cached {
    key: DisplayKey,
    image: DisplayImage,
}

pub fn spawn(log_path: PathBuf) -> (Sender<Command>, Receiver<Reply>) {
    let (command_tx, command_rx) = std::sync::mpsc::channel::<Command>();
    let (reply_tx, reply_rx) = std::sync::mpsc::channel::<Reply>();
    std::thread::Builder::new()
        .name("moe-frame-worker".to_owned())
        .spawn(move || run(command_rx, reply_tx, log_path))
        .expect("worker thread must spawn");
    (command_tx, reply_rx)
}

/// 取一份显示数据：预载缓存命中（键完全一致）直接交出，否则现解码。
fn obtain(cache: &mut Option<Cached>, key: &DisplayKey, log_path: &Path) -> Option<DisplayImage> {
    if matches!(&cache, Some(cached) if cached.key == *key) {
        return cache.take().map(|cached| cached.image);
    }
    let request = LoadRequest::product(key.target, key.mode);
    match load_image(&key.path, request) {
        Ok(image) => Some(image),
        Err(error) => {
            // The skip log is best-effort evidence; a failed write must not kill loading.
            let _ = append_skip_log(log_path, &[error]);
            None
        }
    }
}

fn run(commands: Receiver<Command>, replies: Sender<Reply>, log_path: PathBuf) {
    let mut cache: Option<Cached> = None;
    while let Ok(command) = commands.recv() {
        match command {
            Command::Scan { folder } => {
                cache = None;
                let paths = scan_library(&folder);
                if replies.send(Reply::ScanCompleted(paths)).is_err() {
                    return;
                }
            }
            Command::Load {
                seq,
                candidates,
                request,
            } => {
                // 预加载命中：首候选正是缓存的显示数据且口径未变，直接交出。
                let first_key = candidates.first().map(|first| DisplayKey {
                    path: first.clone(),
                    target: request.target_pixels,
                    mode: request.mode,
                });
                let cached_hit = match (&first_key, &cache) {
                    (Some(key), Some(cached)) if cached.key == *key => {
                        cache.take().map(|cached| cached.image)
                    }
                    _ => None,
                };
                let image = match cached_hit {
                    Some(image) => Some(image),
                    None => {
                        let outcome = load_first_available(&candidates, request);
                        if !outcome.skipped.is_empty() {
                            // The skip log is best-effort evidence; a failed write must not kill loading.
                            let _ = append_skip_log(&log_path, &outcome.skipped);
                        }
                        outcome.image
                    }
                };
                if replies.send(Reply::Loaded { seq, image }).is_err() {
                    return;
                }
            }
            Command::Preload { key } => {
                // 预加载失败不留缓存：正式加载时会再按候选顺序尝试并记录跳过原因。
                let request = LoadRequest::product(key.target, key.mode);
                if let Ok(image) = load_image(&key.path, request) {
                    cache = Some(Cached { key, image });
                }
            }
            Command::Refresh { key } => {
                let image = obtain(&mut cache, &key, &log_path);
                if replies.send(Reply::Refreshed { image }).is_err() {
                    return;
                }
            }
        }
    }
}
