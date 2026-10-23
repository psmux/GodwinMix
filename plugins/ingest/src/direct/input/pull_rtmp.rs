//! Someone else's RTMP server, played.
//!
//! ```text
//!   rtmpsrc | rtmp2src ──► parsebin ──► tags
//! ```
//!
//! GStreamer has two RTMP clients and neither plays from every server.
//! `rtmp2src` keeps only the messages on the stream `createStream` gave it,
//! so it hears nothing from a server that sends its media on stream 0, as
//! `ffmpeg -listen 1` does; and a server that reads chunk headers strictly
//! (MediaMTX) closes it at `createStream`. librtmp's `rtmpsrc` plays from
//! both, so it goes first where it is installed. The connections then take
//! turns, `rtmp2src` next, so a server librtmp cannot play costs one failed
//! connection, or one stall, before the other plays it. `ffmpeg -listen 1`
//! answers one connection and no more, so it must be the first that plays.

use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;

use super::loss::Loss;
use super::pads::{make, parse_into, Pads};
use super::pull::redact;
use super::runner::Plan;
use super::spec::InputSpec;

pub struct Rtmp {
    uri: String,
    tries: u32,
}

impl Rtmp {
    pub fn new(spec: &InputSpec) -> Rtmp {
        Rtmp { uri: spec.uri.clone(), tries: 0 }
    }

    /// The client this connection uses, and the location it is given.
    fn client(&mut self) -> (&'static str, String) {
        let turn = self.tries;
        self.tries = self.tries.wrapping_add(1);
        if turn % 2 == 0 && gst::ElementFactory::find("rtmpsrc").is_some() {
            // `live=1`, or librtmp asks for the stream from its start.
            ("rtmpsrc", format!("{} live=1", self.uri))
        } else {
            ("rtmp2src", self.uri.clone())
        }
    }
}

impl Plan for Rtmp {
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String> {
        let (factory, location) = self.client();
        let src = make(factory)?;
        src.set_property("location", location);
        pipeline.add(&src).map_err(|e| e.to_string())?;
        parse_into(pipeline, &src.static_pad("src").ok_or_else(|| format!("{factory} has no src pad"))?, pads)?;
        Ok(Loss::none())
    }

    fn address(&self) -> String {
        redact(&self.uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_clients_take_turns_and_librtmp_is_asked_for_the_live_stream() {
        gst::init().unwrap();
        let mut r = Rtmp::new(&InputSpec::new("rtmp://host/live/key"));
        let first = r.client();
        assert_eq!(r.client(), ("rtmp2src", "rtmp://host/live/key".to_string()));
        if gst::ElementFactory::find("rtmpsrc").is_some() {
            assert_eq!(first, ("rtmpsrc", "rtmp://host/live/key live=1".to_string()));
            assert_eq!(r.client().0, "rtmpsrc");
        } else {
            assert_eq!(first.0, "rtmp2src");
        }
    }
}
