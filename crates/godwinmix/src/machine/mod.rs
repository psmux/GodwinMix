//! The machine a station runs on, for a headend wall: its NVIDIA GPUs
//! (`gpu.status`) and the box itself (`machine.status`).
//!
//! Nothing is read until a client asks. The first ask starts one thread
//! that reads every GPU and the CPU once a second and keeps two minutes of
//! GPU readings; the thread ends once nobody has asked for ten seconds, and
//! the readings go with it. A page polling once a second keeps it alive;
//! a station nobody watches reads nothing.
//!
//! The GPU is read through NVML where the driver's library can be opened
//! (Linux, `nvml.rs`), else with one `nvidia-smi` query a second (`smi.rs`),
//! else not at all: a Mac, or a box with no NVIDIA card, answers no GPUs and
//! a note saying why.

mod gpu;
mod host;
pub mod methods;
#[cfg(target_os = "linux")]
mod nvml;
pub mod smi;

use godwinmix_protocol::machine::{GpuStatus, MachineStatus};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

/// How often a reading is taken.
pub const EVERY: Duration = Duration::from_secs(1);
/// How long the thread runs after the last ask.
const IDLE_MS: u64 = 10_000;
/// Readings kept per GPU: two minutes.
pub const KEEP: usize = 120;

#[derive(Default)]
struct Held {
    gpu: GpuStatus,
    cpu_pct: Option<f64>,
    ticks: Option<(u64, u64)>,
    /// True once the first reading is in.
    read: bool,
}

#[derive(Default)]
struct Sampler {
    held: Mutex<Held>,
    running: AtomicBool,
    asked_ms: AtomicU64,
}

fn sampler() -> &'static Sampler {
    static S: OnceLock<Sampler> = OnceLock::new();
    S.get_or_init(Sampler::default)
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Note an ask, and start the thread if it is not running. Answers whether
/// a first reading is already in.
fn ask() -> bool {
    let s = sampler();
    s.asked_ms.store(now_ms(), Ordering::Release);
    if !s.running.swap(true, Ordering::AcqRel) {
        let spawned = std::thread::Builder::new().name("gmx-machine".into()).spawn(run);
        if spawned.is_err() {
            s.running.store(false, Ordering::Release);
        }
    }
    s.held.lock().read
}

fn run() {
    let s = sampler();
    let mut reader = gpu::Reader::open();
    loop {
        let gpus = reader.read();
        let ticks = godwinmix_govern::load::sys::system_ticks();
        {
            let mut h = s.held.lock();
            h.gpu.source = reader.source().into();
            h.gpu.note = reader.note();
            h.gpu.every_ms = EVERY.as_millis() as u32;
            gpu::keep(&mut h.gpu.gpus, gpus);
            h.cpu_pct = cpu_between(h.ticks, ticks).or(h.cpu_pct);
            h.ticks = ticks.or(h.ticks);
            h.read = true;
        }
        std::thread::sleep(EVERY);
        if now_ms().saturating_sub(s.asked_ms.load(Ordering::Acquire)) > IDLE_MS {
            *s.held.lock() = Held::default();
            s.running.store(false, Ordering::Release);
            return;
        }
    }
}

/// Percent of every core busy between two `/proc/stat` style readings.
fn cpu_between(was: Option<(u64, u64)>, now: Option<(u64, u64)>) -> Option<f64> {
    let ((b0, t0), (b1, t1)) = (was?, now?);
    let total = t1.checked_sub(t0).filter(|t| *t > 0)?;
    Some((b1.saturating_sub(b0) as f64 * 100.0 / total as f64).clamp(0.0, 100.0))
}

/// Wait a little for the first reading after a start, so the first answer
/// is not empty. `nvidia-smi` takes a few hundred milliseconds at worst.
async fn first_reading() {
    for _ in 0..30 {
        if ask() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// `gpu.status`: every GPU's newest reading and its history after `since`.
pub async fn gpu_status(since_ms: Option<u64>) -> GpuStatus {
    first_reading().await;
    let mut out = sampler().held.lock().gpu.clone();
    if out.source.is_empty() {
        out.source = "none".into();
    }
    let since = since_ms.unwrap_or(0);
    for d in &mut out.gpus {
        d.history.retain(|x| x.at_ms > since);
    }
    out
}

/// `machine.status`: the box's CPU, memory, uptime and load.
pub async fn machine_status() -> MachineStatus {
    first_reading().await;
    let cpu_pct = sampler().held.lock().cpu_pct.map(|p| (p * 10.0).round() / 10.0);
    host::status(cpu_pct)
}

#[cfg(test)]
mod tests {
    #[test]
    fn cpu_is_the_busy_share_of_the_ticks_between_two_readings() {
        assert_eq!(super::cpu_between(Some((100, 1000)), Some((350, 2000))), Some(25.0));
        assert_eq!(super::cpu_between(None, Some((1, 2))), None);
        assert_eq!(super::cpu_between(Some((5, 10)), Some((5, 10))), None);
    }
}
