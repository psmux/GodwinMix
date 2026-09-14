//! The shared normaliser: the part of a source pipeline every kind has.
//!
//! Whatever a kind does upstream (open a socket, spawn a process, render a
//! page), it ends here: rate, convert, scale, caps, then a tee feeding the
//! programme proxy and, when one is wanted, the thumbnail proxy. Audio is
//! convert, resample, caps, proxy. Downstream of the two capsfilters every
//! source in the system is byte for byte interchangeable, which is what makes
//! a take a property change rather than a renegotiation.
//!
//! This was lines 1861 to 1931 of the old `build_kind`, which the audit called
//! "the genuinely reusable part". It is now the only copy.

use crate::caps::CanvasCaps;
use crate::gstutil::{self, make};
use crate::input::{optional_livesync, THUMB_HEIGHT, THUMB_WIDTH};
use crate::state::SourceId;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

/// The thumbnail end. Built only when `start(.., thumb = true)` asked for one.
pub struct ThumbEnd {
    pub queue: gst::Element,
    pub scale: gst::Element,
    pub rate: gst::Element,
    pub caps: gst::Element,
    pub proxy: gst::Element,
}

impl ThumbEnd {
    fn build(id: &SourceId, thumb_fps: i32) -> Result<Self> {
        Ok(Self {
            queue: gstutil::queue_thread(&format!("{id}-vthumb-q"))?,
            scale: make("videoscale", &format!("{id}-tscale"))?,
            rate: make("videorate", &format!("{id}-trate"))?,
            caps: gstutil::capsfilter(
                &format!("{id}-tcaps"),
                &CanvasCaps::video_at(
                    THUMB_WIDTH,
                    THUMB_HEIGHT,
                    gst::Fraction::new(thumb_fps.max(1), 1),
                ),
            )?,
            proxy: make("proxysink", &format!("{id}-tproxy"))?,
        })
    }

    fn elements(&self) -> [&gst::Element; 5] {
        [
            &self.queue,
            &self.scale,
            &self.rate,
            &self.caps,
            &self.proxy,
        ]
    }
}

pub struct Normaliser {
    pub vrate: gst::Element,
    pub vconv: gst::Element,
    pub vscale: gst::Element,
    /// The canvas video capsfilter. Its src pad is the per source filter
    /// insertion point (02, "Where a filter would plug in").
    pub vcaps: gst::Element,
    pub vsync: Option<gst::Element>,
    pub vtee: gst::Element,
    pub vprog_q: gst::Element,
    pub video_proxy: gst::Element,
    pub thumb: Option<ThumbEnd>,
    pub aconv: gst::Element,
    pub ares: gst::Element,
    /// The canvas audio capsfilter, and the audio filter insertion point.
    pub acaps: gst::Element,
    pub audio_proxy: gst::Element,
}

impl Normaliser {
    /// Build every element. Nothing is added to a pipeline or linked yet: the
    /// caller adds the kind's own elements at the same time so one `add_many`
    /// covers the lot.
    pub fn build(
        id: &SourceId,
        canvas: &CanvasCaps,
        thumb_fps: Option<i32>,
        livesync: bool,
    ) -> Result<Self> {
        let vrate = make("videorate", &format!("{id}-vrate"))?;
        // Fill gaps rather than letting the framerate sag. A source that sends
        // 28 fps into a 30 fps canvas must still produce 30 fps.
        crate::probe::set_bool(&vrate, "skip-to-first", true);
        crate::probe::set_bool(&vrate, "drop-only", false);
        let vconv = make("videoconvert", &format!("{id}-vconv"))?;
        // The converter is where a decoder's half-filled colour tag would be
        // acted on, so the tag is completed just before it.
        gstutil::assume_broadcast_colorimetry(&vconv, "sink")?;
        let vscale = make("videoscale", &format!("{id}-vscale"))?;
        let vcaps = gstutil::capsfilter(&format!("{id}-vcaps"), &canvas.video())?;
        // livesync exists to absorb drift and gaps in a *live* stream. A file
        // has neither, and its timestamps start at zero while the programme's
        // running time is minutes in, so livesync judges every early frame late
        // and discards it: an eight second ad lost its first 1.4 seconds. Media
        // sources are rebased on the mixer pad instead.
        let vsync = if livesync {
            optional_livesync(&format!("{id}-vsync"))?
        } else {
            None
        };
        let vtee = make("tee", &format!("{id}-vtee"))?;
        vtee.set_property("allow-not-linked", true);

        Ok(Self {
            vrate,
            vconv,
            vscale,
            vcaps,
            vsync,
            vtee,
            vprog_q: gstutil::queue_thread(&format!("{id}-vprog-q"))?,
            video_proxy: make("proxysink", &format!("{id}-vproxy"))?,
            thumb: thumb_fps.map(|f| ThumbEnd::build(id, f)).transpose()?,
            aconv: make("audioconvert", &format!("{id}-aconv"))?,
            ares: make("audioresample", &format!("{id}-ares"))?,
            acaps: gstutil::capsfilter(&format!("{id}-acaps"), &canvas.audio())?,
            audio_proxy: make("proxysink", &format!("{id}-aproxy"))?,
        })
    }

    pub fn elements(&self) -> Vec<&gst::Element> {
        let mut all = vec![
            &self.vrate,
            &self.vconv,
            &self.vscale,
            &self.vcaps,
            &self.vtee,
            &self.vprog_q,
            &self.video_proxy,
            &self.aconv,
            &self.ares,
            &self.acaps,
            &self.audio_proxy,
        ];
        if let Some(s) = &self.vsync {
            all.push(s);
        }
        if let Some(t) = &self.thumb {
            all.extend(t.elements());
        }
        all
    }

    /// Link the normaliser to itself. Every element must already be in one
    /// pipeline.
    pub fn link(&self) -> Result<()> {
        let mut vchain: Vec<&gst::Element> =
            vec![&self.vrate, &self.vconv, &self.vscale, &self.vcaps];
        if let Some(s) = &self.vsync {
            vchain.push(s);
        }
        vchain.push(&self.vtee);
        gst::Element::link_many(&vchain).context("linking video normaliser")?;
        gst::Element::link_many([&self.vtee, &self.vprog_q, &self.video_proxy])
            .context("linking program video branch")?;
        if let Some(t) = &self.thumb {
            gst::Element::link_many([&self.vtee, &t.queue, &t.rate, &t.scale, &t.caps, &t.proxy])
                .context("linking thumbnail branch")?;
        }
        gst::Element::link_many([&self.aconv, &self.ares, &self.acaps, &self.audio_proxy])
            .context("linking audio normaliser")?;
        Ok(())
    }

    /// Where a kind's video chain joins the shared one.
    pub fn video_entry(&self) -> gst::Element {
        self.vrate.clone()
    }

    /// Where a kind's audio chain joins the shared one.
    pub fn audio_entry(&self) -> gst::Element {
        self.aconv.clone()
    }

    pub fn thumb_proxy(&self) -> Option<gst::Element> {
        self.thumb.as_ref().map(|t| t.proxy.clone())
    }
}
