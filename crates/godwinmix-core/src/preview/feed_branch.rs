//! The GStreamer half of the full quality programme feed (`feed.rs`): built
//! once with the programme, asleep and unlinked until somebody asks.
//!
//! One source at full size (`/mjpeg/{source}?full=1`, what Interact shows) is
//! the same chain on that source's own tee, built in the source's pipeline
//! when the first viewer comes and taken out of it when the last one goes.
//!
//! Everything after the leaky queue runs on that queue's own streaming
//! thread: the rate, the scale, the convert and the JPEG encode. The tee the
//! programme encoder hangs off only ever hands the queue a reference to a
//! buffer, and a full queue drops the oldest frame rather than waiting.

use super::feed::{FeedShape, ProgrammeFeed};
use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::sync::Arc;
use tracing::debug;

/// JPEG quality. 85 is where a 1080p programme on a projector stops showing
/// blocks in a gradient, at about 150 to 300 kB a frame.
pub const QUALITY: i32 = 85;

pub struct FeedBranch {
    chain: Vec<gst::Element>,
    rate: gst::Element,
    filter: gst::Element,
    tee: gst::Element,
    pad: Option<gst::Pad>,
    canvas: (i32, i32),
}

impl FeedBranch {
    /// Build the chain in `pipeline` behind `bridge` (what brings a GPU frame
    /// to system memory, empty on the CPU). Every element is named `{tag}-*`:
    /// `pgm-f` for the programme, `{source}-f` for one source.
    pub fn build(
        pipeline: &gst::Pipeline,
        tee: &gst::Element,
        bridge: Vec<gst::Element>,
        canvas: (i32, i32),
        shared: &Arc<ProgrammeFeed>,
        tag: &str,
    ) -> Result<Self> {
        let rate = make("videorate", &format!("{tag}-rate"))?;
        crate::probe::set_bool(&rate, "drop-only", true);
        crate::probe::set_bool(&rate, "skip-to-first", true);
        let filter = gstutil::capsfilter(&format!("{tag}-caps"), &caps(0, canvas))?;
        let enc = make("jpegenc", &format!("{tag}-jpeg"))?;
        enc.set_property("quality", QUALITY);
        let mut chain = vec![gstutil::queue_preview(&format!("{tag}-q"))?, rate.clone()];
        chain.extend(bridge);
        chain.push(make("videoscale", &format!("{tag}-scale"))?);
        chain.push(make("videoconvert", &format!("{tag}-yuv"))?);
        chain.push(filter.clone());
        chain.push(enc);
        chain.push(sink(shared, tag));
        pipeline.add_many(&chain).context("adding a full quality feed branch")?;
        gst::Element::link_many(&chain).context("linking a full quality feed branch")?;
        for el in &chain {
            el.set_locked_state(true);
        }
        Ok(Self { chain, rate, filter, tee: tee.clone(), pad: None, canvas })
    }

    pub fn attached(&self) -> bool {
        self.pad.is_some()
    }

    /// Make pictures at `shape`, putting the branch on the tee first if it is
    /// off. On the mixer thread.
    pub fn apply(&mut self, shape: FeedShape) -> Result<()> {
        let max_rate = if shape.fps == 0 { i64::from(i32::MAX) } else { i64::from(shape.fps.min(240)) };
        crate::probe::set_int(&self.rate, "max-rate", max_rate);
        let want = caps(shape.width, self.canvas);
        if self.filter.property::<gst::Caps>("caps") != want {
            self.filter.set_property("caps", want);
        }
        if self.pad.is_some() {
            return Ok(());
        }
        let head = self.chain.first().context("the programme feed branch has no head")?;
        let sink = head.static_pad("sink").context("the programme feed branch head has no sink pad")?;
        for el in self.chain.iter().rev() {
            el.set_locked_state(false);
            el.sync_state_with_parent().context("starting the programme feed branch")?;
        }
        let pad = self.tee.request_pad_simple("src_%u").context("the raw programme tee refused a pad for the feed")?;
        pad.link(&sink).context("linking the programme feed branch to the raw tee")?;
        self.pad = Some(pad);
        debug!(?shape, "programme feed branch attached");
        Ok(())
    }

    /// Take the branch off the tee and put it to sleep.
    pub fn detach(&mut self) {
        let Some(pad) = self.pad.take() else { return };
        if let Some(peer) = pad.peer() {
            let _ = pad.unlink(&peer);
        }
        self.tee.release_request_pad(&pad);
        for el in &self.chain {
            el.set_locked_state(true);
            let _ = el.set_state(gst::State::Null);
        }
        debug!("programme feed branch detached: nobody is watching");
    }

    /// Detach, then take every element out of its pipeline. For a branch
    /// built on demand, which is built again from nothing next time.
    pub fn dismantle(mut self) {
        self.detach();
        for el in &self.chain {
            if let Some(bin) = el.parent().and_then(|p| p.downcast::<gst::Bin>().ok()) {
                let _ = bin.remove(el);
            }
        }
    }
}

/// The picture the encoder is handed: `width` wide at the canvas's shape, or
/// the canvas's own size for zero or anything wider. Never upscaled.
pub fn caps(width: u32, canvas: (i32, i32)) -> gst::Caps {
    let (cw, ch) = (canvas.0.max(2), canvas.1.max(2));
    let w = if width == 0 { cw } else { (width as i32).clamp(16, cw) };
    let h = (i64::from(w) * i64::from(ch) / i64::from(cw)).max(2) as i32;
    gst::Caps::builder("video/x-raw")
        .field("width", w & !1)
        .field("height", h & !1)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build()
}

/// The newest JPEG only, never a clock wait, never a preroll.
fn sink(shared: &Arc<ProgrammeFeed>, tag: &str) -> gst::Element {
    let sink = gst_app::AppSink::builder().name(format!("{tag}-sink")).max_buffers(1).drop(true).sync(false).build();
    sink.set_property("async", false);
    let shared = shared.clone();
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                if let Some(map) = sample.buffer().and_then(|b| b.map_readable().ok()) {
                    shared.publish(map.as_slice());
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    sink.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(c: &gst::Caps) -> (i32, i32) {
        let s = c.structure(0).unwrap();
        (s.get("width").unwrap(), s.get("height").unwrap())
    }

    #[test]
    fn the_feed_is_the_canvas_size_or_smaller_and_never_upscaled() {
        gst::init().unwrap();
        assert_eq!(size(&caps(0, (1920, 1080))), (1920, 1080), "zero is the programme's own size");
        assert_eq!(size(&caps(1280, (1920, 1080))), (1280, 720));
        assert_eq!(size(&caps(3840, (1920, 1080))), (1920, 1080), "a 4K display gets the 1080p programme");
        assert_eq!(size(&caps(1366, (1280, 720))), (1280, 720));
        assert_eq!(size(&caps(1024, (1080, 1920))), (1024, 1820), "a portrait canvas keeps its shape");
    }

    /// A real pipeline: a 1080p test pattern through the branch, attached,
    /// reshaped and detached, with the frames counted at each step.
    #[test]
    fn the_branch_makes_full_size_jpegs_only_while_attached() {
        gst::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc").property("is-live", true).build().unwrap();
        let capsf = gstutil::capsfilter("t-caps", &"video/x-raw,width=1920,height=1080,framerate=30/1".parse().unwrap()).unwrap();
        let tee = gst::ElementFactory::make("tee").property("allow-not-linked", true).build().unwrap();
        pipeline.add_many([&src, &capsf, &tee]).unwrap();
        gst::Element::link_many([&src, &capsf, &tee]).unwrap();
        let feed = ProgrammeFeed::new();
        let mut branch = FeedBranch::build(&pipeline, &tee, Vec::new(), (1920, 1080), &feed, "pgm-f").unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(feed.frames(), 0, "nothing is encoded before anybody asks");

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let clients = super::super::StreamClients::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let switch: super::super::feed::Switch = Arc::new(move |s| {
            let _ = tx.send(s);
        });
        let mut sub = feed.subscribe(super::super::feed::FeedAsk::default(), switch, &clients);
        branch.apply(rx.recv().unwrap().unwrap()).unwrap();
        let jpeg = rt.block_on(async { tokio::time::timeout(std::time::Duration::from_secs(5), sub.recv()).await }).unwrap().unwrap();
        let img = crate::snapshot::decode_jpeg(&jpeg).unwrap();
        assert_eq!(img.dimensions(), (1920, 1080), "the projector gets the whole programme");
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let second = feed.frames();
        assert!(second >= 20, "at the programme's rate, not the mosaic's: {second} frames in about a second");

        drop(sub);
        assert_eq!(rx.recv().unwrap(), None);
        branch.detach();
        let after = feed.frames();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(feed.frames() <= after + 1, "detached, the branch stops encoding");
        pipeline.set_state(gst::State::Null).unwrap();
    }
}
