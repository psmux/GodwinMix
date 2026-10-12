//! How many threads a software video decoder inside a `uridecodebin` gets.
//!
//! FFmpeg's decoders default to one thread per core, and with frame threading
//! each thread keeps a picture of its own in flight. On a four core runner
//! that is four 720p frames and four decoding contexts per source, for a
//! picture the canvas shows at 30 frames a second. Two threads decode 720p30
//! H.264 with room to spare on a Pi 4 class core; a machine that wants more
//! for a 4K clip can say so with `GODWINMIX_DECODE_THREADS`.

use gstreamer as gst;
use gstreamer::prelude::*;

/// What a decoder gets when nothing says otherwise.
const DEFAULT_THREADS: i32 = 2;

/// Watch `bin` for the decoders it plugs and cap their threads.
pub fn cap(bin: &gst::Element) {
    let Some(bin) = bin.downcast_ref::<gst::Bin>() else { return };
    let threads = threads();
    bin.connect_deep_element_added(move |_, _, el| {
        let is_decoder = el
            .factory()
            .is_some_and(|f| f.klass().contains("Decoder") && f.klass().contains("Video"));
        if is_decoder && el.find_property("max-threads").is_some() {
            crate::probe::set_int(el, "max-threads", threads as i64);
        }
    });
}

fn threads() -> i32 {
    std::env::var("GODWINMIX_DECODE_THREADS")
        .ok()
        .and_then(|v| v.trim().parse::<i32>().ok())
        .filter(|n| *n >= 0)
        .unwrap_or(DEFAULT_THREADS)
}
