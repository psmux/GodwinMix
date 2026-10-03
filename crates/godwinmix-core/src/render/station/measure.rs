//! Taking a calibration on a thread of its own, and keeping it.

use super::candidates;
use super::Station;
use godwinmix_govern::calibrate::{calibrate, Options};
use godwinmix_govern::store::Store;
use godwinmix_govern::Profile;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tracing::{info, warn};

/// How often a calibration waiting for the air to clear looks again.
const WAIT_STEP: Duration = Duration::from_secs(5);

impl Station {
    pub(super) fn spawn(&self, wait: bool) -> bool {
        if self.inner.store.is_none() || self.inner.calibrating.swap(true, Ordering::SeqCst) {
            return false;
        }
        let me = self.clone();
        let started = std::thread::Builder::new()
            .name("governor-calibrate".into())
            .spawn(move || {
                while wait && me.on_air() {
                    std::thread::sleep(WAIT_STEP);
                }
                me.measure();
                me.inner.calibrating.store(false, Ordering::SeqCst);
            });
        if started.is_err() {
            self.inner.calibrating.store(false, Ordering::SeqCst);
        }
        started.is_ok()
    }

    /// In a process of its own when the binary set one up, here otherwise.
    fn measure(&self) {
        let apart = self.inner.apart.lock().clone();
        match apart {
            Some(apart) => self.measure_apart(&apart),
            None => {
                self.measure_here();
            }
        }
    }

    /// Blocking: a few seconds of every encoder this machine has. True when
    /// the result was kept.
    pub(super) fn measure_here(&self) -> bool {
        let cat = crate::catalogue::global();
        let cands = candidates::candidates(&cat, self.inner.pin);
        let audio = candidates::audio(&cat);
        if cands.is_empty() {
            warn!("no video encoder from the catalogue is installed; nothing to calibrate");
            return false;
        }
        info!(encoders = cands.len(), "calibrating this machine's encoders");
        let cal = calibrate(&cands, &audio, &Options::default());
        let mut kept = false;
        if let Some(dir) = &self.inner.store {
            match Store::new(dir).save(&cal) {
                Ok(_) => kept = true,
                Err(e) => warn!(?e, "the calibration could not be kept; it will be taken again next start"),
            }
        }
        info!(took_ms = cal.took_ms, measured = cal.encoders.len(), "calibration done");
        self.inner.governor.set_profile(Profile::from_calibration(cal));
        kept
    }
}
