//! Taking the calibration in a process of its own.
//!
//! Calibrating loads every hardware encoder's driver, and a driver can bring
//! its process down while it is loaded: on an Intel Arc 140T with GStreamer
//! 1.28.6, the qsv plugin corrupts the heap while it registers its encoders
//! in about one load in four. In the station's own process that took every
//! show with it. In a child it costs that child, and the station tries again.

use super::candidates;
use super::Station;
use godwinmix_govern::calibrate::fingerprint_for;
use godwinmix_govern::store::Store;
use godwinmix_govern::Profile;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use tracing::{info, warn};

/// How many times a calibration that died is started again before the
/// station gives up until its next start.
const ATTEMPTS: u32 = 3;

/// The flag a calibrating child is started with, followed by the directory
/// it keeps the result in.
pub const CALIBRATE_FLAG: &str = "--calibrate-into";

/// How to start a calibrating child: this program, and the arguments that
/// give it the same config and catalogue as the process asking.
#[derive(Debug, Clone)]
pub struct Apart {
    pub exe: PathBuf,
    pub args: Vec<String>,
}

impl Station {
    /// Measure in a child process from now on.
    pub fn measure_apart_with(&self, apart: Apart) {
        *self.inner.apart.lock() = Some(apart);
    }

    /// What the child runs: measure and keep the result in `dir`, here.
    /// False when nothing was kept.
    pub fn calibrate_here(pin: crate::config::Accel, dir: &Path) -> bool {
        let governor = godwinmix_govern::Governor::new(Default::default(), Profile::uncalibrated());
        let st = Self::build(governor, &crate::catalogue::global(), pin, Some(dir.to_path_buf()));
        st.measure_here()
    }

    pub(super) fn measure_apart(&self, apart: &Apart) {
        let Some(dir) = self.inner.store.clone() else { return };
        let fp = fingerprint_for(&candidates::candidates(&crate::catalogue::global(), self.inner.pin));
        for attempt in 1..=ATTEMPTS {
            match run(apart, &dir) {
                Ok(()) => {
                    let store = Store::new(&dir);
                    match store.load(&fp).or_else(|| store.latest()) {
                        Some(cal) => {
                            info!(took_ms = cal.took_ms, measured = cal.encoders.len(), "calibration done in its own process");
                            self.inner.governor.set_profile(Profile::from_calibration(cal));
                        }
                        None => warn!(dir = %dir.display(), "the calibration process finished but kept nothing; the governor keeps its stand in figures"),
                    }
                    return;
                }
                Err(why) => warn!(attempt, of = ATTEMPTS, why, "the calibration process failed; nothing else was touched"),
            }
        }
        warn!("calibration failed {ATTEMPTS} times; the governor keeps its stand in figures and measures again on the next start");
    }
}

fn run(apart: &Apart, dir: &Path) -> Result<(), String> {
    let status = Command::new(&apart.exe)
        .args(&apart.args)
        .arg(CALIBRATE_FLAG)
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map_err(|e| format!("could not start {}: {e}", apart.exe.display()))?;
    match status.success() {
        true => Ok(()),
        false => Err(describe(status)),
    }
}

/// An exit code as Windows writes its crash codes (`0xc0000374` is heap
/// corruption), or the signal on Unix.
fn describe(status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("killed by signal {signal}");
        }
    }
    match status.code() {
        Some(code) => format!("exit code {:#010x}", code as u32),
        None => status.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_that_fails_is_reported_and_not_fatal() {
        let apart = Apart { exe: PathBuf::from("this-program-does-not-exist-anywhere"), args: vec![] };
        let why = run(&apart, Path::new(".")).unwrap_err();
        assert!(why.contains("could not start"), "{why}");
    }
}
