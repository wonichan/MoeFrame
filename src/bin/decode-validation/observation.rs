use moe_frame::diagnostics::SessionLog;
use std::{
    io, mem,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use windows_sys::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::System::{
        ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        },
        SystemInformation::OSVERSIONINFOW,
        Threading::GetCurrentProcess,
    },
};

#[derive(Clone, Copy, Debug)]
pub struct ProcessMemory {
    pub working_set_bytes: u64,
    pub peak_working_set_bytes: u64,
    pub private_bytes: u64,
    pub page_fault_count: u32,
}
pub fn process_memory() -> io::Result<ProcessMemory> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    // SAFETY: Current-process pseudo handle is live and must not be closed. The EX struct is
    // initialized, starts with PROCESS_MEMORY_COUNTERS, and has the supplied byte count.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(ProcessMemory {
        working_set_bytes: counters.WorkingSetSize as u64,
        peak_working_set_bytes: counters.PeakWorkingSetSize as u64,
        private_bytes: counters.PrivateUsage as u64,
        page_fault_count: counters.PageFaultCount,
    })
}
pub fn system_version() -> io::Result<String> {
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: Initialized, correctly-sized OSVERSIONINFOW output; read-only system query.
    let status = unsafe { RtlGetVersion(&mut version) };
    if status < 0 {
        return Err(io::Error::other(format!("RtlGetVersion NTSTATUS={status}")));
    }
    Ok(format!(
        "Windows NT {}.{}.{}; architecture={}",
        version.dwMajorVersion,
        version.dwMinorVersion,
        version.dwBuildNumber,
        std::env::consts::ARCH
    ))
}

pub const IDLE: u8 = 0;
pub const AWAITING_UPLOAD: u8 = 10;
pub const UPLOADING: u8 = 11;
pub const SETTLING: u8 = 12;
pub const QUIESCENT: u8 = 13;
pub const CLEARING: u8 = 14;
pub const HIDDEN: u8 = 15;
pub fn phase_name(phase: u8) -> &'static str {
    match phase {
        0 => "idle",
        1 => "opening",
        2 => "decoding",
        3 => "resizing",
        4 => "full_resolution_released",
        5 => "display_ready",
        10 => "awaiting_upload",
        11 => "uploading",
        12 => "settling",
        13 => "quiescent",
        14 => "clearing",
        15 => "hidden",
        _ => "unknown",
    }
}

/// Byte counters describe owned display buffers/texture metadata, NOT process memory.
pub struct Observation {
    log: Mutex<SessionLog>,
    failure: Mutex<Option<String>>,
    pub phase: AtomicU8,
    pub display_rgba_bytes: AtomicUsize,
    pub texture_metadata_bytes: AtomicUsize,
    pub working_set_bytes: AtomicU64,
    pub peak_working_set_bytes: AtomicU64,
    pub samples: AtomicU64,
    pub prepared: AtomicU64,
    pub rejected: AtomicU64,
    pub shown: AtomicU64,
}
impl Observation {
    pub fn new(log: SessionLog) -> Self {
        Self {
            log: Mutex::new(log),
            failure: Mutex::new(None),
            phase: AtomicU8::new(IDLE),
            display_rgba_bytes: AtomicUsize::new(0),
            texture_metadata_bytes: AtomicUsize::new(0),
            working_set_bytes: AtomicU64::new(0),
            peak_working_set_bytes: AtomicU64::new(0),
            samples: AtomicU64::new(0),
            prepared: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            shown: AtomicU64::new(0),
        }
    }
    pub fn fail(&self, error: String) {
        if let Ok(mut failure) = self.failure.lock() {
            if failure.is_none() {
                eprintln!("{error}");
                *failure = Some(error);
            }
        }
    }
    pub fn failure(&self) -> Option<String> {
        self.failure
            .lock()
            .map(|f| f.clone())
            .unwrap_or_else(|_| Some("failure mutex poisoned".into()))
    }
    pub fn record(&self, kind: &str, message: &str) {
        let result = self
            .log
            .lock()
            .map_err(|_| "log mutex poisoned".to_owned())
            .and_then(|mut log| {
                log.record(kind, message)
                    .map_err(|e| format!("validation log write failed: {e}"))
            });
        if let Err(error) = result {
            self.fail(error);
        }
    }
    pub fn sample(&self) {
        match process_memory() {
            Ok(memory) => {
                self.working_set_bytes
                    .store(memory.working_set_bytes, Ordering::Relaxed);
                self.peak_working_set_bytes
                    .store(memory.peak_working_set_bytes, Ordering::Relaxed);
                self.samples.fetch_add(1, Ordering::Relaxed);
                self.record("memory", &format!("phase={}; working_set_bytes={}; peak_working_set_bytes={}; private_bytes={}; page_fault_count={}; display_rgba_bytes={}; texture_metadata_bytes={}",
                    phase_name(self.phase.load(Ordering::Relaxed)), memory.working_set_bytes, memory.peak_working_set_bytes, memory.private_bytes, memory.page_fault_count,
                    self.display_rgba_bytes.load(Ordering::Relaxed), self.texture_metadata_bytes.load(Ordering::Relaxed)));
            }
            Err(error) => {
                self.record("memory_error", &error.to_string());
                self.fail(format!("memory sampling failed: {error}"));
            }
        }
    }
}

pub struct Sampler {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Sampler {
    pub fn start(observation: Arc<Observation>, period: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                observation.sample();
                if observation.failure().is_some() {
                    break;
                }
                thread::park_timeout(period);
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
    pub fn stop(&mut self, observation: &Observation) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            if thread.join().is_err() {
                observation.fail("memory sampler panicked".into());
            }
        }
    }
}
