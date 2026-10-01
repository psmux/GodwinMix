//! Vitals: a thumbnail and the alarms for every direct show, cheap enough
//! for two hundred at once.
//!
//! ```text
//!   hub ──► one reader per show (tap.rs) ──► keyframes only, about 1/s ──┐
//!            packet clock                     3 sound frames, about 1/s ──┤
//!   a decode that exists (offer_frame) ─────► one frame a second ────────┤
//!                                                                         ▼
//!                one waiting job per show ──► a few workers (work.rs), each with
//!                (pool.rs; newest wins)       one decoder per codec, shared by
//!                                             every show (chain.rs)
//!                                                │
//!          luma, black share, change, peak ◄─────┘──► JPEG, only while wanted
//!                     │
//!   once a second: judge.rs ──► event/direct.health on a change of state or kinds
//! ```
//!
//! Nothing here decodes a frame that is not a keyframe, and nothing decodes
//! at all for a show with its alarms off that nobody is looking at. A copy
//! only show's black and freeze checks therefore see one picture per
//! keyframe, at most one a second: they are good to about a second, or to
//! the GOP when that is longer. `docs/reference/show-health.md` has the
//! thresholds and what each part costs.

mod calls;
mod chain;
mod judge;
mod measure;
mod picture;
mod pool;
mod show;
mod tap;
mod ticker;
mod work;
mod registry;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod bench;

// The direct host owns the one `Vitals` and hands it the table rows, the
// counters, output failures, decoded frames and `direct.thumbnail`.
pub use registry::Vitals;

/// Unix milliseconds, the clock every alarm's `since_ms` is on.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
