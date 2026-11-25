//! Where a replay source finds its mixer's rings.
//!
//! One entry per mixer in this process, keyed by the instant the mixer was
//! built, which every source it builds is handed (`BuildCtx::origin`). A show
//! is a process of its own, so in practice there is one; the key keeps two
//! mixers in one test binary apart. Entries are weak: a mixer that has gone
//! leaves nothing alive behind it.

use super::Shared;
use parking_lot::Mutex;
use std::sync::{Arc, Weak};
use std::time::Instant;

static DESKS: Mutex<Vec<(Instant, Weak<Shared>)>> = Mutex::new(Vec::new());

pub fn register(origin: Instant, shared: &Arc<Shared>) {
    let mut desks = DESKS.lock();
    desks.retain(|(at, w)| *at != origin && w.strong_count() > 0);
    desks.push((origin, Arc::downgrade(shared)));
}

pub fn find(origin: Instant) -> Option<Arc<Shared>> {
    DESKS.lock().iter().find(|(at, _)| *at == origin).and_then(|(_, w)| w.upgrade())
}
