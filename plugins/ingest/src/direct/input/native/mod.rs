//! A native MPEG-TS input for `udp://` and `rtp://`: one socket per
//! multicast group, shared by every show that takes a program from it, and
//! the demuxing and framing in Rust, with no GStreamer on the data path.
//!
//! ```text
//!   socket thread, one per group ──► PAT, PMT, SDT read once (the udp plugin's filter)
//!     │  each program's own packets, through a bounded queue that never blocks
//!     ├──► show thread: program ──► PES ──► es parser ──► tags ──► hub
//!     └──► show thread: program ──► ...
//! ```
//!
//! The tags are byte for byte the ones the GStreamer path (`udpsrc`,
//! `parsebin`, `h264parse` and the tagger) makes, so everything after the
//! hub is unchanged. `docs/explanation/direct-host.md` has the numbers.

mod clock;
pub mod es;
mod pes;
mod program;
mod stream;
mod timeline;
mod tracks;

/// What one program's packets have counted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub bytes: u64,
    pub cc_errors: u64,
    pub ts_lost: u64,
    pub scrambled: u64,
}

#[cfg(test)]
mod tests;
