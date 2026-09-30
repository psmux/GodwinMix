//! The process wide map from output id to its stream, which is how a request
//! with only an output id finds what the mixer thread built.

use super::stream::Stream;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

fn registry() -> &'static RwLock<BTreeMap<String, Arc<Stream>>> {
    static STREAMS: OnceLock<RwLock<BTreeMap<String, Arc<Stream>>>> = OnceLock::new();
    STREAMS.get_or_init(Default::default)
}

/// Publish an output under its id, replacing one of the same id.
pub fn publish(stream: Arc<Stream>) {
    registry().write().insert(stream.id.clone(), stream);
}

/// Take an output down, if the one published under its id is still this one.
pub fn withdraw(stream: &Arc<Stream>) {
    let mut map = registry().write();
    if map.get(&stream.id).is_some_and(|s| Arc::ptr_eq(s, stream)) {
        map.remove(&stream.id);
    }
}

pub fn get(id: &str) -> Option<Arc<Stream>> {
    registry().read().get(id).cloned()
}

pub fn ids() -> Vec<String> {
    registry().read().keys().cloned().collect()
}

/// Everything every HLS output is sending, in kbit/s: the number the
/// governor adds to its uplink budget.
pub fn egress_kbps() -> u32 {
    registry().read().values().map(|s| s.viewers.egress_kbps()).sum()
}
