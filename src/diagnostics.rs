//! 验证程序的真实文件日志。None 表示不可得，不是通过。

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowObservation {
    pub logical_client: Option<[f32; 2]>,
    pub physical_client: Option<[u32; 2]>,
    pub win32_client: Option<[i32; 2]>,
    pub system_logical_client: Option<[f64; 2]>,
    pub window_rect: Option<[i32; 4]>,
    pub dpi: Option<u32>,
    pub native_scale_factor: Option<f64>,
    pub pixels_per_point: Option<f32>,
    pub zoom_factor: Option<f32>,
    pub maximized: Option<bool>,
    pub egui_maximized: Option<bool>,
    pub fullscreen: Option<bool>,
    pub foreground: Option<bool>,
    pub foreground_hwnd: Option<usize>,
    pub keyboard_focus: Option<bool>,
    pub winit_focus: Option<bool>,
    pub visible: Option<bool>,
    pub topmost: Option<bool>,
    pub hwnd: Option<usize>,
    pub window_style: Option<isize>,
    pub window_ex_style: Option<isize>,
    pub dpi_awareness: Option<i32>,
    pub dpi_awareness_v2: Option<bool>,
    pub error: Option<String>,
}

pub struct SessionLog {
    file: File,
    path: PathBuf,
    started: Instant,
}

impl SessionLog {
    pub fn create(directory: &Path, metadata: &str) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        for attempt in 0..16 {
            let path = directory.join(format!(
                "validation-{stamp}-{}-{attempt}.log",
                std::process::id()
            ));
            let file = match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let mut log = Self {
                file,
                path,
                started: Instant::now(),
            };
            log.record("environment", &format!(
                "schema=window-validation-v1; None means unknown; requests and observations are not human acceptance; {metadata}"
            ))?;
            return Ok(log);
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique validation log",
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// One escaped record per line. Do not let metadata or errors forge another record.
    pub fn record(&mut self, kind: &str, message: &str) -> io::Result<()> {
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_millis();
        // Escaping keeps one record per line; this bar also blocks a forged verdict token.
        let message = message.replace("verdict=", "verdict_");
        writeln!(
            self.file,
            "unix_ms={unix_ms} elapsed_ms={} kind={kind:?} details={:?} verdict=unverified",
            self.started.elapsed().as_millis(),
            message
        )?;
        self.file.flush()
    }

    pub fn observe(&mut self, reason: &str, observation: &WindowObservation) -> io::Result<()> {
        self.record(reason, &format!("{observation:?}"))
    }
}
