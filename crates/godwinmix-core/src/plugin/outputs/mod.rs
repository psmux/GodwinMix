//! The outputs the core ships with. Adding one is a file like `srt.rs`: a
//! manifest, a `build` that makes a muxer and a sink, and an honest
//! `connected`.

pub mod rtmp;
pub mod flv;
pub mod rist;
pub mod rist_live;
pub mod progress;
pub mod srt;
pub mod srt_live;
pub mod srt_params;
pub mod ts;

pub mod record;
