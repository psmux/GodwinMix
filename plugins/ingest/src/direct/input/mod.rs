//! Every input a direct show can take, each yielding the hub's `MediaTag`
//! stream and `InputStats`, with nothing decoded.
//!
//! | Address | Module | What it does |
//! |---|---|---|
//! | `udp://`, `rtp://` | `ts_in` | MPEG-TS, bare or in RTP, unicast or multicast, one program |
//! | `srt://` | `ts_in` | SRT caller, or listener for `srt://@:port` |
//! | `rist://` | `rist` | RIST Simple Profile, listening |
//! | `rtsp://` | `rtsp` | a camera or an encoder, over TCP or UDP |
//! | `http(s)://` | `pull` | HLS or DASH |
//! | `rtmp://` | `pull` | someone else's RTMP server, played |
//! | `file://` | `pull` | a TS or MP4 file, looped at its own pace |
//! | `channel:` | `channel` | a channel's stream, off the hub |
//!
//! [`open`] checks the address and answers an [`Input`] whose `run` the host
//! puts on a thread of its own.

mod backup;
mod channel;
mod frames;
mod loss;
mod meter;
mod outlet;
mod pads;
mod pull;
mod rist;
mod rtsp;
mod runner;
pub mod spec;
pub mod stats;
mod ts_in;

use super::{Input, Sink, StopSignal};
use crate::hub::Hub;
pub use spec::{InputError, InputSpec, Kind};
pub use stats::InputStats;

/// What an input may need from the host it runs in.
#[derive(Clone, Default)]
pub struct Context {
    /// The channels' hub, for `channel:` inputs.
    pub hub: Option<Hub>,
}

/// The input `spec` describes, ready to run, or why it cannot be.
pub fn open(spec: &InputSpec, ctx: &Context) -> Result<Box<dyn Input>, InputError> {
    gmx_netkit::init().map_err(|e| InputError::new(e, serde_json::json!({})))?;
    let main = one(spec, ctx)?;
    match &spec.backup {
        Some(b) => Ok(Box::new(backup::Backup::new(main, one(b, ctx)?, spec))),
        None => Ok(main),
    }
}

fn one(spec: &InputSpec, ctx: &Context) -> Result<Box<dyn Input>, InputError> {
    Ok(match spec.kind()? {
        Kind::Udp => Box::new(Gst(ts_in::Udp::new(spec)?)),
        Kind::Srt => Box::new(Gst(ts_in::Srt::new(spec))),
        Kind::Rist => Box::new(Gst(rist::Rist::new(spec)?)),
        Kind::Rtsp => Box::new(Gst(rtsp::Rtsp::new(spec)?)),
        Kind::Http => Box::new(Gst(pull::Uri::new(spec, true))),
        Kind::Rtmp => Box::new(Gst(pull::Uri::new(spec, false))),
        Kind::File => Box::new(Gst(pull::File::new(spec)?)),
        Kind::Channel => Box::new(channel::Channel::new(spec, ctx.hub.as_ref())?),
    })
}

/// A GStreamer backed input: its plan, run by the runner.
struct Gst<P: runner::Plan>(P);

impl<P: runner::Plan> Input for Gst<P> {
    fn run(self: Box<Self>, out: Sink, stop: StopSignal) {
        runner::run(self.0, out, stop);
    }
}

#[cfg(test)]
mod tests;
