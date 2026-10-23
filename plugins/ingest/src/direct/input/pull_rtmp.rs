//! Someone else's RTMP server, played.
//!
//! ```text
//!   rtmpsrc | rtmp2src ──► parsebin ──► tags
//! ```
//!
//! GStreamer has two RTMP clients. `rtmp2src` keeps only the messages on
//! the stream `createStream` gave it, so it hears nothing from a server that
//! sends its media on stream 0, as `ffmpeg -listen 1` does, and a server that
//! reads chunk headers strictly (MediaMTX) closes it at `createStream`.
//! librtmp's `rtmpsrc` plays from both, so `rtmp://` takes it wherever it is
//! installed, and `rtmp2src` where it is not. They do not take turns: a
//! server like `ffmpeg -listen 1` answers one connection, and a turn of
//! `rtmp2src` used it up and left the next `rtmpsrc` nothing to call.
//! `rtmps://` keeps `rtmp2src`, whose TLS is GStreamer's own.

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
}

impl Rtmp {
    pub fn new(spec: &InputSpec) -> Rtmp {
        Rtmp { uri: spec.uri.clone() }
    }

    /// The client, and the location it is given.
    fn client(&self) -> (&'static str, String) {
        if self.uri.starts_with("rtmp://") && gst::ElementFactory::find("rtmpsrc").is_some() {
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
    fn librtmp_plays_rtmp_where_it_is_installed_and_rtmps_keeps_rtmp2src() {
        gst::init().unwrap();
        let r = Rtmp::new(&InputSpec::new("rtmp://host/live/key"));
        if gst::ElementFactory::find("rtmpsrc").is_some() {
            assert_eq!(r.client(), ("rtmpsrc", "rtmp://host/live/key live=1".to_string()));
        } else {
            assert_eq!(r.client().0, "rtmp2src");
        }
        assert_eq!(Rtmp::new(&InputSpec::new("rtmps://host/live/key")).client(), ("rtmp2src", "rtmps://host/live/key".to_string()));
        assert_eq!(r.client(), r.client(), "the same client every time");
    }
}
