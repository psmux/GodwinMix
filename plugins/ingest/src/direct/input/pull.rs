//! The inputs that pull: RTSP from a camera or an encoder, HLS and DASH,
//! someone else's RTMP server, and a file played in a loop.
//!
//! ```text
//!   rtspsrc ──(each RTP stream)──► parsebin (depayloads) ──► tags
//!   urisourcebin (hls, dash, rtmp) ──► parsebin ──► tags
//!   filesrc ──► parsebin ──► tags, at the clock's pace, again from the top at the end
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;

use super::loss::Loss;
use super::pads::{make, parse_into, Pads};
use super::runner::Plan;
use super::spec::{InputError, InputSpec};

pub struct Rtsp {
    uri: String,
    /// `tcp`, `udp`, or empty to let rtspsrc try UDP first.
    transport: String,
    latency_ms: u32,
}

impl Rtsp {
    pub fn new(spec: &InputSpec) -> Result<Rtsp, InputError> {
        let transport = spec.param("transport").unwrap_or_default().to_ascii_lowercase();
        if !["", "tcp", "udp", "auto"].contains(&transport.as_str()) {
            return Err(InputError::new(
                format!("RTSP transport '{transport}' is not one of tcp, udp or auto. Choose tcp for a camera across a firewall or NAT."),
                json!({"field": "params.transport", "got": transport, "allowed": ["tcp", "udp", "auto"]}),
            ));
        }
        let latency_ms = spec.number("latency_ms").unwrap_or(200).min(10_000) as u32;
        Ok(Rtsp { uri: spec.uri.clone(), transport, latency_ms })
    }
}

impl Plan for Rtsp {
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String> {
        let src = make("rtspsrc")?;
        src.set_property("location", &self.uri);
        src.set_property("latency", self.latency_ms);
        match self.transport.as_str() {
            "tcp" => src.set_property_from_str("protocols", "tcp"),
            "udp" => src.set_property_from_str("protocols", "udp"),
            _ => {}
        }
        let manager = Arc::new(std::sync::Mutex::new(None::<gst::Element>));
        let keep = manager.clone();
        src.connect("new-manager", false, move |args| {
            *keep.lock().unwrap_or_else(|e| e.into_inner()) = args.get(1).and_then(|v| v.get::<gst::Element>().ok());
            None
        });
        let (weak, shared) = (pipeline.downgrade(), pads.clone());
        src.connect_pad_added(move |_, pad| {
            if let Some(pipeline) = weak.upgrade() {
                if let Err(e) = parse_into(&pipeline, pad, &shared) {
                    shared.note.lock().unwrap_or_else(|e| e.into_inner()).push(e);
                }
            }
        });
        pipeline.add(&src).map_err(|e| e.to_string())?;
        Ok(Loss::none().with_poll(move || manager.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(0, rtp_lost)))
    }

    fn address(&self) -> String {
        redact(&self.uri)
    }
}

/// RTP packets the camera sent that never came, from each session's
/// statistics of the sources it heard from.
fn rtp_lost(rtpbin: &gst::Element) -> u64 {
    (0u32..4)
        .filter_map(|i| rtpbin.emit_by_name::<Option<glib::Object>>("get-session", &[&i]))
        .map(|session| {
            let stats = session.property::<gst::Structure>("stats");
            let sources = stats.get::<glib::ValueArray>("source-stats").ok();
            sources
                .iter()
                .flat_map(|a| a.iter())
                .filter_map(|v| v.get::<gst::Structure>().ok())
                .filter(|s| !s.get::<bool>("internal").unwrap_or(true))
                .filter_map(|s| s.get::<i32>("packets-lost").ok())
                .map(|n| n.max(0) as u64)
                .sum::<u64>()
        })
        .sum()
}

/// HLS, DASH and RTMP, through `urisourcebin`.
pub struct Uri {
    uri: String,
    /// HLS and DASH hand over a segment at a time; pacing them to the clock
    /// keeps the tags as steady as the stream.
    paced: bool,
}

impl Uri {
    pub fn new(spec: &InputSpec, paced: bool) -> Uri {
        Uri { uri: spec.uri.clone(), paced }
    }
}

impl Plan for Uri {
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String> {
        let src = make("urisourcebin")?;
        src.set_property("uri", &self.uri);
        let (weak, shared) = (pipeline.downgrade(), pads.clone());
        src.connect_pad_added(move |_, pad| {
            if let Some(pipeline) = weak.upgrade() {
                if let Err(e) = parse_into(&pipeline, pad, &shared) {
                    shared.note.lock().unwrap_or_else(|e| e.into_inner()).push(e);
                }
            }
        });
        pipeline.add(&src).map_err(|e| e.to_string())?;
        Ok(Loss::none())
    }

    fn address(&self) -> String {
        redact(&self.uri)
    }

    fn paced(&self) -> bool {
        self.paced
    }
}

/// A file on this machine, played at the clock's pace and looped.
pub struct File {
    path: PathBuf,
    program: Option<u16>,
}

impl File {
    pub fn new(spec: &InputSpec) -> Result<File, InputError> {
        let path = if spec.uri.starts_with('/') {
            PathBuf::from(&spec.uri)
        } else {
            glib::filename_from_uri(&spec.uri).map(|(p, _)| p).map_err(|e| {
                InputError::new(format!("'{}' is not a file address ({e}). Write file:///path/to/clip.ts.", spec.uri), json!({"field": "uri", "got": spec.uri}))
            })?
        };
        if !path.is_file() {
            return Err(InputError::new(
                format!("there is no file at {}. Copy the clip to this machine, or give the whole path.", path.display()),
                json!({"field": "uri", "path": path.display().to_string()}),
            ));
        }
        Ok(File { path, program: spec.program })
    }
}

impl Plan for File {
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String> {
        let src = make("filesrc")?;
        src.set_property("location", self.path.to_string_lossy().to_string());
        pipeline.add(&src).map_err(|e| e.to_string())?;
        parse_into(pipeline, &src.static_pad("src").ok_or("filesrc has no src pad")?, pads)?;
        Ok(Loss::none())
    }

    fn address(&self) -> String {
        self.path.display().to_string()
    }

    fn program(&self) -> Option<u16> {
        self.program
    }

    fn looped(&self) -> bool {
        true
    }
}

/// An address with its password taken out, for messages.
pub fn redact(uri: &str) -> String {
    let Some((scheme, rest)) = uri.split_once("://") else { return uri.to_string() };
    let (auth, tail) = rest.split_once('/').map_or((rest, ""), |(a, t)| (a, t));
    match auth.rsplit_once('@') {
        Some((login, host)) => {
            let user = login.split(':').next().unwrap_or("");
            let slash = if rest.contains('/') { "/" } else { "" };
            format!("{scheme}://{user}:***@{host}{slash}{tail}")
        }
        None => uri.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_camera_password_never_reaches_a_message() {
        assert_eq!(redact("rtsp://admin:hunter2@10.0.0.9/stream1"), "rtsp://admin:***@10.0.0.9/stream1");
        assert_eq!(redact("rtsp://10.0.0.9/stream1"), "rtsp://10.0.0.9/stream1");
    }

    #[test]
    fn a_missing_file_and_a_bad_transport_say_what_to_do() {
        let err = File::new(&InputSpec::new("file:///no/such/clip.ts")).err().unwrap();
        assert!(err.message.contains("/no/such/clip.ts"), "{err}");
        let spec = InputSpec { params: json!({"transport": "quic"}), ..InputSpec::new("rtsp://cam/s") };
        assert_eq!(Rtsp::new(&spec).err().unwrap().data["field"], "params.transport");
    }
}
