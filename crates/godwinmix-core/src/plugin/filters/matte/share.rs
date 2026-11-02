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
pub fn worker_for(key: Option<&str>, settings: &Settings) -> Arc<Worker> {
    let Some(key) = key else { return Arc::new(Worker::start(settings.clone())) };
    let mut pool = POOL.lock();
    pool.retain(|(_, w)| w.strong_count() > 0);
    let found = pool
        .iter()
        .filter(|(k, _)| k == key)
        .filter_map(|(_, w)| w.upgrade())
        .find(|w| w.answers_for(settings));
    if let Some(worker) = found {
        tracing::debug!(picture = key, "a cutout shares the model already running on its picture");
        return worker;
    }
    let worker = Arc::new(Worker::start(settings.clone()));
    pool.push((key.to_string(), Arc::downgrade(&worker)));
    worker
}

impl Worker {
    /// Offer a frame, as the lead or as a follower. A follower's frame is
    /// dropped while the lead is feeding.
    pub fn offer_as(&self, frame: &gstreamer::Buffer, info: &gstreamer_video::VideoInfo, lead: bool) {
        {
            let mut lead_at = self.shared.lead_at.lock();
            if lead {
                *lead_at = Some(Instant::now());
            } else if lead_at.is_some_and(|at| at.elapsed() < FOLLOW_GAP) {
                return;
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
mod tests {
    use super::*;

    /// No model is needed: a worker that cannot open one still takes offers
    /// and still decides whether to.
    #[test]
    fn the_same_picture_and_masks_share_one_worker_and_others_do_not() {
        let s = Settings::default();
        let a = worker_for(Some("share-test-cam"), &s);
        let b = worker_for(Some("share-test-cam"), &s);
        assert!(Arc::ptr_eq(&a, &b), "two cutouts on one picture ran two models");
        let other = worker_for(Some("share-test-other"), &s);
        assert!(!Arc::ptr_eq(&a, &other), "two pictures shared one model");
        let steadier = Settings { steady: 0.9, ..s.clone() };
        let c = worker_for(Some("share-test-cam"), &steadier);
        assert!(!Arc::ptr_eq(&a, &c), "a cutout asking other masks was given these");
        let edge = Settings { cutoff: 0.3, feather: 4, ..s.clone() };
        assert!(Arc::ptr_eq(&a, &worker_for(Some("share-test-cam"), &edge)), "the edge is not the model's");
        let alone = worker_for(None, &s);
        assert!(!Arc::ptr_eq(&a, &alone));
    }

    #[test]
    fn a_follower_waits_while_the_lead_feeds() {
        gstreamer::init().unwrap();
        let w = worker_for(Some("share-test-follow"), &Settings { quality: crate::plugin::filters::matte::params::Quality::Named("nothing-here".into()), ..Settings::default() });
        let info = gstreamer_video::VideoInfo::builder(gstreamer_video::VideoFormat::I420, 16, 16).build().unwrap();
        let frame = gstreamer::Buffer::with_size(info.size()).unwrap();
        w.offer_as(&frame, &info, false);
        assert!(w.shared.lead_at.lock().is_none(), "a follower is not the lead");
        w.offer_as(&frame, &info, true);
        w.shared.job.lock().take();
        w.offer_as(&frame, &info, false);
        assert!(w.shared.job.lock().is_none(), "a follower's frame went in while the lead was feeding");
    }
}
