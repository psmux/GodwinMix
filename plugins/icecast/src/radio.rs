//! `icecast/source`: an internet radio stream, or any audio stream served
//! over HTTP, as a live source.
//!
//! ```text
//!   souphttpsrc (iradio-mode) ──► icydemux ──► parsebin ──► queue ──► matroskamux ──► fdsink fd=1
//! ```
//!
//! `file/source` would open the same address as a clip, expect it to end, and
//! drift against the programme clock. This reads it as the live stream it is,
//! takes the ICY song titles out of it for health, and hands the core the
//! sound as it came (MP3, AAC, Vorbis or Opus) for the core to decode once.

use std::sync::{Arc, Mutex};

use gmx_netkit::pipe::Pipe;
use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;

/// Where the Matroska goes: stdout in a running plugin, a file in a test.
#[derive(Debug, Clone)]
pub enum Sink {
    Stdout,
    #[allow(dead_code)]
    File(std::path::PathBuf),
}

pub struct Radio {
    pub pipe: Pipe,
    /// The last song title the station sent.
    pub title: Arc<Mutex<String>>,
}

fn make(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory).build().map_err(|_| format!("GStreamer has no {factory}. It comes from {}.", gmx_netkit::elements::where_from(factory)))
}

impl Radio {
    pub fn start(uri: &str, sink: Sink, reporter: Option<Reporter>) -> Result<Radio, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(&["souphttpsrc", "icydemux", "parsebin", "matroskamux"])?;
        let pipeline = gst::Pipeline::with_name("gmx-icecast-source");
        let src = make("souphttpsrc")?;
        src.set_property("location", uri);
        src.set_property("iradio-mode", true);
        src.set_property("is-live", true);
        src.set_property("do-timestamp", true);
        let (icy, parse, mux) = (make("icydemux")?, make("parsebin")?, make("matroskamux")?);
        mux.set_property("streamable", true);
        let out = match &sink {
            Sink::Stdout => {
                let o = make("fdsink")?;
                o.set_property("fd", 1i32);
                o
            }
            Sink::File(p) => {
                let o = make("filesink")?;
                o.set_property("location", p.to_string_lossy().to_string());
                o
            }
        };
        out.set_property("sync", false);
        pipeline.add_many([&src, &icy, &parse, &mux, &out]).map_err(|e| e.to_string())?;
        gst::Element::link_many([&src, &icy]).map_err(|e| e.to_string())?;
        mux.link(&out).map_err(|e| e.to_string())?;
        // icydemux adds its pad once it has seen the stream's first bytes, and
        // sends each song title down it as a tag event.
        let (p, title) = (parse.clone(), Arc::new(Mutex::new(String::new())));
        let t = title.clone();
        icy.connect_pad_added(move |_, pad| {
            let t = t.clone();
            pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
                if let Some(gst::EventView::Tag(tag)) = info.event().map(|e| e.view()) {
                    if let Some(v) = tag.tag().get::<gst::tags::Title>() {
                        *t.lock().unwrap_or_else(|e| e.into_inner()) = v.get().to_string();
                    }
                }
                gst::PadProbeReturn::Ok
            });
            let _ = p.static_pad("sink").map(|s| pad.link(&s));
        });
        link_parsed(&parse, &pipeline, &mux);
        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter)?;
        Ok(Radio { pipe, title })
    }
}

/// The first audio stream parsebin finds, through a queue into the muxer.
fn link_parsed(parse: &gst::Element, pipeline: &gst::Pipeline, mux: &gst::Element) {
    let (weak, mux) = (pipeline.downgrade(), mux.clone());
    parse.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let audio = caps.structure(0).is_some_and(|s| s.name().starts_with("audio"));
        let Ok(queue) = gst::ElementFactory::make("queue").build() else { return };
        let target = audio.then(|| mux.request_pad_simple("audio_%u")).flatten();
        let Some(target) = target else { return };
        if pipeline.add(&queue).is_err() || queue.link_pads(Some("src"), &mux, Some(target.name().as_str())).is_err() {
            return;
        }
        let _ = queue.sync_state_with_parent();
        let _ = pad.link(&queue.static_pad("sink").expect("a queue has a sink"));
    });
}
