//! A stand in for the vitals (`vitals/`, the "vitals" work) with the
//! calls the host makes, so the host builds before that work merges. It
//! judges nothing and raises no `direct.health`. Delete it when `vitals/`
//! lands.

use std::sync::Arc;

use serde_json::Value;

use crate::hub::Hub;

/// Where `event/direct.health` goes.
pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

pub struct Vitals;

impl Vitals {
    pub fn start(_hub: Hub, _emit: Emit, _workers: usize) -> Arc<Vitals> {
        Arc::new(Vitals)
    }

    pub fn watch(&self, _id: &str, _app: &str, _stream: &str, _monitor: &Value) {}

    pub fn keep(&self, _ids: &[&str]) {}

    pub fn counters(&self, _id: &str, _cc_errors: u64, _lost: u64) {}

    pub fn output(&self, _id: &str, _output: &str, _failed: Option<&str>) {}

    pub fn offer_frame(&self, _id: &str, _sample: &gstreamer::Sample) {}

    /// `direct.thumbnail`; `None` for any other call.
    pub fn call(&self, name: &str, _params: &Value) -> Option<Value> {
        (name == "direct.thumbnail").then(|| serde_json::json!({"pending": true}))
    }
}
