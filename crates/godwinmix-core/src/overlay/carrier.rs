//! The carrier: what a transparent source sends through the ordinary source
//! path while the board draws its real picture.
//!
//! Every source ends at the shared normaliser, and the supervisor, the
//! thumbnail, the multiview tile and the slot binding all hang off that. A
//! transparent source keeps all of it by sending one frame there: its picture
//! flattened onto dark grey at the canvas size, made once per change and
//! repeated by `imagefreeze` as a live stream. Already at the canvas caps, so
//! the normaliser's rate, convert and scale pass it through untouched, and the
//! programme branch drops it before the compositor (`board::hold_back`).

use super::blend::{self, Draw, Planes, Rect, Source};
use super::picture::Picture;
use super::place::{fitted, Fit};
use crate::caps::CanvasCaps;
use crate::gstutil::make;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

/// The grey a transparent area shows as on a tile. Dark enough to read white
/// text on, light enough to tell from a source that is black.
const GREY: u8 = 48;

pub struct Carrier {
    pub src: gst_app::AppSrc,
    pub freeze: gst::Element,
    info: gst_video::VideoInfo,
}

impl Carrier {
    pub fn build(id: &str, canvas: &CanvasCaps) -> Result<Carrier> {
        let caps = canvas.video();
        let info = gst_video::VideoInfo::from_caps(&caps).context("the canvas caps are not a video format")?;
        let src = gst_app::AppSrc::builder()
            .name(format!("{id}-carrier"))
            .caps(&caps)
            .format(gst::Format::Time)
            .is_live(false)
            .build();
        let freeze = make("imagefreeze", &format!("{id}-carrier-freeze"))?;
        freeze.set_property("is-live", true);
        freeze.set_property("allow-replace", true);
        Ok(Carrier { src, freeze, info })
    }

    pub fn elements(&self) -> Vec<gst::Element> {
        vec![self.src.clone().upcast(), self.freeze.clone()]
    }

    pub fn link(&self) -> Result<()> {
        self.src.link(&self.freeze).context("linking the carrier to its freeze")
    }

    /// Send `picture`, or an empty grey frame, as the source's still.
    pub fn show(&self, picture: Option<&Picture>) {
        match flatten(&self.info, picture) {
            Ok(buffer) => {
                let _ = self.src.push_buffer(buffer);
            }
            Err(e) => tracing::warn!(error = %e, "could not make the carrier frame"),
        }
    }
}

/// `picture` contained in a grey canvas frame.
pub fn flatten(info: &gst_video::VideoInfo, picture: Option<&Picture>) -> Result<gst::Buffer> {
    let mut buffer = gst::Buffer::with_size(info.size()).context("allocating the carrier frame")?;
    {
        let buf = buffer.get_mut().context("a new buffer is writable")?;
        let mut frame = gst_video::VideoFrameRef::from_buffer_ref_writable(buf, info)
            .map_err(|e| anyhow::anyhow!("mapping the carrier frame: {e}"))?;
        let s = info.stride();
        let strides = [s[0] as usize, s[1] as usize, s[2] as usize];
        let [y, u, v, _] = frame.planes_data_mut();
        y.fill(GREY);
        u.fill(128);
        v.fill(128);
        let (w, h) = (info.width() as i32, info.height() as i32);
        if let Some(pic) = picture {
            let map = pic.buffer.map_readable().context("reading the picture")?;
            let canvas = Rect::new(0, 0, w, h);
            let to = fitted(canvas, pic.natural, Fit::Contain);
            let draw = Draw { window: Rect::new(0, 0, pic.width as i32, pic.height as i32), to, clip: canvas, alpha: 255 };
            let mut planes = Planes { y, u, v, strides, width: w, height: h };
            blend::draw(&mut planes, &Source { data: &map, stride: pic.stride }, &draw);
        }
    }
    Ok(buffer)
}
