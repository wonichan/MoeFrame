//! 相框后台工作线程：扫描图库与解码图片都在这里执行，UI 线程只收结果（NFR-6）。

use moe_frame::{
    decode_validation::DisplayImage,
    library::scan_library,
    loading::{append_skip_log, load_first_available, LoadRequest},
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
    LoadFirst {
        seq: u64,
        candidates: Vec<PathBuf>,
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
    while let Ok(command) = commands.recv() {
        match command {
            Command::Scan { folder } => {
                let paths = scan_library(&folder);
                if replies.send(Reply::ScanCompleted(paths)).is_err() {
                    return;
                }
            }
            Command::LoadFirst {
                seq,
                candidates,
                request,
            } => {
                let outcome = load_first_available(&candidates, request);
                if !outcome.skipped.is_empty() {
                    // The skip log is best-effort evidence; a failed write must not kill loading.
                    let _ = append_skip_log(&log_path, &outcome.skipped);
                }
                let reply = Reply::Loaded {
                    seq,
                    image: outcome.image,
                };
                if replies.send(reply).is_err() {
                    return;
                }
            }
        }
    }
}
