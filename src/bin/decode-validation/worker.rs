use super::observation::{self, Observation};
use eframe::egui;
use moe_frame::decode_validation::{
    CycleEvent, CycleSnapshot, DecodePhase, DecodeRequest, ValidationCycle,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc,
    },
    thread::{self, JoinHandle},
};

pub enum Command {
    Configure(DecodeRequest),
    PreloadNext,
    Advance,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReadyKind {
    /// 当前图已备好，可以先显示；下一张尚未触碰。
    Current,
    /// 当前图显示后请求的下一张已备好。
    Preload,
    /// 已提升预加载图为当前图；新的下一张尚未解码。
    Advanced,
}
pub struct Ready {
    pub snapshot: CycleSnapshot,
    pub kind: ReadyKind,
}

/// One in-flight UI request and capacity-one replies; no full-size pixels enter either channel.
pub struct Worker {
    commands: Option<SyncSender<Command>>,
    ready: Option<Receiver<Ready>>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(paths: Vec<PathBuf>, observation: Arc<Observation>, ctx: egui::Context) -> Self {
        let (commands, receive) = mpsc::sync_channel(1);
        let (send, ready) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let mut cycle: Option<ValidationCycle> = None;
            let observe = |event: CycleEvent| {
                match &event {
                    CycleEvent::Phase { phase, .. } => {
                        let phase = match phase {
                            DecodePhase::Opening => 1,
                            DecodePhase::Decoding => 2,
                            DecodePhase::Resizing => 3,
                            DecodePhase::FullResolutionReleased => 4,
                            DecodePhase::DisplayReady => 5,
                        };
                        observation.phase.store(phase, Ordering::Relaxed);
                    }
                    CycleEvent::Prepared { .. } => {
                        observation.prepared.fetch_add(1, Ordering::Relaxed);
                    }
                    CycleEvent::Rejected { .. } => {
                        observation.rejected.fetch_add(1, Ordering::Relaxed);
                    }
                    CycleEvent::Retired { .. } => {}
                }
                observation.record("decode_event", &format!("{event:?}"));
                observation.sample();
            };
            while let Ok(command) = receive.recv() {
                let (snapshot, kind) = match command {
                    Command::Configure(request) => {
                        if let Some(old) = &mut cycle {
                            old.clear(observe);
                        }
                        let mut new = ValidationCycle::new(paths.clone(), request);
                        let snapshot = new.prime_current(observe);
                        cycle = Some(new);
                        (snapshot, ReadyKind::Current)
                    }
                    Command::PreloadNext => {
                        let Some(cycle) = &mut cycle else {
                            observation.fail("preload before configure".into());
                            break;
                        };
                        (cycle.preload_next(observe), ReadyKind::Preload)
                    }
                    Command::Advance => {
                        let Some(cycle) = &mut cycle else {
                            observation.fail("advance before configure".into());
                            break;
                        };
                        (cycle.advance(observe), ReadyKind::Advanced)
                    }
                };
                observation
                    .display_rgba_bytes
                    .store(snapshot.resident_rgba_bytes(), Ordering::Relaxed);
                observation
                    .phase
                    .store(observation::AWAITING_UPLOAD, Ordering::Relaxed);
                if send.send(Ready { snapshot, kind }).is_err() {
                    break;
                }
                ctx.request_repaint();
                if observation.failure().is_some() {
                    break;
                }
            }
            if let Some(cycle) = &mut cycle {
                cycle.clear(observe);
            }
            observation.display_rgba_bytes.store(0, Ordering::Relaxed);
            observation.record("worker_stopped", "current and next cycle slots cleared");
        });
        Self {
            commands: Some(commands),
            ready: Some(ready),
            thread: Some(thread),
        }
    }
    pub fn submit(&self, command: Command) -> Result<(), String> {
        self.commands
            .as_ref()
            .ok_or("worker stopped")?
            .try_send(command)
            .map_err(|e| format!("worker request refused: {e}"))
    }
    pub fn poll(&self) -> Result<Option<Ready>, String> {
        match self.ready.as_ref().ok_or("worker stopped")?.try_recv() {
            Ok(ready) => Ok(Some(ready)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err("decode worker disconnected".into()),
        }
    }
    pub fn stop(&mut self, observation: &Observation) {
        // Dropping replies first unblocks a send if shutdown races the last preparation.
        self.ready.take();
        self.commands.take();
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                observation.fail("decode worker panicked".into());
            }
        }
    }
}
