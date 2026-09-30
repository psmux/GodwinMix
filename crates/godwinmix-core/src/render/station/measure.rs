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

    /// Blocking: a few seconds of every encoder this machine has.
    fn measure(&self) {
        let cat = crate::catalogue::global();
        let cands = candidates::candidates(&cat, self.inner.pin);
        let audio = candidates::audio(&cat);
        if cands.is_empty() {
            warn!("no video encoder from the catalogue is installed; nothing to calibrate");
            return;
        }
        info!(encoders = cands.len(), "calibrating this machine's encoders");
        let cal = calibrate(&cands, &audio, &Options::default());
        if let Some(dir) = &self.inner.store {
            if let Err(e) = Store::new(dir).save(&cal) {
                warn!(?e, "the calibration could not be kept; it will be taken again next start");
            }
        }
        info!(took_ms = cal.took_ms, measured = cal.encoders.len(), "calibration done");
        self.inner.governor.set_profile(Profile::from_calibration(cal));
    }
}
