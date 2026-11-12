//! Keypoints from MoveNet Lightning (Apache 2.0), run on a square crop
//! around each of the best person boxes another detector found. MoveNet takes
//! 192 by 192 RGB as 32 bit integers and gives 17 rows of y, x and score,
//! normalised to the crop.

use std::path::PathBuf;

use tract_onnx::prelude::*;

use super::{ensure_model, load_plan, models_dir, ModelError, ModelSpec, Plan};
use crate::detect::{Detection, Detector, Keypoint, Keypoints};
use crate::frame::FrameRef;
use crate::geom::{Point, Rect};

const SIDE: usize = 192;

/// MoveNet single pose Lightning, the Xenova ONNX conversion, pinned to one
/// revision. Apache 2.0.
pub const MOVENET_LIGHTNING: ModelSpec = ModelSpec {
    file: "movenet_lightning.onnx",
    url: "https://huggingface.co/Xenova/movenet-singlepose-lightning/resolve/ed0f314bb7356fd1dbf1e4f52c2d40791bf6534f/onnx/model.onnx",
    sha256: "1ad4f8d6c2f776a9967db3993c9ca740bc350104f9d37c151dc183fc29a464ad",
    licence: "Apache-2.0",
};

/// Wraps a box detector and adds keypoints to its best `max_people` people.
pub struct PoseDetector<D> {
    pub inner: D,
    /// How many people get keypoints, best scores first. Each costs a run.
    pub max_people: usize,
    /// Run MoveNet on one call in this many. The tracker carries each
    /// person's last keypoints along with their box in between.
    pub every: u32,
    calls: u64,
    plan: Plan,
    input: Vec<i32>,
}

impl<D> PoseDetector<D> {
    /// Loads MoveNet from `dir` (or [`models_dir`]), fetching it first if
    /// need be. Blocks on the network the first time.
    pub fn new(inner: D, max_people: usize, dir: Option<PathBuf>) -> Result<Self, ModelError> {
        let dir = dir.unwrap_or_else(models_dir);
        let bytes = ensure_model(&MOVENET_LIGHTNING, &dir)?;
        let fact = i32::fact([1, SIDE, SIDE, 3]).into();
        let plan = load_plan(&bytes, fact, dir.join(MOVENET_LIGHTNING.file))?;
        Ok(Self { inner, max_people, every: 1, calls: 0, plan, input: vec![0; SIDE * SIDE * 3] })
    }

    /// A square around the box, a fifth bigger, in pixels: left, top, side.
    fn crop(frame: &FrameRef, r: &Rect) -> (f32, f32, f32) {
        let (fw, fh) = (frame.width as f32, frame.height as f32);
        let side = (r.w * fw).max(r.h * fh) * 1.2;
        (r.cx() * fw - side / 2.0, r.cy() * fh - side / 2.0, side)
    }

    fn keypoints(&mut self, frame: &FrameRef, r: &Rect) -> TractResult<Keypoints> {
        let (left, top, side) = Self::crop(frame, r);
        let step = side / SIDE as f32;
        for y in 0..SIDE {
            let sy = top + (y as f32 + 0.5) * step;
            for x in 0..SIDE {
                let sx = left + (x as f32 + 0.5) * step;
                let i = (y * SIDE + x) * 3;
                let inside = sx >= 0.0 && sy >= 0.0;
                let px = if inside { frame.rgb_at(sx as u32, sy as u32) } else { [0; 3] };
                self.input[i..i + 3].copy_from_slice(&px.map(i32::from));
            }
        }
        let tensor = Tensor::from_shape(&[1, SIDE, SIDE, 3], &self.input)?;
        let out = self.plan.run(tvec!(tensor.into()))?;
        let view = out[0].to_plain_array_view::<f32>()?;
        let raw = view.as_slice().unwrap_or_default();
        if raw.len() < 51 {
            return Err(TractError::msg("MoveNet gave fewer than 17 keypoints"));
        }
        let (fw, fh) = (frame.width as f32, frame.height as f32);
        let mut k = Keypoints::default();
        for (j, kp) in k.0.iter_mut().enumerate() {
            let (y, x, score) = (raw[j * 3], raw[j * 3 + 1], raw[j * 3 + 2]);
            let at = Point::new((left + x * side) / fw, (top + y * side) / fh);
            *kp = Keypoint { at, score };
        }
        Ok(k)
    }
}

impl<D: Detector> Detector for PoseDetector<D> {
    fn detect(&mut self, frame: &FrameRef) -> Vec<Detection> {
        let mut out = Vec::new();
        self.detect_into(frame, &mut out);
        out
    }

    /// The inner detector's boxes, best first, the top ones with keypoints.
    fn detect_into(&mut self, frame: &FrameRef, out: &mut Vec<Detection>) {
        self.inner.detect_into(frame, out);
        out.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
        self.calls += 1;
        if (self.calls - 1) % self.every.max(1) as u64 != 0 {
            return;
        }
        for d in out.iter_mut().take(self.max_people) {
            d.keypoints = self.keypoints(frame, &d.rect).ok();
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn camera_motion(&mut self, ego: crate::ego::EgoMotion) {
        self.inner.camera_motion(ego)
    }
}
