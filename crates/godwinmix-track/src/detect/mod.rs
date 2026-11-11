//! Layer one: find people in a frame. A [`Detector`] turns a [`FrameRef`] into
//! boxes with a confidence, and keypoints when the backend has them.

pub mod motion;
#[cfg(feature = "tract")]
pub mod tract;

use crate::frame::FrameRef;
use crate::geom::{Point, Rect};

/// What a detection is of. The motion backend cannot tell a person from
/// anything else that moves, so it says `Motion`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Class {
    #[default]
    Person,
    Motion,
}

/// The 17 COCO body points, in the order MoveNet and most pose models give
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Joint {
    Nose,
    LeftEye,
    RightEye,
    LeftEar,
    RightEar,
    LeftShoulder,
    RightShoulder,
    LeftElbow,
    RightElbow,
    LeftWrist,
    RightWrist,
    LeftHip,
    RightHip,
    LeftKnee,
    RightKnee,
    LeftAnkle,
    RightAnkle,
}

/// One body point: where it is and how sure the model is, 0 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Keypoint {
    pub at: Point,
    pub score: f32,
}

/// All 17 points of one person, inline so a detection never allocates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Keypoints(pub [Keypoint; 17]);

impl Keypoints {
    /// The point for `joint` when the model scored it at least `min_score`.
    pub fn get(&self, joint: Joint, min_score: f32) -> Option<Point> {
        let k = self.0[joint as usize];
        (k.score >= min_score).then_some(k.at)
    }

    /// The midpoint of a left and right pair, or whichever of the two is seen.
    pub fn mid(&self, left: Joint, right: Joint, min_score: f32) -> Option<Point> {
        match (self.get(left, min_score), self.get(right, min_score)) {
            (Some(a), Some(b)) => Some(Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)),
            (a, b) => a.or(b),
        }
    }
}

/// One thing found in a frame, in normalised coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Detection {
    pub rect: Rect,
    /// Confidence, 0 to 1.
    pub score: f32,
    pub class: Class,
    pub keypoints: Option<Keypoints>,
}

impl Detection {
    pub fn new(rect: Rect, score: f32) -> Self {
        Self { rect, score, class: Class::Person, keypoints: None }
    }
}

/// A detector backend.
///
/// `detect` is the simple call. `detect_into` reuses the caller's vector, so a
/// loop that keeps one vector allocates nothing per frame; the backends in this
/// crate implement it directly. A frame that fails [`FrameRef::validate`]
/// gives no detections.
pub trait Detector {
    fn detect(&mut self, frame: &FrameRef) -> Vec<Detection>;

    /// Clears `out` and fills it with the detections in `frame`.
    fn detect_into(&mut self, frame: &FrameRef, out: &mut Vec<Detection>) {
        out.clear();
        out.extend(self.detect(frame));
    }

    /// Forgets anything learned from earlier frames, for a cut or a camera
    /// move. Backends that keep nothing need not implement it.
    fn reset(&mut self) {}
}

impl<D: Detector + ?Sized> Detector for Box<D> {
    fn detect(&mut self, frame: &FrameRef) -> Vec<Detection> {
        (**self).detect(frame)
    }

    fn detect_into(&mut self, frame: &FrameRef, out: &mut Vec<Detection>) {
        (**self).detect_into(frame, out)
    }

    fn reset(&mut self) {
        (**self).reset()
    }
}

/// Keeps the best of overlapping detections: sorts by score and drops any box
/// whose IoU with a kept one is above `iou`. Works in place.
pub fn non_max_suppression(dets: &mut Vec<Detection>, iou: f32) {
    dets.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept = 0;
    for i in 0..dets.len() {
        let d = dets[i];
        if dets[..kept].iter().all(|k| k.rect.iou(&d.rect) <= iou) {
            dets[kept] = d;
            kept += 1;
        }
    }
    dets.truncate(kept);
}
