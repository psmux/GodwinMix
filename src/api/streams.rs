//! Nothing runs unless asked.
//!
//! One counter per expensive stream, with a hook on the edge from zero to one
//! and back. The multiview pipeline, the meter taps and anything else that
//! costs CPU hangs off one of these, so a headless core with no UI attached
//! pays nothing for the mosaic it is not drawing.
//!
//! # For whoever owns multiview.rs
//!
//! `AppState::multiview_gate` is a `StreamGate`. Call `on_first` and `on_last`
//! once at startup with closures that build and tear down the pipeline; the
//! `/rpc` connections take a `GateGuard` each while they are subscribed to
//! `ext.multiview`, and the guard's `Drop` fires `on_last` when the final one
//! goes. Nothing in this file touches GStreamer, and nothing in multiview.rs
//! needs to know a WebSocket exists.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

type Hook = Box<dyn Fn() + Send + Sync>;

/// A subscriber count with edges.
#[derive(Default)]
pub struct StreamGate {
    count: AtomicUsize,
    on_first: parking_lot::Mutex<Option<Hook>>,
    on_last: parking_lot::Mutex<Option<Hook>>,
    /// Woken on the edge from zero to one, so a task that only works while
    /// somebody is watching can park rather than poll.
    wake: tokio::sync::Notify,
}

impl std::fmt::Debug for StreamGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamGate").field("count", &self.count()).finish()
    }
}

impl StreamGate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Run this the first time somebody subscribes, and every time the count
    /// goes from zero to one after that.
    pub fn on_first(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.on_first.lock() = Some(Box::new(hook));
    }

    /// Run this when the last subscriber goes.
    pub fn on_last(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.on_last.lock() = Some(Box::new(hook));
    }

    pub fn count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    pub fn anyone_watching(&self) -> bool {
        self.count() > 0
    }

    /// Take a place in the count. The stream runs until the guard is dropped.
    pub fn subscribe(self: &Arc<Self>) -> GateGuard {
        if self.count.fetch_add(1, Ordering::SeqCst) == 0 {
            if let Some(hook) = self.on_first.lock().as_ref() {
                hook();
            }
            self.wake.notify_waiters();
        }
        GateGuard { gate: self.clone() }
    }

    /// Park until somebody is watching. A task that wakes on this and checks
    /// `anyone_watching` again cannot miss an edge, because the notify is
    /// permit based and a notification sent before the wait is remembered.
    pub async fn wait_for_a_subscriber(&self) {
        while !self.anyone_watching() {
            self.wake.notified().await;
        }
    }

    fn release(&self) {
        if self.count.fetch_sub(1, Ordering::SeqCst) == 1 {
            if let Some(hook) = self.on_last.lock().as_ref() {
                hook();
            }
        }
    }
}

/// One subscriber's place in the count. Dropping it gives the place back, so a
/// client that vanishes mid frame is not left holding an encoder open.
pub struct GateGuard {
    gate: Arc<StreamGate>,
}

impl Drop for GateGuard {
    fn drop(&mut self) {
        self.gate.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn the_hooks_fire_on_the_edges_and_not_in_between() {
        let starts = Arc::new(AtomicU32::new(0));
        let stops = Arc::new(AtomicU32::new(0));
        let gate = StreamGate::new();
        {
            let s = starts.clone();
            gate.on_first(move || {
                s.fetch_add(1, Ordering::SeqCst);
            });
            let s = stops.clone();
            gate.on_last(move || {
                s.fetch_add(1, Ordering::SeqCst);
            });
        }
        assert_eq!(gate.count(), 0);
        assert!(!gate.anyone_watching());

        let a = gate.subscribe();
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        let b = gate.subscribe();
        // A second viewer is not a second pipeline.
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(gate.count(), 2);

        drop(a);
        assert_eq!(stops.load(Ordering::SeqCst), 0, "one viewer left, nothing torn down");
        drop(b);
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert_eq!(gate.count(), 0);

        // And it comes back for the next viewer.
        let c = gate.subscribe();
        assert_eq!(starts.load(Ordering::SeqCst), 2);
        drop(c);
        assert_eq!(stops.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_task_can_park_until_somebody_watches() {
        let gate = StreamGate::new();
        let waiter = {
            let gate = gate.clone();
            tokio::spawn(async move {
                gate.wait_for_a_subscriber().await;
                gate.count()
            })
        };
        // Give the waiter a moment to park, then let it through.
        tokio::task::yield_now().await;
        let guard = gate.subscribe();
        assert_eq!(waiter.await.unwrap(), 1);
        drop(guard);

        // Already watching means no wait at all.
        let guard = gate.subscribe();
        gate.wait_for_a_subscriber().await;
        drop(guard);
    }
}
