//! Instant replay and the replay buffer.
//!
//! Each angle (a source, or the programme) keeps its last N seconds as
//! encoded picture and sound in memory: a ring of whole GOPs (`ring`), filled
//! by a branch on the source's own tees (`angle`, `branch`). Memory is the
//! bitrate times the seconds, about 45 MB a minute for a 6 Mbit/s angle, so a
//! Raspberry Pi 4 holds several. Marks make events out of the rings
//! (`marks`), a replay source plays one back at any speed down to a tenth
//! (`plugin::kinds::replay`), and a save writes one to an MP4 without encoding
//! it again (`save`).
//!
//! Nothing here exists until replay is turned on. With it off the source
//! pipelines and the programme are exactly what they were, and turning it off
//! takes every branch out and frees every ring.
//!
//! The programme is never made to wait. Every branch starts with a leaky
//! queue, so an encoder that falls behind drops its own frames; the tee above
//! it never blocks. The only work on a streaming thread is the push into the
//! ring at the end of a branch, which is a short lock and a few pointer moves.

pub mod angle;
pub mod branch;
pub mod clip;
mod desk;
pub mod marks;
pub mod registry;
pub mod ring;
pub mod save;
pub mod shared;
mod status;

pub use desk::{Desk, SourceTaps, Taps};
pub use shared::{Shared, Window};

use godwinmix_protocol::replay::{EventRenameRequest, MarkRequest, ReplayConfig};

/// One call on the replay desk, as the mixer queue carries it.
#[derive(Debug)]
pub enum ReplayCall {
    Start(ReplayConfig),
    Stop,
    Get,
    Mark(MarkRequest),
    Rename(EventRenameRequest),
    Delete(String),
}

/// Why a replay call changed nothing, with what to do instead.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    NotFound { kind: &'static str, id: String, known: Vec<String> },
    Invalid { field: &'static str, message: String },
    NotInState(String),
    Failed(String),
}

impl Refusal {
    pub fn invalid(field: &'static str, message: String) -> Self {
        Self::Invalid { field, message }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { kind, id, known } => {
                let list = if known.is_empty() { "none".to_string() } else { known.join(", ") };
                write!(f, "there is no replay {kind} '{id}'. The {kind}s here: {list}")
            }
            Self::Invalid { message, .. } | Self::NotInState(message) | Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Refusal {}

#[cfg(test)]
mod tests;
