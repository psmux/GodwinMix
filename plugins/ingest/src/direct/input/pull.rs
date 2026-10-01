//! The inputs that pull: HLS and DASH, someone else's RTMP server, and a
//! file played in a loop. RTSP is `rtsp.rs`.
//!
//! ```text
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
#[path = "pull_tests.rs"]
mod tests;
