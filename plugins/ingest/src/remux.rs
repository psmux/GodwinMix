//! FLV in, Matroska out, with nothing decoded in between.
//!
//! ```text
//!   appsrc(FLV) ──► flvdemux ──┬─► h264parse ─┐
//!                              └─► aacparse ──┴─► matroskamux ──► fdsink fd=1
//! ```
//!
//! # Why not hand the core the FLV
//!
//! The core's container transport is `fdsrc ! decodebin`, and FLV is a
//! container `decodebin` opens, so sending the FLV straight through looked
//! right and cost nothing. It does not work on macOS: `flvdemux` feeding
//! `decodebin` makes it autoplug Apple's `vtdec_hw`, which negotiates GL backed
//! memory, and the core's normaliser works in system memory. The pipeline then
//! fails with `not-negotiated`, the source never produces a frame, and the log
//! says nothing that names the cause. The same stream in Matroska or MPEG-TS
//! decodes on the same machine with the same hardware decoder; it is FLV alone
//! that trips it. That was measured with `gst-launch-1.0` and no GodwinMix code
//! involved, so it is not something this plugin can fix by being cleverer.
//!
//! So the FLV is remuxed here. It costs two parsers and a muxer: no decode, no
//! encode, no copy of a picture. The tags go in and come out as the same
//! encoded frames in a container every platform's `decodebin` opens the same
//! way.
//!
//! The core could fix it instead, by putting the catalogue's `download` element
//! between `decodebin` and the normaliser on the sidecar container path the way
//! the built in `rtmp/source` does for its own decoder. That is the better
//! long term answer and it belongs in `crates/godwinmix-core/src/plugin/host/
//! source.rs`, not here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;

/// Where the Matroska goes.
pub enum Out {
    Stdout,
    /// Only the tests build this one: a test harness owns its own stdout and a
    /// megabyte of Matroska in the middle of the test output helps nobody.
    #[allow(dead_code)]
    File(std::path::PathBuf),
}

/// The remuxer. Feed it FLV bytes; it writes Matroska.
pub struct Remux {
    pipeline: gst::Pipeline,
    src: AppSrc,
    /// Set when the pipeline has posted an error, so a caller stops pushing
    /// into something that will never drain.
    broken: Arc<AtomicBool>,
    watch: Option<std::thread::JoinHandle<()>>,
}

impl Remux {
    pub fn open(out: Out) -> Result<Remux, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(&[
            "appsrc",
            "flvdemux",
            "h264parse",
            "aacparse",
            "matroskamux",
        ])?;

        let pipeline = gst::Pipeline::with_name("gmx-ingest-remux");
        let src = gst::ElementFactory::make("appsrc")
            .name("in")
            .build()
            .map_err(|e| format!("could not make appsrc: {e}"))?;
        let src: AppSrc = src
            .downcast()
            .map_err(|_| "appsrc is not an appsrc".to_string())?;
        // The caps are known, so `decodebin`'s typefinding is not needed and
        // the demuxer is built before the first tag arrives.
        src.set_caps(Some(&gst::Caps::builder("video/x-flv").build()));
        src.set_is_live(true);
        src.set_format(gst::Format::Bytes);
        src.set_do_timestamp(false);
        // Four megabytes of slack, then the push blocks. Blocking is the right
        // answer here: it becomes TCP backpressure to the publisher, which can
        // slow down, rather than a hole in the middle of the stream, which
        // nothing downstream can recover from.
        src.set_max_bytes(4 * 1024 * 1024);
        src.set_property("block", true);

        let demux = make("flvdemux", "demux")?;
        let mux = make("matroskamux", "mux")?;
        mux.set_property("streamable", true);
        let sink = match &out {
            Out::Stdout => {
                let s = make("fdsink", "out")?;
                s.set_property("fd", 1i32);
                s
            }
            Out::File(path) => {
                let s = make("filesink", "out")?;
                s.set_property("location", path.to_string_lossy().to_string());
                s
            }
        };
        for name in ["sync", "async"] {
            if sink.find_property(name).is_some() {
                sink.set_property(name, false);
            }
        }

        pipeline
            .add_many([src.upcast_ref(), &demux, &mux, &sink])
            .map_err(|e| format!("could not assemble the remuxer: {e}"))?;
        gst::Element::link(src.upcast_ref::<gst::Element>(), &demux)
            .map_err(|e| format!("could not link the FLV source to the demuxer: {e}"))?;
        gst::Element::link(&mux, &sink)
            .map_err(|e| format!("could not link the muxer to the pipe: {e}"))?;

        let weak = pipeline.downgrade();
        demux.connect_pad_added(move |_, pad| {
            let Some(pipeline) = weak.upgrade() else { return };
            let _ = branch(&pipeline, pad);
        });

        let broken = Arc::new(AtomicBool::new(false));
        let watch = spawn_watch(&pipeline, Arc::clone(&broken));
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| format!("the remuxer would not start: {e}"))?;
        Ok(Remux { pipeline, src, broken, watch })
    }

    /// Push FLV bytes. Blocks when the muxer is behind, which is backpressure
    /// rather than a hole in the stream.
    pub fn write(&self, bytes: &[u8]) {
        if self.broken.load(Ordering::Relaxed) {
            return;
        }
        let buffer = gst::Buffer::from_slice(bytes.to_vec());
        if self.src.push_buffer(buffer).is_err() {
            self.broken.store(true, Ordering::Relaxed);
        }
    }

    /// Has the pipeline failed? A caller that sees this should stop.
    pub fn broken(&self) -> bool {
        self.broken.load(Ordering::Relaxed)
    }
}

impl Drop for Remux {
    fn drop(&mut self) {
        // End of stream first, so the muxer finishes what it is holding rather
        // than the last second of the show being truncated.
        let _ = self.src.end_of_stream();
        let _ = self.pipeline.set_state(gst::State::Null);
        self.broken.store(true, Ordering::Relaxed);
        if let Some(thread) = self.watch.take() {
            let _ = thread.join();
        }
    }
}

/// Connect one demuxed stream to the muxer through its parser.
fn branch(pipeline: &gst::Pipeline, pad: &gst::Pad) -> Result<(), String> {
    let mux = pipeline.by_name("mux").ok_or("the remuxer has lost its muxer")?;
    let name = pad
        .current_caps()
        .and_then(|c| c.structure(0).map(|s| s.name().to_string()))
        .unwrap_or_default();
    let parser = if name.starts_with("video/x-h264") {
        "h264parse"
    } else if name.starts_with("audio/mpeg") {
        "aacparse"
    } else {
        // Something neither half of RTMP normally carries. Leaving it
        // unlinked is right: the muxer takes what it knows.
        return Ok(());
    };
    let queue = make("queue", "")?;
    let parse = make(parser, "")?;
    for element in [&queue, &parse] {
        pipeline
            .add(element)
            .map_err(|e| format!("could not add {parser}: {e}"))?;
        element.sync_state_with_parent().ok();
    }
    gst::Element::link(&queue, &parse)
        .map_err(|e| format!("could not link the queue to {parser}: {e}"))?;
    let sink_pad = queue.static_pad("sink").ok_or("the queue has no sink pad")?;
    pad.link(&sink_pad)
        .map_err(|e| format!("could not link a demuxed pad: {e}"))?;
    parse
        .link(&mux)
        .map_err(|e| format!("could not link {parser} into the muxer: {e}"))
}

/// Pop the bus on its own thread and record a failure.
fn spawn_watch(
    pipeline: &gst::Pipeline,
    broken: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    let bus = pipeline.bus()?;
    std::thread::Builder::new()
        .name("gmx-ingest-remux-bus".into())
        .spawn(move || {
            let tick = gst::ClockTime::from_mseconds(250);
            loop {
                if broken.load(Ordering::Relaxed) {
                    return;
                }
                let Some(message) = bus.timed_pop(Some(tick)) else {
                    continue;
                };
                if let gst::MessageView::Error(_) = message.view() {
                    broken.store(true, Ordering::Relaxed);
                    return;
                }
            }
        })
        .ok()
}

fn make(factory: &str, name: &str) -> Result<gst::Element, String> {
    let builder = gst::ElementFactory::make(factory);
    let builder = if name.is_empty() { builder } else { builder.name(name) };
    builder
        .build()
        .map_err(|e| format!("could not make '{factory}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remuxer_opens_and_takes_bytes_without_a_publisher() {
        let path = std::env::temp_dir().join(format!("gmx-remux-{}.mkv", std::process::id()));
        let remux = Remux::open(Out::File(path.clone())).expect("the remuxer assembles");
        remux.write(&crate::flv::header());
        assert!(!remux.broken());
        drop(remux);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rubbish_that_is_not_flv_produces_no_stream_rather_than_a_broken_one() {
        let path = std::env::temp_dir().join(format!("gmx-remux-bad-{}.mkv", std::process::id()));
        let remux = Remux::open(Out::File(path.clone())).expect("the remuxer assembles");
        for _ in 0..20 {
            remux.write(b"this is not an FLV stream at all, not even close");
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(remux);
        // `flvdemux` finds no tags it understands, so no pad appears and the
        // muxer is never given a stream. Nothing is written, which is the
        // honest outcome: the core sees a source that produces no frames and
        // says so, rather than a container with nothing decodable inside it.
        let written = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        assert_eq!(written, 0, "{written} bytes came out of a stream that is not FLV");
        let _ = std::fs::remove_file(&path);
    }
}
