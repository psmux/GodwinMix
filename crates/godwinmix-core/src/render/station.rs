//! The governor for this process: one per station, sampling the machine from
//! the moment the core starts, and a calibration taken in the background the
//! first time this machine (or this hardware, drivers and GStreamer) is seen,
//! once nothing is on air.

use super::candidates;
use crate::catalogue::Catalogue;
use crate::config::Accel;
use godwinmix_govern::calibrate::fingerprint_for;
use godwinmix_govern::store::{decide, Decision, Store};
use godwinmix_govern::{Governor, GovernorConfig, Profile};
use godwinmix_protocol::rendition::{AudioCodec, EncoderSlot};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

struct Inner {
    governor: Governor,
    pin: Accel,
    slots: Vec<EncoderSlot>,
    audio: Vec<AudioCodec>,
    store: Option<PathBuf>,
    on_air: AtomicBool,
    calibrating: AtomicBool,
    /// Nothing stored for this machine: measure once the air is clear.
    wanted: AtomicBool,
}

/// Cheap to clone; every clone is the same station.
#[derive(Clone)]
pub struct Station {
    inner: Arc<Inner>,
}

impl Station {
    /// A station on `governor` with the catalogue's encoders, and nothing
    /// started: no sampler, no calibration. For tests and for `gmx bench`.
    pub fn with_governor(governor: Governor, cat: &Catalogue, pin: Accel) -> Station {
        Self::build(governor, cat, pin, None)
    }

    fn build(governor: Governor, cat: &Catalogue, pin: Accel, store: Option<PathBuf>) -> Station {
        let reg = crate::catalogue::select::GstRegistry;
        let slots = candidates::video_entries(cat, pin, &reg)
            .into_iter()
            .filter_map(candidates::slot_of)
            .collect();
        let audio = candidates::audio(cat).into_iter().map(|a| a.codec).collect();
        Station {
            inner: Arc::new(Inner {
                governor,
                pin,
                slots,
                audio,
                store,
                on_air: AtomicBool::new(false),
                calibrating: AtomicBool::new(false),
                wanted: AtomicBool::new(false),
            }),
        }
    }

    /// The station a running core has: the stored calibration for this
    /// machine if there is one and the sampler running. When there is none,
    /// [`Station::begin`] measures in the background once nothing is on air.
    pub fn start(config: GovernorConfig, cat: &Catalogue, pin: Accel, data_dir: &Path) -> Station {
        let governor = Governor::new(config, Profile::uncalibrated());
        if let Err(e) = governor.start_sampling() {
            warn!(?e, "the governor cannot read this machine's load; it admits from its book alone");
        }
        let st = Self::build(governor, cat, pin, Some(data_dir.to_path_buf()));
        let cands = candidates::candidates(cat, pin);
        let fp = fingerprint_for(&cands);
        let store = Store::new(data_dir);
        match decide(&store, &fp, false, false) {
            Decision::Use(cal) => {
                info!(fingerprint = %fp, "the governor uses this machine's calibration");
                st.inner.governor.set_profile(Profile::from_calibration(cal));
            }
            other => {
                // Another machine's figures (or this one's before a new
                // driver) are a better guess than none while it measures.
                let stand_in = match other {
                    Decision::Wait { stand_in } => stand_in,
                    _ => store.latest(),
                };
                if let Some(cal) = stand_in {
                    st.inner.governor.set_profile(Profile::from_calibration(cal));
                }
                st.inner.wanted.store(true, Ordering::SeqCst);
            }
        }
        st
    }

    /// Called once the core is up and the mixer has said whether anything
    /// is on air: starts the first run calibration if this machine needs one.
    pub fn begin(&self) -> bool {
        self.inner.wanted.swap(false, Ordering::SeqCst) && self.calibrate_when_clear()
    }

    pub fn governor(&self) -> &Governor {
        &self.inner.governor
    }

    pub fn slots(&self) -> Vec<EncoderSlot> {
        self.inner.slots.clone()
    }

    pub fn audio(&self) -> Vec<AudioCodec> {
        self.inner.audio.clone()
    }

    /// Whether anything is going out. The mixer says so as outputs come
    /// and go; a calibration never starts while it is true.
    pub fn set_on_air(&self, on: bool) {
        self.inner.on_air.store(on, Ordering::Relaxed);
    }

    pub fn on_air(&self) -> bool {
        self.inner.on_air.load(Ordering::Relaxed)
    }

    pub fn calibrating(&self) -> bool {
        self.inner.calibrating.load(Ordering::Relaxed)
    }

    /// Measure now, on a thread of its own. False when one is running
    /// already or there is nowhere to keep the result.
    pub fn calibrate_now(&self) -> bool {
        self.spawn(false)
    }

    /// Measure once nothing is on air.
    fn calibrate_when_clear(&self) -> bool {
        self.spawn(true)
    }
}

mod measure;
