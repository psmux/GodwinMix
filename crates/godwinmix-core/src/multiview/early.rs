//! The queue in front of a source's mosaic tile, which waits for early frames.
//!
//! A source tile carries the source's own timestamps, shifted onto the
//! programme's clock, and the mosaic compositor draws each frame when its time
//! comes. A live source delivers its frames at about that time. A clip does
//! not: it decodes as fast as the programme's queues take it. On 2026-10-13,
//! with a 1080p30 clip on air, a frame left the source's tee 1 s before it
//! left the source's programme queue, 2 s before it left the programme
//! branch's queue, and about 4 s before the mosaic drew it, the rest being the
//! programme compositor's second of latency and the frames waiting on its pad.
//! Every frame of the clip's tile waited those 4 s at its mosaic pad.
//!
//! The tile queue used to be the preview path's two frame leaky queue. With a
//! frame waiting at the pad and two in the queue, every frame after them was
//! dropped as it arrived, so when the waiting frame came due the next one the
//! pad got was the newest, again seconds ahead. The tile, the Studio preview
//! drawn off it, and the Preview pane showing a single source from it moved
//! about once a second and froze in between, while the programme beside them,
//! stamped on arrival, played smoothly.
//!
//! So a source tile's queue holds up to [`HOLD_SECS`] of frames. They are
//! thumbnails by now, 480 by 270, about 190 kB each, so a clip 4 s ahead at
//! 25 fps costs about 19 MB while its tile is shown, and a live source, whose
//! frames are not early, keeps the queue near empty. It is still leaky: when a
//! mosaic stops reading it drops its oldest frame and never blocks the source.

use crate::gstutil;
use anyhow::Result;
use gstreamer as gst;

/// How far ahead of its time a source tile's frame may arrive and still be
/// drawn: the 4 s a clip runs ahead, and a second of room. With 4 the queue
/// was measured full and dropping, and the tile moved 11 times a second.
pub const HOLD_SECS: f64 = 5.0;

/// The queue at the head of a source's tile branch.
pub fn tile_queue(name: &str) -> Result<gst::Element> {
    gstutil::queue_preview_holding(name, HOLD_SECS)
}
