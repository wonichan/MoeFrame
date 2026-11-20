//! 相框后台工作线程：扫描图库与解码图片都在这里执行，UI 线程只收结果（NFR-6）。
//! 另持有一份预加载缓存：当前图的下一张（工单 11），长期只多留这一份显示数据。

use moe_frame::{
    decode_validation::{DisplayImage, DisplayMode},
    library::scan_library,
    loading::{append_skip_log, load_first_available, load_image, LoadRequest},
};
use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};

pub enum Command {
    Scan {
        folder: PathBuf,
    },
    /// `seq` 由宿主递增：换文件夹后旧一轮的加载结果会被宿主按序号丢弃。
    Load {
        seq: u64,
        candidates: Vec<PathBuf>,
        request: LoadRequest,
    },
    /// 预加载当前图的下一张到缓存，命中时 Load 直接取用，不回送结果。
    Preload {
        path: PathBuf,
        request: LoadRequest,
    },
}

pub enum Reply {
    ScanCompleted(Vec<PathBuf>),
    Loaded {
        seq: u64,
        image: Option<DisplayImage>,
    },
}

struct Cached {
    path: PathBuf,
    target: [u32; 2],
    mode: DisplayMode,
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
                // 预加载命中：首候选正是缓存的下一张且目标/模式未变，直接交出。
                let cached_hit = match (&cache, candidates.first()) {
                    (Some(cached), Some(first))
                        if cached.path == *first
                            && cached.target == request.target_pixels
                            && cached.mode == request.mode =>
                    {
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
            Command::Preload { path, request } => {
                // 预加载失败不留缓存：正式加载时会再按候选顺序尝试并记录跳过原因。
                if let Ok(image) = load_image(&path, request) {
                    cache = Some(Cached {
                        path,
                        target: request.target_pixels,
                        mode: request.mode,
                        image,
                    });
                }
            }
        }
    }
}
