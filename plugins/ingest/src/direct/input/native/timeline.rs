//! Units to hub tags on one session's timeline: one zero for the video and
//! every sound track, the first time any of them gave, as `tagger::Zero`
//! sets it for the GStreamer path.

use std::sync::Arc;

use gstreamer::ClockTime;

use super::stream::Out;
use crate::media_tag::MediaTag;
use crate::tagger::Zero;

pub fn tag(out: Out, zero: &Zero) -> MediaTag {
    let u = out.unit;
    MediaTag {
        track: out.track,
        kind: out.kind,
        timestamp_ms: zero.ms(ClockTime::from_nseconds(u.at)),
        keyframe: u.keyframe,
        sequence_header: u.header,
        payload: Arc::from(u.body),
    }
}
