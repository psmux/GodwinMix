//! The HLS packager: a process of its own that packages the HLS outputs of
//! shows without compositing, so a crash in GStreamer there costs those
//! outputs and never the station's control port, a channel or a direct
//! show's other outputs.
//!
//! ```text
//!   direct host: input ──► hub "direct.<id>/main" ──► relay (loopback) ──┐
//!                (and a rendition's pair, when one is asked for)          │
//!                                                                         ▼
//!   packager:  one thread per HLS output ──► appsrc ──► hls::attach (cmafmux, ring)
//!                                                                         │
//!   station:   /hls/<output>/master.m3u8?show=<id>&key=...  key checked,  │
//!              then forwarded on loopback ◄───────────────────────────────┘
//! ```
//!
//! It is this binary run with `--hls-packager`, started by the station
//! (`station::direct::hls::keep`) only while an HLS output of a direct show
//! is on, and started again by it when it dies. The station holds the
//! viewer keys, lets a player in or refuses it, and hands every request it
//! let in to this process's loopback port, where the control port's own
//! `/hls` handlers answer it from the rings. Nothing is decoded: the relay
//! hands over the input's own frames, or the pair a rendition made, and
//! `cmafmux` repackages them.
//!
//! The station talks to it over that same port ([`wire`]): the outputs to
//! run, and what each one is doing.

mod board;
pub mod caps;
mod feed;
mod flv;
mod output;
mod outputs;
mod run;
mod serve;
mod session;
pub mod wire;

pub use run::run;
