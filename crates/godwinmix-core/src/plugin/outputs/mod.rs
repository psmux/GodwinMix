//! The outputs the core ships with. Adding one is a file like `srt.rs`: a
//! manifest, a `build` that makes a muxer and a sink, and an honest
//! `connected`.

pub mod rtmp;
pub mod srt;

pub mod record;
