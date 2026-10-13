//! The queue in front of a source's mosaic tile, which waits for early frames.
//!
//! A source tile carries the source's own timestamps, shifted onto the
//! programme's clock, and the mosaic compositor draws each frame when its time
//! comes. A live source delivers its frames at about that time. A clip does
//! not: it decodes as fast as the programme's queues take it, and those hold
//! a second, so its frames leave its tee and reach the mosaic most of a second
//! before they are due. Each one then waits at its compositor pad.
//!
//! The tile queue used to be the preview path's two frame leaky queue. With a
//! frame waiting at the pad and two more in the queue, every frame after them
//! was dropped as it arrived, so when the waiting frame came due the next one
//! the pad got was the newest, again most of a second ahead. The tile, the
//! Studio preview drawn off it, and the Preview pane showing a single source
//! from it moved once in each such wait and froze in between, while the
//! programme tile beside them, which is stamped on arrival, played smoothly.
//! Measured on 2026-10-13 with a 1080p30 clip on a 25 fps mosaic.
//!
//! So a source tile's queue holds up to [`HOLD_SECS`] of frames. They are
//! thumbnails by now, 480 by 270, about 190 kB each, so a clip a second ahead
//! at 25 fps costs about 5 MB while its tile is shown, and a live source, whose
//! frames are not early, keeps the queue near empty. It is still leaky: when a
//! mosaic stops reading it drops its oldest frame and never blocks the source.

use crate::gstutil;
use anyhow::Result;
use gstreamer as gst;

/// How far ahead of its time a source tile's frame may arrive and still be
/// drawn. Above the second the programme's source queue holds, with room for
/// the frames waiting in the mixers.
pub const HOLD_SECS: f64 = 2.0;

/// The queue at the head of a source's tile branch.
pub fn tile_queue(name: &str) -> Result<gst::Element> {
    gstutil::queue_preview_holding(name, HOLD_SECS)
}
