//! GStreamer on both ends of the bus.
//!
//! `gmxbussink bus-name=camera:cam-wide` at the end of an owner's decode
//! pipeline publishes every buffer it is given. `gmxbussrc
//! bus-name=camera:cam-wide` at the start of a reader's pipeline hands out
//! each frame as a buffer whose memory is the shared slot itself, with a
//! `GstVideoMeta` giving the plane offsets and strides; the slot's lease is
//! given back when the last reference to the buffer goes. Call [`register`]
//! once, after `gst::init`, to make both names known to `parse::launch` and
//! `ElementFactory::make` in this process.

use gstreamer as gst;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;

use crate::{Error, Format, Layout, Publisher};

mod sink;
mod src;

pub use sink::BusSink;
pub use src::BusSrc;

/// The caps of the `ReferenceTimestampMeta` every buffer from `gmxbussrc`
/// carries: the monotonic time the owner was handed the frame, for end to
/// end latency across processes.
pub const CAPTURED_CAPS: &str = "timestamp/x-gmx-monotonic";

/// Register `gmxbussink` and `gmxbussrc` in this process. Safe to call again.
pub fn register() -> Result<(), glib::BoolError> {
    use glib::prelude::StaticType;
    gst::Element::register(None, "gmxbussink", gst::Rank::NONE, BusSink::static_type())?;
    gst::Element::register(None, "gmxbussrc", gst::Rank::NONE, BusSrc::static_type())
}

/// The bus layout for frames GStreamer describes with `info`.
pub fn layout_of(info: &gst_video::VideoInfo) -> Result<Layout, Error> {
    let format = Format::from_name(info.format().to_str()).ok_or_else(|| {
        Error::BadLayout(format!(
            "the frame bus does not carry {}. Put a videoconvert before it, to NV12",
            info.format().to_str()
        ))
    })?;
    let fps = info.fps();
    Ok(Layout::new(format, info.width(), info.height())?
        .with_fps(fps.numer().max(0) as u32, fps.denom().max(1) as u32))
}

/// Caps for frames laid out as `layout`.
pub fn caps_of(layout: &Layout) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", layout.format.name())
        .field("width", layout.width as i32)
        .field("height", layout.height as i32)
        .field(
            "framerate",
            gst::Fraction::new(layout.fps_n as i32, layout.fps_d.max(1) as i32),
        )
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build()
}

/// The formats both elements accept, as caps.
pub fn template_caps() -> gst::Caps {
    let formats: Vec<&str> = Format::ALL.iter().map(|f| f.name()).collect();
    gst::Caps::builder("video/x-raw")
        .field("format", gst::List::new(formats))
        .field("width", gst::IntRange::new(1, 16384))
        .field("height", gst::IntRange::new(1, 16384))
        .field(
            "framerate",
            gst::FractionRange::new(gst::Fraction::new(0, 1), gst::Fraction::new(i32::MAX, 1)),
        )
        .build()
}

impl Publisher {
    /// Publish a GStreamer buffer described by `info`, changing the bus's
    /// format first if `info` differs from it. `Ok(false)` when every slot was
    /// leased and the frame was dropped.
    pub fn push_buffer(
        &mut self,
        buffer: &gst::BufferRef,
        info: &gst_video::VideoInfo,
    ) -> Result<bool, Error> {
        let layout = layout_of(info)?;
        if layout != self.layout() {
            self.set_layout(layout)?;
        }
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, info)
            .map_err(|_| Error::BadLayout("the buffer is smaller than its caps say".into()))?;
        let planes: Vec<(&[u8], usize)> = (0..frame.n_planes())
            .map(|i| {
                (
                    frame.plane_data(i).unwrap_or(&[]),
                    frame.plane_stride()[i as usize] as usize,
                )
            })
            .collect();
        let pts = buffer.pts().map(|t| t.nseconds());
        let duration = buffer.duration().map(|t| t.nseconds());
        Ok(self.write(pts, duration, |dst| {
            crate::publisher::stats::copy_planes(&layout, &planes, dst)
        }))
    }
}
