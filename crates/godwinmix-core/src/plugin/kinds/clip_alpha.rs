//! A clip whose decoder hands out alpha: WebM with VP8 or VP9 alpha, ProRes
//! 4444, QuickTime Animation, PNG in a MOV.
//!
//! The decoded pad is looked at when it appears. Without alpha it goes to the
//! normaliser exactly as every clip does. With alpha it goes to a tee:
//!
//! ```text
//!   decoder =| tee -> queue -> convert -> AYUV -> appsink (paced)  => layer
//!               \-> queue -> videorate (2 fps) -> convert/scale -> normaliser
//! ```
//!
//! The appsink keeps the newest frame as the layer's picture, which the board
//! draws; the second branch is the carrier, two frames a second for the tile
//! and the supervisor. The programme branch drops it, so nothing upstream is
//! paced by the compositor any more, and the appsink paces the clip itself:
//! its `ts-offset` is set on the first frame so the clip plays from now in
//! real time.

use super::clip_pace;
use crate::gstutil::{self, make};
use crate::overlay::{Layer, Picture};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use std::sync::Arc;

/// The elements of the alpha branch, built up front and joined only when a
/// decoder turns out to give alpha.
pub struct AlphaTap {
    pub tee: gst::Element,
    picture: Vec<gst::Element>,
    carrier: Vec<gst::Element>,
    sink: gst_app::AppSink,
    pub layer: Arc<Layer>,
}

impl AlphaTap {
    pub fn build(id: &str, canvas: &crate::caps::CanvasCaps) -> Result<AlphaTap> {
        let tee = make("tee", &format!("{id}-alpha-tee"))?;
        let ayuv = gst::Caps::builder("video/x-raw").field("format", "AYUV").field("colorimetry", crate::caps::COLORIMETRY).build();
        // Scaled here, once per frame, to the size the item is drawn at, so
        // the board blends it without scaling and the conversion to AYUV is
        // only as big as the box.
        let picture = vec![
            gstutil::queue_thread(&format!("{id}-alpha-q"))?,
            make("videoscale", &format!("{id}-alpha-scale"))?,
            make("videoconvert", &format!("{id}-alpha-convert"))?,
            gstutil::capsfilter(&format!("{id}-alpha-caps"), &ayuv)?,
        ];
        // Not part of preroll: a clip that turns out opaque never feeds it,
        // and a sink waiting for its first buffer would hold the source in
        // PAUSED for ever.
        let sink = gst_app::AppSink::builder().name(format!("{id}-alpha-sink")).sync(true).max_buffers(2).drop(false).build();
        sink.set_property("async", false);
        let rate = make("videorate", &format!("{id}-alpha-rate"))?;
        crate::probe::set_int(&rate, "max-rate", 2);
        crate::probe::set_bool(&rate, "drop-only", true);
        let carrier = vec![
            gstutil::queue_preview(&format!("{id}-alpha-cq"))?,
            rate,
            make("videoconvert", &format!("{id}-alpha-cconvert"))?,
            make("videoscale", &format!("{id}-alpha-cscale"))?,
            gstutil::capsfilter(&format!("{id}-alpha-ccaps"), &carrier_caps(canvas))?,
        ];
        let layer = Layer::new(false);
        Ok(AlphaTap { tee, picture, carrier, sink, layer })
    }

    pub fn elements(&self) -> Vec<gst::Element> {
        let mut all = vec![self.tee.clone()];
        all.extend(self.picture.iter().cloned());
        all.push(self.sink.clone().upcast());
        all.extend(self.carrier.iter().cloned());
        all
    }

    /// Link both branches to the tee. The carrier's far end is left free
    /// until a decoder turns out to give alpha (`join`), because until then
    /// the normaliser's entry may be wanted by the decoder itself.
    pub fn link(&self) -> Result<()> {
        let mut pic: Vec<&gst::Element> = vec![&self.tee];
        pic.extend(self.picture.iter());
        pic.push(self.sink.upcast_ref());
        gst::Element::link_many(pic).context("linking the alpha picture branch")?;
        let mut car: Vec<&gst::Element> = vec![&self.tee];
        car.extend(self.carrier.iter());
        gst::Element::link_many(car).context("linking the alpha carrier branch")?;
        clip_pace::pace_from_now(&self.sink);
        let caps = self.picture.last().cloned();
        self.layer.on_resize(move |(w, h)| {
            if let Some(c) = &caps {
                let sized = gst::Caps::builder("video/x-raw")
                    .field("format", "AYUV")
                    .field("colorimetry", crate::caps::COLORIMETRY)
                    .field("width", w.max(2) as i32)
                    .field("height", h.max(2) as i32)
                    .build();
                c.set_property("caps", sized);
            }
        });
        let layer = self.layer.clone();
        self.sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    if let Some(p) = picture(&sample) {
                        layer.set_picture(Some(Arc::new(p)));
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        Ok(())
    }

    /// Send a decoded pad with alpha this way: the carrier to the normaliser,
    /// the pad to the tee, and the layer on. Called from `pad-added`.
    pub fn join(&self, pad: &gst::Pad, video_entry: &gst::Element) -> Result<()> {
        let end = self.carrier.last().context("no carrier branch")?;
        end.link(video_entry).context("linking the alpha carrier to the canvas")?;
        let sink = self.tee.static_pad("sink").context("the alpha tee has no sink")?;
        pad.link(&sink).map_err(|e| anyhow::anyhow!("linking the clip to its alpha branch: {e:?}"))?;
        self.layer.activate(true);
        Ok(())
    }
}

/// The canvas size in I420 at any rate: the normaliser's own rate does the
/// rest, by repeating.
fn carrier_caps(canvas: &crate::caps::CanvasCaps) -> gst::Caps {
    gst::Caps::builder("video/x-raw")
        .field("format", crate::caps::VIDEO_FORMAT)
        .field("width", canvas.width)
        .field("height", canvas.height)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build()
}

/// One decoded frame as the layer's picture, from the decoder's own buffer.
fn picture(sample: &gst::Sample) -> Option<Picture> {
    let info = gst_video::VideoInfo::from_caps(sample.caps()?).ok()?;
    let buffer = sample.buffer_owned()?;
    let stride = buffer.meta::<gst_video::VideoMeta>().map(|m| m.stride()[0]).unwrap_or(info.stride()[0]) as usize;
    let (w, h) = (info.width(), info.height());
    Some(Picture { buffer, width: w, height: h, stride, natural: (w, h), within: None, keyed: None })
}
