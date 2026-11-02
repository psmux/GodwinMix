//! The half of starting an encode chain that waits.
//!
//! Joining a chain to the raw tee waits for nothing. Bringing the chain up
//! first is different: a hardware encoder opens its session on the way up,
//! and on Windows Quick Sync and Media Foundation take a second or more over
//! it. That ran on the mixer thread, inside whichever command took the first
//! lease, so the first output a show had held the command loop while the
//! encoder opened: on 2026-11-02 a guest's return did, for more than five
//! seconds, and every call behind it answered that the mixer was busy.
//!
//! So the chains are brought up here, on a thread from `mixer::offload`, and
//! the thread asks the mixer again when it is done. The mixer then joins them
//! to the tee on its own thread, which takes no time, or takes them down
//! again if nobody wants them any more. The order is still up first, linked
//! second, for the reason `EncodeChain::attach` gives.

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A chain not yet joined to its tee, to be brought up.
pub struct Raise {
    pub tag: &'static str,
    /// Head first, as the chain keeps them.
    pub chain: Vec<gst::Element>,
}

impl Raise {
    /// Unlocked, then up to the pipeline's state, tail first, so everything
    /// downstream of the encoder is ready before the encoder is.
    pub fn run(&self) -> Result<()> {
        for el in &self.chain {
            el.set_locked_state(false);
        }
        for el in self.chain.iter().rev() {
            el.sync_state_with_parent()
                .with_context(|| format!("{} encode chain would not start", self.tag))?;
        }
        Ok(())
    }
}

/// A start running on a thread of its own.
#[derive(Default)]
pub struct Starting {
    done: AtomicBool,
    failed: Mutex<Option<String>>,
}

impl Starting {
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }

    /// Why the chains would not come up, once it has finished.
    pub fn failure(&self) -> Option<String> {
        self.failed.lock().clone()
    }
}

/// Bring `raises` up on a thread of their own and call `then` when they are.
/// None when no thread could be started, and nothing was done.
pub fn start<F>(raises: Vec<Raise>, then: F) -> Option<Arc<Starting>>
where
    F: FnOnce() + Send + 'static,
{
    let starting = Arc::new(Starting::default());
    let state = starting.clone();
    let started = crate::mixer::offload::run("encoder-start", "programme", move || {
        if let Some(e) = raises.iter().map(Raise::run).find_map(Result::err) {
            *state.failed.lock() = Some(format!("{e:#}"));
        }
        state.done.store(true, Ordering::SeqCst);
        then();
    });
    started.then_some(starting)
}
