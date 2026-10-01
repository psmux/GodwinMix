//! The frame tap: one show's stream decoded, only while someone asked.
//!
//! ```text
//!   hub reader ──► tap thread ──► Input ──► appsrc ─ decodebin ─ videoconvert ─ I420 ─ appsink ──► FrameSink::video
//!                                       └─► appsrc ─ decodebin ─ audioconvert ─ S16 ── appsink ──► FrameSink::audio
//! ```
//!
//! The tags are handed to the decoders by `crate::transcode`'s own input,
//! which sets the caps from the sequence headers and holds frames back to
//! the first keyframe. With `keyframes_only` only keyframes go in, which is
//! a decode of about one frame a second; with `audio_every` above one, one
//! sound frame in that many. The appsinks drop rather than queue, and the
//! hub reader drops whole GOPs when the tap thread falls behind.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gmx_netkit::pipe::Pipe;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use super::tap::{FrameSink, FrameTap, TapWant};
use crate::hub::{Hub, Recv};
use crate::media_tag::TagKind;
use crate::transcode::input::Input;

const PIPELINE: &str = "appsrc name=v format=time is-live=true do-timestamp=false ! decodebin ! videoconvert \
    ! video/x-raw,format=I420 ! appsink name=vs sync=false max-buffers=2 drop=true \
    appsrc name=a format=time is-live=true do-timestamp=false ! decodebin ! audioconvert \
    ! audio/x-raw,format=S16LE,layout=interleaved ! appsink name=as sync=false max-buffers=4 drop=true";

/// Start a tap on `app/main` of `hub`.
pub fn start(hub: &Hub, app: &str, want: TapWant, sink: Arc<dyn FrameSink>) -> FrameTap {
    let stop = Arc::new(AtomicBool::new(false));
    let (hub, app, halt) = (hub.clone(), app.to_string(), stop.clone());
    let _ = std::thread::Builder::new().name(format!("gmx-tap-{app}")).spawn(move || {
        while !halt.load(Ordering::Relaxed) {
            if let Err(e) = session(&hub, &app, want, &sink, &halt) {
                eprintln!("frame tap for {app}: {e}");
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    });
    FrameTap { stop }
}

/// One publisher session's worth of decoding.
fn session(hub: &Hub, app: &str, want: TapWant, sink: &Arc<dyn FrameSink>, stop: &AtomicBool) -> Result<(), String> {
    gmx_netkit::init()?;
    let reader = hub.subscribe(app, "main");
    let mut pipe = Pipe::launch(PIPELINE)?;
    let input = Input::default();
    let src = |name: &str| pipe.by_name(name).and_then(|e| e.downcast::<gst_app::AppSrc>().ok());
    let (v, a) = (src("v").ok_or("no video appsrc")?, src("a").ok_or("no audio appsrc")?);
    for s in [&v, &a] {
        s.set_property("max-bytes", 2_000_000u64);
        s.set_property_from_str("leaky-type", "downstream");
    }
    input.attach(TagKind::Video, Some(v));
    if want.audio {
        input.attach(TagKind::Audio, Some(a));
    }
    attach_sink(&pipe, "vs", sink.clone(), true);
    attach_sink(&pipe, "as", sink.clone(), false);
    pipe.play(None)?;
    let (mut base, mut nth) = (None, 0u32);
    while !stop.load(Ordering::Relaxed) {
        let tag = match reader.recv_timeout(Duration::from_millis(250)) {
            Recv::Tag(t) => t,
            Recv::Ended => break,
            Recv::Timeout => continue,
        };
        if !tag.sequence_header && tag.kind != TagKind::Script {
            let keep = match tag.kind {
                TagKind::Video => !want.keyframes_only || tag.keyframe,
                _ => {
                    nth = nth.wrapping_add(1);
                    want.audio && nth % want.audio_every.max(1) == 0
                }
            };
            if !keep {
                continue;
            }
        }
        let at = *base.get_or_insert(tag.timestamp_ms);
        input.push(&tag, at);
    }
    pipe.stop();
    Ok(())
}

fn attach_sink(pipe: &Pipe, name: &str, sink: Arc<dyn FrameSink>, video: bool) {
    let Some(appsink) = pipe.by_name(name).and_then(|e| e.downcast::<gst_app::AppSink>().ok()) else { return };
    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |s| {
                let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                if video {
                    sink.video(&sample);
                } else {
                    sink.audio(&sample);
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
}
