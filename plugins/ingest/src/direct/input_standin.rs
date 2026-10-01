//! A stand in for the inputs (`input/`, the "directin" work) with the same
//! public names, so the host builds and its tests and measurements run
//! before that work merges. It takes bare MPEG-TS over UDP (unicast or
//! multicast) and `channel:` streams, nothing else. Delete it when `input/`
//! lands.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;

use super::{Input, Sink, StopSignal};
use crate::hub::{Hub, Recv};
use crate::media_tag::MediaTag;
use crate::rtmp::Inlet;
use crate::tagger;

pub mod spec {
    use serde_json::{json, Value};

    #[derive(Debug, Clone, PartialEq)]
    pub struct InputSpec {
        pub uri: String,
        pub program: Option<u16>,
        pub params: Value,
        pub backup: Option<Box<InputSpec>>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct InputError {
        pub message: String,
        pub data: Value,
    }

    impl InputSpec {
        pub fn new(uri: &str) -> InputSpec {
            InputSpec { uri: uri.to_string(), program: None, params: json!({}), backup: None }
        }

        pub fn from_json(v: &Value) -> Result<InputSpec, InputError> {
            let uri = v.get("uri").and_then(Value::as_str).map(str::trim).unwrap_or_default();
            if uri.is_empty() {
                return Err(InputError { message: "an input needs an address".into(), data: json!({"field": "uri"}) });
            }
            let backup = match v.get("backup") {
                None | Some(Value::Null) => None,
                Some(b) => Some(Box::new(InputSpec::from_json(b)?)),
            };
            Ok(InputSpec {
                uri: uri.to_string(),
                program: v.get("program").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()),
                params: v.get("params").cloned().filter(Value::is_object).unwrap_or_else(|| json!({})),
                backup,
            })
        }

        pub fn param(&self, key: &str) -> Option<String> {
            self.params.get(key).and_then(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        }
    }
}

pub mod stats {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub enum State {
        #[default]
        Connecting,
        Live,
        Retrying,
    }

    #[derive(Debug, Clone, Default, PartialEq)]
    pub struct InputStats {
        pub state: State,
        pub error: Option<String>,
        pub kbps: u32,
        pub fps: f64,
        pub width: u32,
        pub height: u32,
        pub video_codec: String,
        pub audio_codec: String,
        pub audio_channels: u32,
        pub cc_errors: u64,
        pub packets_lost: u64,
        pub keyframe_ms: Option<u64>,
        pub last_frame_ms: Option<u64>,
        pub program: Option<u16>,
    }

    impl InputStats {
        pub fn json(&self) -> serde_json::Value {
            serde_json::json!({
                "state": match self.state { State::Connecting => "connecting", State::Live => "live", State::Retrying => "retrying" },
                "kbps": self.kbps, "fps": self.fps, "width": self.width, "height": self.height,
                "video_codec": self.video_codec, "audio_codec": self.audio_codec,
                "audio_channels": self.audio_channels, "cc_errors": self.cc_errors,
                "packets_lost": self.packets_lost, "keyframe_ms": self.keyframe_ms,
                "last_frame_ms": self.last_frame_ms,
            })
        }
    }
}

pub use spec::{InputError, InputSpec};
pub use stats::InputStats;

/// What an input may need from the host it runs in.
#[derive(Clone, Default)]
pub struct Context {
    pub hub: Option<Hub>,
}

/// The input an address means.
pub fn open(spec: &InputSpec, ctx: &Context) -> Result<Box<dyn Input>, InputError> {
    if let (Some(name), Some(hub)) = (spec.uri.strip_prefix("channel:"), &ctx.hub) {
        let (app, stream) = name.split_once('/').unwrap_or((name, "main"));
        return Ok(Box::new(Channel { hub: hub.clone(), app: app.into(), stream: stream.into() }));
    }
    if spec.uri.starts_with("udp://") {
        return Ok(Box::new(Udp(spec.clone())));
    }
    Err(InputError { message: format!("this build takes udp:// and channel: inputs only, not {}", spec.uri), data: serde_json::json!({"field": "uri"}) })
}

struct Channel {
    hub: Hub,
    app: String,
    stream: String,
}

impl Input for Channel {
    fn run(self: Box<Self>, mut out: Sink, stop: StopSignal) {
        while !stop.is_stopped() {
            let reader = self.hub.subscribe(&self.app, &self.stream);
            loop {
                match reader.recv_timeout(Duration::from_millis(250)) {
                    Recv::Tag(t) => out.tag(t),
                    Recv::Ended => break,
                    Recv::Timeout if stop.is_stopped() => return,
                    Recv::Timeout => {}
                }
            }
        }
    }
}

struct Udp(InputSpec);

struct ToSink(Arc<Mutex<Sink>>);

impl Inlet for ToSink {
    fn tag(&mut self, tag: MediaTag) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).tag(tag);
    }
}

impl Input for Udp {
    fn run(self: Box<Self>, out: Sink, stop: StopSignal) {
        let sink = Arc::new(Mutex::new(out));
        let uri = self.0.uri.replace("udp://@", "udp://");
        let iface = self.0.param("interface").map(|i| format!(" multicast-iface={i}")).unwrap_or_default();
        // No queue after the demuxer: the whole input runs on udpsrc's one
        // streaming thread.
        let line = format!("udpsrc uri={uri}{iface} buffer-size=4194304 ! tsdemux name=d");
        let Ok(p) = gst::parse::launch(&line).map(|e| e.downcast::<gst::Pipeline>().expect("a pipeline")) else { return };
        let to = tagger::share(Box::new(ToSink(sink.clone())));
        let zero = Arc::new(tagger::Zero::default());
        let weak = p.downgrade();
        p.by_name("d").expect("demux").connect_pad_added(move |_, pad| {
            let Some(p) = weak.upgrade() else { return };
            let name = pad.current_caps().and_then(|c| c.structure(0).map(|s| s.name().to_string())).unwrap_or_default();
            let (parser, end) = match name.as_str() {
                "video/x-h264" => ("h264parse", tagger::video_sink(to.clone(), zero.clone())),
                "video/x-h265" => ("h265parse", tagger::hevc_sink(to.clone(), zero.clone())),
                "audio/mpeg" => ("aacparse", tagger::audio_sink(to.clone(), zero.clone())),
                _ => return,
            };
            let Ok(parse) = gst::ElementFactory::make(parser).build() else { return };
            let _ = p.add_many([&parse, &end]);
            let _ = parse.link(&end);
            let _ = parse.sync_state_with_parent();
            let _ = end.sync_state_with_parent();
            let _ = pad.link(&parse.static_pad("sink").expect("a sink pad"));
        });
        let _ = p.set_state(gst::State::Playing);
        while !stop.wait(Duration::from_secs(1)) {
            let s = stats::InputStats { state: stats::State::Live, ..Default::default() };
            sink.lock().unwrap_or_else(|e| e.into_inner()).stats(&s);
        }
        let _ = p.set_state(gst::State::Null);
    }
}
