//! Why a set of requests cannot be planned, and the nearest thing that can.
//! Every message says what was asked and what to do instead; `data()` carries
//! the same facts for a page to build a button from.

use std::fmt;

use godwinmix_protocol::rendition::{Fps, VideoShape};
use serde::Serialize;

use crate::container::{shape_text, video_name};

/// A shape this machine can make instead of the one asked for.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Suggestion {
    /// The codec slug, `h264`.
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: Fps,
    /// The encoder that would make it.
    pub encoder: String,
    /// "H.264 1920x1080 at 60 fps with h264-software-x264".
    pub text: String,
}

/// One encoder the planner tried and passed over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skip {
    pub encoder: String,
    /// "h265-nvidia cannot make 7680x4320 at 60 fps", "the GPU nvidia0 is full".
    pub why: String,
}

/// The facts of a `no-encoder` refusal, boxed to keep `PlanError` small.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoEncoder {
    pub request: String,
    /// The codec slug asked for.
    pub codec: String,
    pub shape: VideoShape,
    /// Every encoder of that codec, and why each was passed over.
    pub tried: Vec<Skip>,
    pub nearest: Option<Suggestion>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "kebab-case")]
pub enum PlanError {
    /// A request names a source the plan was not given.
    UnknownSource {
        request: String,
        source: String,
        known: Vec<String>,
    },
    /// Two requests share one id.
    DuplicateRequest { request: String },
    /// The request asks for a codec its container cannot carry.
    ContainerCodec {
        request: String,
        container: String,
        codec: String,
        allowed: Vec<String>,
    },
    /// The request wants a track its source does not have.
    MissingTrack {
        request: String,
        source: String,
        track: String,
    },
    /// The request drops both video and audio.
    NothingAsked { request: String },
    /// No video encoder on this machine can make the shape.
    NoEncoder(Box<NoEncoder>),
    /// No audio encoder on this machine for the codec.
    NoAudioEncoder {
        request: String,
        codec: String,
        nearest: Option<String>,
    },
}

impl PlanError {
    /// The slug a client matches on.
    pub fn code(&self) -> &'static str {
        match self {
            PlanError::UnknownSource { .. } => "unknown-source",
            PlanError::DuplicateRequest { .. } => "duplicate-request",
            PlanError::ContainerCodec { .. } => "container-codec",
            PlanError::MissingTrack { .. } => "missing-track",
            PlanError::NothingAsked { .. } => "nothing-asked",
            PlanError::NoEncoder(_) => "no-encoder",
            PlanError::NoAudioEncoder { .. } => "no-audio-encoder",
        }
    }

    /// The facts as an object, with `code` and `message` in it.
    pub fn data(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(map) = value.as_object_mut() {
            map.insert("message".into(), self.to_string().into());
        }
        value
    }
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::UnknownSource {
                request,
                source,
                known,
            } => write!(
                f,
                "Output `{request}` reads source `{source}`, which is not running. \
                 Pick one of: {}.",
                list(known)
            ),
            PlanError::DuplicateRequest { request } => {
                write!(f, "Two outputs are called `{request}`. Rename one of them.")
            }
            PlanError::ContainerCodec {
                request,
                container,
                codec,
                allowed,
            } => write!(
                f,
                "Output `{request}` asks for {codec} in {container}, which cannot carry it. \
                 Pick one of {} or another container.",
                list(allowed)
            ),
            PlanError::MissingTrack {
                request,
                source,
                track,
            } => write!(
                f,
                "Output `{request}` asks for {track} but source `{source}` has none. \
                 Turn {track} off for this output, or pick a source that has it."
            ),
            PlanError::NothingAsked { request } => write!(
                f,
                "Output `{request}` drops both video and audio, so there is nothing to send. \
                 Keep one of them."
            ),
            PlanError::NoEncoder(e) => {
                let NoEncoder {
                    request,
                    shape,
                    tried,
                    nearest,
                    ..
                } = e.as_ref();
                let codec = video_name(shape.codec);
                write!(
                    f,
                    "No encoder on this machine can make {codec} {} for `{request}`",
                    shape_text(shape)
                )?;
                if !tried.is_empty() {
                    let why: Vec<&str> = tried.iter().map(|s| s.why.as_str()).collect();
                    write!(f, " ({})", why.join("; "))?;
                }
                match nearest {
                    Some(s) => write!(f, ". {} is possible; ask for that instead.", s.text),
                    None => write!(f, ". Ask for a smaller picture or another codec."),
                }
            }
            PlanError::NoAudioEncoder {
                request,
                codec,
                nearest,
            } => {
                write!(f, "This machine has no {codec} encoder for `{request}`")?;
                match nearest {
                    Some(alt) => write!(f, ". {alt} is possible in this container; ask for that instead."),
                    None => write!(f, ", and none for any codec this container carries. Turn audio off for this output."),
                }
            }
        }
    }
}

impl std::error::Error for PlanError {}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        return String::from("(none)");
    }
    items.join(", ")
}
