//! Sending to an SRT receiver: the tags become MPEG-TS, still undecoded.
//!
//! ```text
//!   appsrc(FLV) ──► flvdemux ──┬─► h264parse or h265parse ─┐
//!                              └─► aacparse ──────────────────┴─► mpegtsmux ──► srtsink
//! ```
//!
//! This is the one place the restreamer uses GStreamer, and it is there for
//! the transport and not the media. SRT is libsrt, which only GStreamer brings
//! into this process; writing MPEG-TS by hand would still leave nothing to
//! carry it. The parsers turn the AVC and AAC framing FLV uses into the
//! Annex B and ADTS framing MPEG-TS wants. No decoder, no encoder. The video
//! parser is chosen by the stream's caps, so an enhanced RTMP publisher's HEVC
//! crosses as HEVC; flvdemux does not read enhanced RTMP's AV1 yet.

use gmx_netkit::pipe::Pipe;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;

use crate::flv;
use crate::media_tag::{MediaTag, TagKind};

use super::link::{Failure, Link};
use super::target::Target;
use super::ts_video::video_by_codec;

/// What the pipeline may hold before the far end counts as slow. Past it a
/// tag is refused and the sender drops to the next keyframe.
const SLACK: u64 = 2 * 1024 * 1024;

pub struct SrtLink {
    pipe: Pipe,
    src: AppSrc,
    name: String,
    /// Past the slack: dropping until the next keyframe.
    skipping: bool,
}

impl SrtLink {
    pub fn dial(target: &Target) -> Result<SrtLink, Failure> {
        let name = target.name();
        gmx_netkit::init().map_err(Failure::Refused)?;
        gmx_netkit::elements::require(&["appsrc", "flvdemux", "h264parse", "h265parse", "aacparse", "mpegtsmux", "srtsink"])
            .map_err(Failure::Refused)?;
        let uri = target.url.replace('"', "");
        let description = format!(
            "appsrc name=in is-live=true format=bytes caps=video/x-flv \
             ! flvdemux name=d \
             d.audio ! queue ! aacparse ! mux. \
             mpegtsmux name=mux alignment=7 ! srtsink uri=\"{uri}\" wait-for-connection=false sync=false"
        );
        let mut pipe = Pipe::launch(&description).map_err(Failure::Refused)?;
        video_by_codec(pipe.pipeline());
        let src: AppSrc = pipe
            .by_name("in")
            .and_then(|e| e.downcast().ok())
            .ok_or_else(|| Failure::Refused("the SRT pipeline has no appsrc".into()))?;
        pipe.play(None).map_err(|e| Failure::Lost(format!("{name}: {e}")))?;
        let link = SrtLink { pipe, src, name, skipping: false };
        // A caller that cannot reach its listener fails on the bus within a
        // moment of starting. Waiting that moment is what lets "nothing
        // answered" be said now rather than after the first GOP is lost.
        std::thread::sleep(std::time::Duration::from_millis(300));
        link.check()?;
        link.push(flv::header())?;
        Ok(link)
    }

    fn check(&self) -> Result<(), Failure> {
        match self.pipe.failure() {
            Some(e) if e.contains("onnect") || e.contains("resolve") => {
                Err(Failure::Lost(format!("nothing answered at {} ({e})", self.name)))
            }
            Some(e) => Err(Failure::Lost(format!("{} failed: {e}", self.name))),
            None => Ok(()),
        }
    }

    fn push(&self, bytes: Vec<u8>) -> Result<usize, Failure> {
        let n = bytes.len();
        self.src
            .push_buffer(gst::Buffer::from_mut_slice(bytes))
            .map_err(|e| Failure::Lost(format!("{} stopped taking the stream ({e:?})", self.name)))?;
        Ok(n)
    }
}

impl Link for SrtLink {
    fn send(&mut self, tag: &MediaTag, timestamp_ms: u32) -> Result<usize, Failure> {
        self.check()?;
        if self.src.current_level_bytes() > SLACK {
            self.skipping = true;
        }
        if self.skipping && tag.kind != TagKind::Script && !tag.sequence_header {
            if !super::queue::starts_gop(tag) {
                return Ok(0);
            }
            self.skipping = false;
        }
        match tag.kind {
            TagKind::Video => self.push(flv::video(timestamp_ms, &tag.payload)),
            TagKind::Audio => self.push(flv::audio(timestamp_ms, &tag.payload)),
            // MPEG-TS has nowhere to put onMetaData, and nothing reads it.
            TagKind::Script => Ok(0),
        }
    }

    fn poll(&mut self) -> Result<(), Failure> {
        self.check()
    }

    /// End the stream and let what the muxer and libsrt still hold go out.
    fn close(&mut self) {
        let _ = self.src.end_of_stream();
        let watch = self.pipe.watch();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !watch.ended() && watch.failure().is_none() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // EOS has reached srtsink, but libsrt still holds what it has not
        // delivered, and in live mode it drops that on close. Measured: the
        // last 1.2 s of a 15 s run never arrived without this wait, and all
        // of it did with it. It is the sender's own thread at the end of a
        // stream, so nothing waits on it.
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = self.pipe.pipeline().set_state(gst::State::Null);
    }
}

