//! The follow loop's person detector: YOLOX nano by default, with the motion
//! detector standing in while the model downloads or loads and for good if
//! it cannot be had. The loader runs on a thread of its own, so the loop
//! follows people by motion from its first picture and moves over to YOLOX
//! the moment it is ready.
//!
//! What runs and why is kept in a [`Status`] the loop copies into
//! `track_state`'s `detector`.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use godwinmix_track::{Detection, Detector, FrameRef, MotionConfig, MotionDetector};

pub type Boxed = Box<dyn Detector + Send>;

/// Which detector a person asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    Motion,
    #[default]
    Yolox,
    YoloxPose,
}

impl Kind {
    pub fn slug(self) -> &'static str {
        match self {
            Kind::Motion => "motion",
            Kind::Yolox => "yolox",
            Kind::YoloxPose => "yolox+pose",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        [Kind::Motion, Kind::Yolox, Kind::YoloxPose].into_iter().find(|k| k.slug() == s)
    }
}

/// What runs now, what was asked for, and why they differ when they do.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Status {
    pub wanted: Kind,
    pub running: Kind,
    /// `loading`, `ready` or `failed`, for a model; `ready` for motion.
    pub stage: &'static str,
    pub why: String,
}

/// A detector that starts as motion and swaps itself for the model once
/// the loader hands one over.
pub struct Switching {
    current: Boxed,
    loading: Option<Receiver<Result<Boxed, String>>>,
    pub status: Status,
}

pub fn motion() -> Boxed {
    Box::new(MotionDetector::new(MotionConfig::default()))
}

impl Switching {
    /// Motion now; the model for `wanted` loading on a thread from `dir`.
    pub fn new(wanted: Kind, dir: Option<PathBuf>) -> Switching {
        let mut s = Switching { current: motion(), loading: None, status: Status { wanted, running: Kind::Motion, stage: "ready", why: String::new() } };
        if wanted == Kind::Motion {
            s.status.why = "motion was asked for in the settings: no model, and it loses a person who stands still".into();
            return s;
        }
        match super::model::start(wanted, dir) {
            Ok((rx, why)) => {
                s.loading = Some(rx);
                (s.status.stage, s.status.why) = ("loading", why);
            }
            Err(why) => (s.status.stage, s.status.why) = ("failed", why),
        }
        s
    }

    /// A detector that is already loaded, for tests and benches.
    pub fn ready(kind: Kind, detector: Boxed) -> Switching {
        let why = format!("{} as asked for", kind.slug());
        Switching { current: detector, loading: None, status: Status { wanted: kind, running: kind, stage: "ready", why } }
    }

    /// Take the model if the loader has finished.
    fn poll(&mut self) {
        let Some(rx) = &self.loading else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.loading = None;
        match result {
            Ok(model) => {
                self.current = model;
                self.status.running = self.status.wanted;
                (self.status.stage, self.status.why) = ("ready", format!("{} as asked for", self.status.wanted.slug()));
            }
            Err(why) => {
                self.status.stage = "failed";
                self.status.why = format!("motion stands in because {} could not be loaded: {why}", self.status.wanted.slug());
            }
        }
    }

    pub fn running(&self) -> Kind {
        self.status.running
    }
}

impl Detector for Switching {
    fn detect(&mut self, frame: &FrameRef) -> Vec<Detection> {
        self.poll();
        self.current.detect(frame)
    }

    fn detect_into(&mut self, frame: &FrameRef, out: &mut Vec<Detection>) {
        self.poll();
        self.current.detect_into(frame, out)
    }

    fn reset(&mut self) {
        self.current.reset()
    }
}
