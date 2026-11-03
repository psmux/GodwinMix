//! A model session kept open between cutouts, and how many cutouts are still
//! getting ready.
//!
//! Opening a session is the slow part of a cutout: three to eleven seconds
//! for the fine model on DirectML, on a tester's laptop. Preview builds its
//! own copy of a scene's filters every time a scene is armed, so each arm
//! opened the model again and the cutout took that long to appear each time.
//! A worker that stops now leaves its session here, one per model and device,
//! and the next worker for the same settings takes it instead of opening
//! another. The cost is one idle session's memory after the last cutout goes.
//!
//! The counts are what `filter.cutout` reports, so a page can say "preparing
//! the cutout" rather than show the camera whole and leave a person guessing.

use super::inference::Open;
use crate::plugin::filters::matte::params::{Device, Quality};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

static POOL: Mutex<Vec<Open>> = Mutex::new(Vec::new());
static PREPARING: AtomicUsize = AtomicUsize::new(0);
static RUNNING: AtomicUsize = AtomicUsize::new(0);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// The session a stopped worker left for these settings, if there is one.
pub(super) fn take(wanted: &(Quality, Device)) -> Option<Open> {
    let mut pool = POOL.lock();
    let at = pool.iter().position(|o| o.for_settings == *wanted)?;
    Some(pool.swap_remove(at))
}

/// Keep a session for the next worker. One per settings: an older one for
/// the same settings is let go.
pub(super) fn put_back(open: Open) {
    let mut pool = POOL.lock();
    pool.retain(|o| o.for_settings != open.for_settings);
    pool.push(open);
}

/// What the cutouts are doing now.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Readiness {
    /// Cutouts whose model is still opening or has not answered a frame yet.
    /// Each draws nothing of the camera until it has.
    pub preparing: usize,
    /// Cutouts drawing a mask.
    pub running: usize,
    /// Why the last one that failed could not start, until one starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn readiness() -> Readiness {
    Readiness {
        preparing: PREPARING.load(Ordering::Relaxed),
        running: RUNNING.load(Ordering::Relaxed),
        error: LAST_ERROR.lock().clone(),
    }
}

/// One worker's place in the counts, from its start until it stops.
pub(super) struct Counted {
    running: bool,
    counted: bool,
}

impl Counted {
    pub(super) fn start() -> Counted {
        PREPARING.fetch_add(1, Ordering::Relaxed);
        Counted { running: false, counted: true }
    }

    /// The first mask is out.
    pub(super) fn running(&mut self) {
        if self.counted && !self.running {
            PREPARING.fetch_sub(1, Ordering::Relaxed);
            RUNNING.fetch_add(1, Ordering::Relaxed);
            self.running = true;
            *LAST_ERROR.lock() = None;
        }
    }

    /// The settings changed: preparing again until the new model answers.
    pub(super) fn again(&mut self) {
        if self.counted && self.running {
            RUNNING.fetch_sub(1, Ordering::Relaxed);
            PREPARING.fetch_add(1, Ordering::Relaxed);
            self.running = false;
        }
    }

    /// The model would not open. Not preparing any more, and not running.
    pub(super) fn failed(&mut self, why: &str) {
        self.leave();
        *LAST_ERROR.lock() = Some(why.to_string());
    }

    /// Counted again once the settings change and it tries again.
    pub(super) fn retry(&mut self) {
        if !self.counted {
            PREPARING.fetch_add(1, Ordering::Relaxed);
            self.counted = true;
        }
    }

    fn leave(&mut self) {
        if !self.counted {
            return;
        }
        let from = if self.running { &RUNNING } else { &PREPARING };
        from.fetch_sub(1, Ordering::Relaxed);
        self.counted = false;
        self.running = false;
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.leave();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counts are process wide, so one test walks the whole life.
    #[test]
    fn a_cutout_is_preparing_until_its_first_mask_and_counted_out_when_it_stops() {
        let before = readiness();
        let mut one = Counted::start();
        assert_eq!(readiness().preparing, before.preparing + 1);
        one.running();
        assert_eq!(readiness().preparing, before.preparing);
        assert_eq!(readiness().running, before.running + 1);
        one.again();
        assert_eq!(readiness().preparing, before.preparing + 1, "a new model is preparing again");
        one.failed("no model");
        assert_eq!(readiness().preparing, before.preparing);
        assert_eq!(readiness().running, before.running);
        assert_eq!(readiness().error.as_deref(), Some("no model"));
        one.retry();
        assert_eq!(readiness().preparing, before.preparing + 1);
        drop(one);
        assert_eq!(readiness().preparing, before.preparing);
        assert_eq!(readiness().running, before.running);
    }
}
