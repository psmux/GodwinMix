//! The programme as one small picture a second, for a monitoring wall.
//!
//! A station's wall shows every show it runs, fifty or two hundred of them,
//! each as a picture 160 or 320 pixels wide every two seconds. A show that
//! composites used to answer that from its mosaic, which meant building a
//! compositor of every source plus the programme, encoding it as JPEG eight
//! times a second and decoding each of those frames again for motion: a few
//! percent of a core per show on a laptop, which fifty shows on a Raspberry
//! Pi cannot pay. This is the cheap way to the same picture.
//!
//! ```text
//!   vraw_tee ──► queue (leaky, 2) ──► videorate max-rate=1 ──► [download]
//!          ──► videoscale ──► videoconvert ──► RGB 320 wide ──► appsink
//! ```
//!
//! The branch hangs off the raw programme tee only while somebody asked in
//! the last [`ASKED_FOR`]. Its queue is leaky, so a slow branch drops its own
//! frames and never pushes back on the tee the encoder is on. The rate is cut
//! to one frame a second before anything is scaled or converted. The appsink
//! keeps the newest frame and nothing else; the JPEG is made when somebody
//! asks, on a blocking thread, at the width they asked for, and kept so a
//! second asker in the same second costs nothing.
//!
//! The answer has the shape the direct host's `direct.thumbnail` has, so a
//! station serves a show that composites and one that does not alike.

use crate::gstutil::{self, make};
use crate::snapshot;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use tracing::{debug, warn};

/// How long one ask keeps pictures coming. The wall asks every two seconds
/// for the rows on screen, so a row scrolled away stops costing anything ten
/// seconds later. The direct host keeps the same ten seconds.
pub const ASKED_FOR: Duration = Duration::from_secs(10);

/// The width the branch scales the programme to. The wall asks for 160 in
/// rows and 320 in tiles; a smaller ask is scaled down from this, a larger one
/// gets this.
pub const BRANCH_WIDTH: u32 = 320;

/// How long the first ask after a quiet spell waits for the branch's first
/// frame before it answers that one is on its way.
pub const FIRST_WAIT: Duration = Duration::from_millis(1500);

/// One picture of the programme as a client gets it.
#[derive(Debug, Clone)]
pub struct Thumb {
    pub jpeg: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    /// When the frame it was made from left the programme, in Unix ms.
    pub at_ms: u64,
}

/// The newest raw frame the branch kept.
#[derive(Clone)]
struct Frame {
    buffer: gst::Buffer,
    width: u32,
    height: u32,
    stride: usize,
    at_ms: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// What the handle and the branch share: the newest frame, the JPEG made from
/// it, and whether anyone has asked lately.
#[derive(Default)]
pub struct ProgrammeThumb {
    latest: Mutex<Option<Frame>>,
    encoded: Mutex<Option<Thumb>>,
    until: Mutex<Option<Instant>>,
    running: AtomicBool,
    starts: AtomicU64,
    frames: AtomicU64,
    /// `gmx_stream_clients{kind="thumbnail"}` is 1 while the branch is on.
    counted: Mutex<Option<super::ClientGuard>>,
}

/// Puts the branch on the tee (`true`) or takes it off (`false`). A send on
/// the mixer's queue, which never waits.
pub type Switch = Arc<dyn Fn(bool) + Send + Sync>;

impl ProgrammeThumb {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// How many times the branch has been asked onto the tee since boot.
    pub fn starts(&self) -> u64 {
        self.starts.load(Ordering::Relaxed)
    }

    /// How many frames the branch has kept since boot. Flat while nobody is
    /// asking, which is what a test reads to know the work stopped.
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// Whether the branch is wanted on the tee right now.
    pub fn running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Say somebody wants pictures for the next [`ASKED_FOR`]. The first ask
    /// after a quiet spell puts the branch on (`switch(true)`) and starts the
    /// watch that takes it off again (`switch(false)`) once asks stop.
    pub fn want(self: &Arc<Self>, switch: Switch, clients: &Arc<super::StreamClients>) {
        // The deadline and the flag move under one lock, so an ask that lands
        // while the watch is giving up is never lost: either the watch sees
        // the new deadline, or this sees the flag down and starts again.
        // The switch is a send on the mixer's queue that never waits, so it
        // goes under the lock too, which keeps a `false` and the next `true`
        // in the order they were decided.
        let mut until = self.until.lock();
        *until = Some(Instant::now() + ASKED_FOR);
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        self.starts.fetch_add(1, Ordering::Relaxed);
        *self.counted.lock() = Some(clients.open("thumbnail"));
        switch(true);
        drop(until);
        tokio::spawn(self.clone().watch(switch));
    }

    async fn watch(self: Arc<Self>, switch: Switch) {
        loop {
            let left = self.until.lock().map(|u| u.saturating_duration_since(Instant::now())).unwrap_or_default();
            if !left.is_zero() {
                tokio::time::sleep(left).await;
                continue;
            }
            let until = self.until.lock();
            if until.is_some_and(|u| Instant::now() < u) {
                continue;
            }
            self.running.store(false, Ordering::SeqCst);
            // Under the lock, so the next ask's `true` lands behind it.
            switch(false);
            self.counted.lock().take();
            drop(until);
            *self.latest.lock() = None;
            *self.encoded.lock() = None;
            debug!("nobody asked for the programme thumbnail lately, taking its branch off");
            return;
        }
    }

    /// Keep one frame from the appsink. On the branch's own streaming
    /// thread, behind its leaky queue, so this costs the programme nothing;
    /// it is a lock and a reference count all the same.
    fn keep(&self, sample: &gst::Sample) {
        let (Some(buffer), Some(caps)) = (sample.buffer_owned(), sample.caps()) else { return };
        let Ok(info) = gstreamer_video::VideoInfo::from_caps(caps) else { return };
        let frame = Frame { buffer, width: info.width(), height: info.height(), stride: info.stride()[0] as usize, at_ms: now_ms() };
        *self.latest.lock() = Some(frame);
        self.frames.fetch_add(1, Ordering::Relaxed);
    }

    /// The newest picture at `width` (16 to [`BRANCH_WIDTH`], even), or
    /// `None` while the first frame has not arrived within [`FIRST_WAIT`].
    pub async fn picture(self: &Arc<Self>, width: u32) -> Option<Thumb> {
        let width = width.clamp(16, BRANCH_WIDTH) & !1;
        let deadline = Instant::now() + FIRST_WAIT;
        let frame = loop {
            if let Some(f) = self.latest.lock().clone() {
                break f;
            }
            if Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        if let Some(t) = self.encoded.lock().clone().filter(|t| t.at_ms == frame.at_ms && t.width == width.min(frame.width)) {
            return Some(t);
        }
        let made = tokio::task::spawn_blocking(move || encode(&frame, width)).await.ok()?;
        match made {
            Ok(t) => {
                *self.encoded.lock() = Some(t.clone());
                Some(t)
            }
            Err(e) => {
                warn!(?e, "the programme thumbnail could not be encoded");
                None
            }
        }
    }
}

/// One JPEG from a kept RGB frame, scaled down to `width`.
fn encode(frame: &Frame, width: u32) -> Result<Thumb> {
    let map = frame.buffer.map_readable().context("mapping the programme frame")?;
    let row = frame.width as usize * 3;
    let mut rgb = Vec::with_capacity(row * frame.height as usize);
    for y in 0..frame.height as usize {
        let start = y * frame.stride;
        rgb.extend_from_slice(map.get(start..start + row).context("a programme frame shorter than its caps")?);
    }
    let img = image::RgbImage::from_raw(frame.width, frame.height, rgb).context("a programme frame of the wrong size")?;
    let img = snapshot::fit_width(img, Some(width));
    let (w, h) = img.dimensions();
    let jpeg = snapshot::encode_jpeg(&img).context("encoding the programme thumbnail")?;
    Ok(Thumb { jpeg: Arc::from(jpeg), width: w, height: h, at_ms: frame.at_ms })
}

/// The branch itself: built once with the programme, in its pipeline but
/// linked to nothing until something asks.
pub struct Branch {
    chain: Vec<gst::Element>,
    tee: gst::Element,
    pad: Option<gst::Pad>,
}

impl Branch {
    /// Build the chain in `pipeline` behind `bridge` (what brings a GPU frame
    /// to system memory, empty on the CPU), at the canvas's shape.
    pub fn build(
        pipeline: &gst::Pipeline,
        tee: &gst::Element,
        bridge: Vec<gst::Element>,
        canvas: (i32, i32),
        shared: &Arc<ProgrammeThumb>,
    ) -> Result<Self> {
        let rate = make("videorate", "pgm-t-rate")?;
        crate::probe::set_int(&rate, "max-rate", 1);
        crate::probe::set_bool(&rate, "drop-only", true);
        crate::probe::set_bool(&rate, "skip-to-first", true);
        let mut chain = vec![gstutil::queue_preview("pgm-t-q")?, rate];
        chain.extend(bridge);
        chain.push(make("videoscale", "pgm-t-scale")?);
        chain.push(make("videoconvert", "pgm-t-conv")?);
        let height = (i64::from(BRANCH_WIDTH) * i64::from(canvas.1.max(2)) / i64::from(canvas.0.max(2))).max(2) as i32 & !1;
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGB")
            .field("width", BRANCH_WIDTH as i32)
            .field("height", height)
            .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
            .build();
        chain.push(gstutil::capsfilter("pgm-t-caps", &caps)?);
        chain.push(sink(shared));
        pipeline.add_many(&chain).context("adding the programme thumbnail branch")?;
        gst::Element::link_many(&chain).context("linking the programme thumbnail branch")?;
        // Asleep until asked: no queue thread, no state to keep in step.
        for el in &chain {
            el.set_locked_state(true);
        }
        Ok(Self { chain, tee: tee.clone(), pad: None })
    }

    /// Whether the branch is on the tee.
    pub fn attached(&self) -> bool {
        self.pad.is_some()
    }

    /// Put the branch on the tee. On the mixer thread: states first, while
    /// nothing feeds the branch, then the link.
    pub fn attach(&mut self) -> Result<()> {
        if self.pad.is_some() {
            return Ok(());
        }
        let head = self.chain.first().context("the thumbnail branch has no head")?;
        let sink = head.static_pad("sink").context("the thumbnail branch head has no sink pad")?;
        for el in self.chain.iter().rev() {
            el.set_locked_state(false);
            el.sync_state_with_parent().context("starting the programme thumbnail branch")?;
        }
        let pad = self.tee.request_pad_simple("src_%u").context("the raw programme tee refused a pad for the thumbnail")?;
        pad.link(&sink).context("linking the programme thumbnail branch to the raw tee")?;
        self.pad = Some(pad);
        debug!("programme thumbnail branch attached");
        Ok(())
    }

    /// Take the branch off the tee and put it to sleep, so the programme fans
    /// out to whatever else is there and nothing more.
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
        debug!("programme thumbnail branch detached: nobody is asking");
    }
}

/// The appsink at the end: the newest frame only, never a clock wait, never a
/// preroll the pipeline would have to wait for.
fn sink(shared: &Arc<ProgrammeThumb>) -> gst::Element {
    let sink = gst_app::AppSink::builder().name("pgm-t-sink").max_buffers(1).drop(true).sync(false).build();
    sink.set_property("async", false);
    let shared = shared.clone();
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                shared.keep(&sample);
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    sink.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_with_padded_rows_encodes_at_the_asked_width() {
        gst::init().unwrap();
        // 322 wide RGB rows are 966 bytes, padded to 968 the way GStreamer
        // lays them out.
        let (w, h, stride) = (322u32, 180u32, 968usize);
        let mut data = vec![0u8; stride * h as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                data[y * stride + x * 3] = 200;
            }
        }
        let frame = Frame { buffer: gst::Buffer::from_mut_slice(data), width: w, height: h, stride, at_ms: 7 };
        let t = encode(&frame, 160).unwrap();
        assert_eq!((t.width, t.at_ms), (160, 7));
        assert_eq!(&t.jpeg[..2], &[0xff, 0xd8]);
        let back = snapshot::decode_jpeg(&t.jpeg).unwrap();
        assert!(back.get_pixel(80, 40).0[0] > 150, "the picture's red survives the padding");
    }

    #[tokio::test(start_paused = true)]
    async fn one_ask_switches_on_once_and_the_watch_switches_off_when_asks_stop() {
        let thumb = ProgrammeThumb::new();
        let clients = super::super::StreamClients::new();
        let said: Arc<Mutex<Vec<bool>>> = Arc::default();
        let switch: Switch = {
            let said = said.clone();
            Arc::new(move |on| said.lock().push(on))
        };
        thumb.want(switch.clone(), &clients);
        thumb.want(switch.clone(), &clients);
        assert_eq!(*said.lock(), vec![true]);
        assert_eq!(clients.count("thumbnail"), 1);
        tokio::time::sleep(ASKED_FOR / 2).await;
        thumb.want(switch.clone(), &clients);
        tokio::time::sleep(ASKED_FOR - Duration::from_secs(1)).await;
        assert_eq!(*said.lock(), vec![true], "an ask inside the window keeps it on");
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(*said.lock(), vec![true, false]);
        assert!(!thumb.running());
        assert_eq!(clients.count("thumbnail"), 0, "the gauge falls with the branch");
        thumb.want(switch, &clients);
        assert_eq!(*said.lock(), vec![true, false, true], "the next ask starts it again");
        assert_eq!(thumb.starts(), 2);
    }
}
