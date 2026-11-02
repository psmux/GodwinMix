//! One model for one picture, however many places it is cut out.
//!
//! A scene with a cutout on the presenter is drawn twice while it is both on
//! air and armed: once on the programme and once in the preview. Two cutouts
//! with two workers would run the model twice on the same camera, which on a
//! machine with no GPU is the difference between a cutout that keeps up and
//! one that does not. So a cutout that says whose picture it works on
//! (`Filter::share_as`) takes the worker another cutout on the same picture
//! already has, when that worker would answer the same masks.
//!
//! The programme's copy leads. While it offers frames the preview's copy only
//! reads the newest mask, which fits its smaller frame because a mask is at
//! the model's size, not the picture's. A scene that is armed and not on air
//! has no lead, and the preview's frames feed the model on their own.

use super::{Settings, Worker};
use parking_lot::Mutex;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

/// How long the lead may go without offering a frame before the others'
/// frames are taken instead.
const FOLLOW_GAP: Duration = Duration::from_millis(250);

/// Every worker a cutout said the picture of, by that picture.
static POOL: Mutex<Vec<(String, Weak<Worker>)>> = Mutex::new(Vec::new());

/// A worker for a cutout on `key`'s picture with `settings`: one already
/// running for the same picture and the same masks, or a new one. A cutout
/// that named no picture always gets its own.
///
/// The key is `filter::share_key`: the source with the crop and the turn in
/// front of the filter, because those change the pixels the model sees. The
/// same camera wide and cropped are two pictures and two models.
pub fn worker_for(key: Option<&str>, settings: &Settings) -> Arc<Worker> {
    let Some(key) = key else { return Arc::new(Worker::start(settings.clone())) };
    let mut pool = POOL.lock();
    if let Some(worker) = find(&mut pool, key, settings, None) {
        tracing::debug!(picture = key, "a cutout shares the model already running on its picture");
        return worker;
    }
    let worker = Arc::new(Worker::start(settings.clone()));
    pool.push((key.to_string(), Arc::downgrade(&worker)));
    worker
}

fn find(pool: &mut Vec<(String, Weak<Worker>)>, key: &str, settings: &Settings, not: Option<&Arc<Worker>>) -> Option<Arc<Worker>> {
    pool.retain(|(_, w)| w.strong_count() > 0);
    pool.iter()
        .filter(|(k, _)| k == key)
        .filter_map(|(_, w)| w.upgrade())
        .filter(|w| not.is_none_or(|n| !Arc::ptr_eq(n, w)))
        .find(|w| w.answers_for(settings))
}

/// Where a cutout on `current` goes when its settings change: `None` to
/// change `current` in place, or the worker to move to.
///
/// Another worker on the picture that already answers the new settings is
/// taken first. That is the second of two cutouts on one item told of the
/// same change: the first moved to a new worker, and this one follows it
/// rather than changing the old one, so the picture ends on one model.
/// A worker another cutout still shares, and nothing answering, is a new
/// worker. A worker this cutout has alone is changed in place, and its last
/// mask stays up while a new model loads.
///
/// `current` is the caller's own clone, so a worker it has alone is counted
/// twice: once in the cutout and once here.
pub fn settle(key: Option<&str>, current: &Arc<Worker>, settings: &Settings) -> Option<Arc<Worker>> {
    if current.answers_for(settings) {
        return None;
    }
    let key = key?;
    if let Some(other) = find(&mut POOL.lock(), key, settings, Some(current)) {
        return Some(other);
    }
    (Arc::strong_count(current) > 2).then(|| worker_for(Some(key), settings))
}

/// How many cutout models are running on `key`'s picture. One while a
/// camera is cut out on air and in Preview at once is the point of this
/// module; the test that says so reads it.
pub fn workers_on(key: &str) -> usize {
    POOL.lock().iter().filter(|(k, w)| k == key && w.strong_count() > 0).count()
}

impl Worker {
    /// Offer a frame, as the lead or as a follower. A follower's frame is
    /// dropped while the lead is feeding.
    ///
    /// Two leads on one worker is one camera on air twice with the same crop,
    /// in one scene or on both sides of a transition. Their frames are the
    /// same picture, so one of them, `me`, feeds and the other only reads,
    /// until the first stops offering.
    pub fn offer_as(&self, frame: &gstreamer::Buffer, info: &gstreamer_video::VideoInfo, lead: bool, me: u64) {
        {
            let mut lead_at = self.shared.lead_at.lock();
            let feeding = lead_at.filter(|(_, at)| at.elapsed() < FOLLOW_GAP).map(|(who, _)| who);
            match (lead, feeding) {
                (true, Some(who)) if who != me => return,
                (true, _) => *lead_at = Some((me, Instant::now())),
                (false, Some(_)) => return,
                (false, None) => {}
            }
        }
        self.offer(frame, info);
    }

    /// True when a cutout with `settings` would ask this worker for the same
    /// masks. The model, the device and the steadying are the worker's; the
    /// edge, the feather and the matte are laid on afterwards on each
    /// cutout's own thread, so two cutouts may differ there and still share.
    pub fn answers_for(&self, settings: &Settings) -> bool {
        let mine = self.shared.settings.lock();
        mine.quality == settings.quality && mine.device == settings.device && mine.steady == settings.steady
    }
}

#[cfg(test)]
#[path = "share_tests.rs"]
mod tests;
