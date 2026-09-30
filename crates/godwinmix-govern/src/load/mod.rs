//! Live load: what the machine and this process are doing, once a second.
//!
//! [`Probe`] turns the operating system's counters into thousandths of a
//! core. [`Window`] smooths them and keeps the recent peak of everything that
//! is not this process. [`Sampler`] runs the two on a thread of its own and
//! writes the result into a [`LoadCell`] that anyone reads without a lock.

mod cell;
pub mod sys;
mod window;

pub use cell::{Load, LoadCell};
pub use window::Window;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// One reading, differences since the one before.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    /// Every process on the machine, thousandths of a core.
    pub system_millicores: u32,
    /// This process alone.
    pub own_millicores: u32,
    pub available_mib: Option<u64>,
    /// Devices that say how busy they are, thousandths.
    pub devices: Vec<(String, u32)>,
}

/// The counters as last read, so the next read is a difference.
pub struct Probe {
    cores: u32,
    last_sys: Option<(u64, u64)>,
    last_own: Option<(u64, Instant)>,
}

impl Probe {
    /// Reads the counters once so the first [`Probe::read`] has a start.
    pub fn new() -> Probe {
        Probe {
            cores: crate::fingerprint::cores(),
            last_sys: sys::system_ticks(),
            last_own: sys::process_cpu_ns().map(|n| (n, Instant::now())),
        }
    }

    pub fn read(&mut self) -> Reading {
        let now_sys = sys::system_ticks();
        let system = match (self.last_sys, now_sys) {
            (Some((b0, t0)), Some((b1, t1))) if t1 > t0 => {
                let frac = (b1.saturating_sub(b0)) as f64 / (t1 - t0) as f64;
                (frac * f64::from(self.cores) * 1000.0).round() as u32
            }
            _ => 0,
        };
        self.last_sys = now_sys.or(self.last_sys);
        let now = Instant::now();
        let own = match (self.last_own, sys::process_cpu_ns()) {
            (Some((n0, at)), Some(n1)) => {
                let wall = now.duration_since(at).as_nanos().max(1) as f64;
                let v = (n1.saturating_sub(n0) as f64 / wall * 1000.0).round() as u32;
                self.last_own = Some((n1, now));
                v
            }
            (_, Some(n1)) => {
                self.last_own = Some((n1, now));
                0
            }
            _ => 0,
        };
        Reading {
            system_millicores: system.max(own),
            own_millicores: own,
            available_mib: sys::memory_mib().map(|(_, a)| a),
            devices: sys::device_busy(),
        }
    }
}

impl Default for Probe {
    fn default() -> Self {
        Self::new()
    }
}

/// The thread that samples. Stops when dropped.
pub struct Sampler {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Sampler {
    /// Sample every `period` into `cell` until dropped.
    pub fn start(cell: Arc<LoadCell>, period: Duration) -> std::io::Result<Sampler> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new().name("gmx-govern-load".into()).spawn(move || {
            let mut probe = Probe::new();
            let mut window = Window::new(10);
            while !flag.load(Ordering::Relaxed) {
                std::thread::park_timeout(period);
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                cell.store(&window.push(probe.read()));
            }
        })?;
        Ok(Sampler { stop, thread: Some(thread) })
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests;
