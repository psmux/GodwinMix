//! The ONNX person detector, run by tract, which is pure Rust: no native
//! runtime to ship and nothing to link. The model is YOLOX nano (Apache 2.0),
//! fetched on first use and checked by SHA256, and MoveNet Lightning adds
//! keypoints when wanted. Behind the `tract` feature.

mod fetch;
mod pose;
mod yolox;

pub use fetch::{ensure_model, models_dir, ModelError, ModelSpec, YOLOX_NANO};
pub use pose::{PoseDetector, MOVENET_LIGHTNING};

use std::path::PathBuf;

use tract_onnx::prelude::*;

use super::{non_max_suppression, Detection, Detector};
use crate::frame::FrameRef;

#[derive(Clone, Debug, PartialEq)]
pub struct TractConfig {
    pub model: ModelSpec,
    /// Where the model is kept. `None` uses [`models_dir`].
    pub dir: Option<PathBuf>,
    /// The size the network runs at, width and height, multiples of 32.
    /// Smaller is faster and misses small people: 416 is what YOLOX nano
    /// was trained at, 320 is about twice as fast.
    pub input: (usize, usize),
    /// The least person score kept, 0 to 1.
    pub score_threshold: f32,
    /// Overlapping boxes above this IoU are folded into the best one.
    pub nms_iou: f32,
}

impl Default for TractConfig {
    fn default() -> Self {
        Self { model: YOLOX_NANO, dir: None, input: (416, 416), score_threshold: 0.3, nms_iou: 0.45 }
    }
}

/// One spawned state, kept so its buffers are reused from frame to frame.
type Plan = tract_onnx::tract_core::plan::SimpleState<TypedFact, Box<dyn TypedOp>>;

/// Loads, sizes, optimises and spawns a model.
fn load_plan(bytes: &[u8], input: InferenceFact, path: PathBuf) -> Result<Plan, ModelError> {
    let fail = |e: TractError| ModelError::Load { path: path.clone(), reason: format!("{e:#}") };
    tract_onnx::onnx()
        .model_for_read(&mut std::io::Cursor::new(bytes))
        .map(forget_shapes)
        .and_then(|m| m.with_input_fact(0, input))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .and_then(|p| p.spawn())
        .map_err(fail)
}

/// A person detector backed by an ONNX model.
pub struct TractDetector {
    pub config: TractConfig,
    plan: Plan,
    input: Vec<f32>,
}

impl std::fmt::Debug for TractDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TractDetector({} at {:?})", self.config.model.file, self.config.input)
    }
}

impl TractDetector {
    /// Loads the model, fetching it first if it is not on disk yet. This
    /// blocks on the network the first time; call it off any media thread.
    pub fn new(config: TractConfig) -> Result<Self, ModelError> {
        let dir = config.dir.clone().unwrap_or_else(models_dir);
        let bytes = ensure_model(&config.model, &dir)?;
        let path = dir.join(config.model.file);
        Self::from_bytes(&bytes, config, path)
    }

    fn from_bytes(bytes: &[u8], config: TractConfig, path: PathBuf) -> Result<Self, ModelError> {
        let (iw, ih) = config.input;
        let plan = load_plan(bytes, f32::fact([1, 3, ih, iw]).into(), path)?;
        Ok(Self { config, plan, input: vec![0.0; 3 * iw * ih] })
    }

    fn run(&mut self, frame: &FrameRef, out: &mut Vec<Detection>) -> TractResult<()> {
        let (iw, ih) = self.config.input;
        yolox::preprocess(frame, iw, ih, &mut self.input);
        let tensor = Tensor::from_shape(&[1, 3, ih, iw], &self.input)?;
        let result = self.plan.run(tvec!(tensor.into()))?;
        let view = result[0].to_plain_array_view::<f32>()?;
        let Some(raw) = view.as_slice() else { return Err(TractError::msg("the model output is not contiguous")) };
        if raw.len() < yolox::anchors(iw, ih) * 85 {
            return Err(TractError::msg("the model gives fewer rows than YOLOX does at this input size"));
        }
        yolox::decode(raw, frame, iw, ih, self.config.score_threshold, out);
        non_max_suppression(out, self.config.nms_iou);
        Ok(())
    }
}

/// The export recorded every intermediate shape at the size it was made
/// for. Forgetting them lets tract work them out again for another size.
fn forget_shapes(mut m: InferenceModel) -> InferenceModel {
    for node in m.nodes_mut() {
        for out in &mut node.outputs {
            out.fact = InferenceFact::default();
        }
    }
    m
}

impl Detector for TractDetector {
    fn detect(&mut self, frame: &FrameRef) -> Vec<Detection> {
        let mut out = Vec::new();
        self.detect_into(frame, &mut out);
        out
    }

    /// A frame tract fails on gives no detections, never a panic: the chain
    /// goes on with the tracker coasting.
    fn detect_into(&mut self, frame: &FrameRef, out: &mut Vec<Detection>) {
        out.clear();
        if frame.validate().is_ok() && self.run(frame, out).is_err() {
            out.clear();
        }
    }
}
