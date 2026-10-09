//! One connection to a destination, whatever carries it.

use crate::media_tag::MediaTag;

use super::target::{RtmpUrl, Target};

/// Why a link could not be made or was lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The far end said no, and asking again the same way will not change its
    /// mind: a key it does not know.
    Refused(String),
    /// The far end could not be reached or went away. Worth another try.
    Lost(String),
}

impl Failure {
    pub fn message(&self) -> &str {
        match self {
            Failure::Refused(m) | Failure::Lost(m) => m,
        }
    }
}

/// A live connection that takes tags.
pub trait Link: Send {
    /// Send one tag with its timestamp already rebased. Answers the bytes
    /// written, for the bitrate.
    fn send(&mut self, tag: &MediaTag, timestamp_ms: u32) -> Result<usize, Failure>;
    /// Read whatever the far end has said, without waiting for it. A server
    /// that is never read from stops reading from us.
    fn poll(&mut self) -> Result<(), Failure>;
    /// Say goodbye, where the carriage has a way to.
    fn close(&mut self);
    /// The file it writes, for a recording.
    fn file(&self) -> Option<&std::path::Path> {
        None
    }
}

/// Connect to a target, by the scheme of its address.
pub fn dial(target: &Target) -> Result<Box<dyn Link>, Failure> {
    let scheme = target.url.split_once("://").map(|(s, _)| s.to_ascii_lowercase());
    match scheme.as_deref() {
        Some("srt") => Ok(Box::new(super::srt_out::SrtLink::dial(target)?)),
        Some("udp" | "rtp") => Ok(Box::new(super::udp_out::UdpLink::dial(target)?)),
        Some("rist") => Ok(Box::new(super::rist_out::RistLink::dial(target)?)),
        Some("file") => Ok(Box::new(super::file_out::FileLink::dial(target)?)),
        _ => {
            let url = RtmpUrl::parse(&target.url).map_err(Failure::Refused)?;
            Ok(Box::new(super::rtmp_out::RtmpLink::dial(target, url)?))
        }
    }
}
