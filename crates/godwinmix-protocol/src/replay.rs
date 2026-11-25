//! Instant replay: a ring of encoded pictures and sound per angle, marks that
//! make events out of it, playback as a source and saving to a file.
//!
//! `ReplayConfig` is both the config file's `[replay]` table and what
//! `replay.start` takes, so a replay set up by hand and one turned on from the
//! desk read back the same way.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod requests;
mod status;
pub use requests::*;
pub use status::*;

/// How much each angle keeps when nothing says: a minute.
pub const DEFAULT_SECONDS: u32 = 60;
/// The most an angle may keep: thirty minutes.
pub const MAX_SECONDS: u32 = 1800;
/// The angle that is the programme itself, read off the programme encoder.
pub const PROGRAMME: &str = "programme";
/// The source a replay plays on when the call names none.
pub const DEFAULT_SOURCE: &str = "replay";
/// The slowest playback speed: a tenth of real time.
pub const MIN_SPEED: f64 = 0.1;

fn default_seconds() -> u32 {
    DEFAULT_SECONDS
}

fn is_default_seconds(s: &u32) -> bool {
    *s == DEFAULT_SECONDS
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// The `[replay]` table. With no angles nothing is built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReplayConfig {
    /// Seconds each angle keeps, 1 to 1800. 60 when left out.
    #[serde(default = "default_seconds", skip_serializing_if = "is_default_seconds")]
    pub seconds: u32,
    /// The angles to record: source ids, and `programme` for the programme
    /// itself. Empty turns replay off.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub angles: Vec<String>,
    /// The bitrate a source angle is encoded at, in kbit/s. 0 or left out
    /// is the programme's own bitrate.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub bitrate_kbps: u32,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self { seconds: DEFAULT_SECONDS, angles: Vec::new(), bitrate_kbps: 0 }
    }
}

impl ReplayConfig {
    pub fn is_default(&self) -> bool {
        self == &ReplayConfig::default()
    }

    /// Whether this asks for anything to be recorded.
    pub fn is_on(&self) -> bool {
        !self.angles.is_empty()
    }
}

/// One event: a stretch of time between an in point and an out point, the
/// same stretch on every angle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReplayEvent {
    /// A slug the mixer gives it: `replay-1`, `replay-2`.
    pub id: String,
    /// What the desk shows. The id until it is renamed.
    pub name: String,
    /// The in point, in milliseconds of the mixer's clock.
    pub in_ms: u64,
    /// The out point. Absent while the event is still open (marked in and not
    /// out yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_ms: Option<u64>,
    /// The angle it was marked on, which playback starts from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<String>,
}

impl ReplayEvent {
    /// How long it runs, or `None` while it is open.
    pub fn duration_ms(&self) -> Option<u64> {
        self.out_ms.map(|o| o.saturating_sub(self.in_ms))
    }
}

#[cfg(test)]
mod tests;
