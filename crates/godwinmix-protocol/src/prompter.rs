//! The teleprompter on the wire: scripts kept on the station, the one on the
//! prompter now, and where every prompter screen is in it.
//!
//! Every screen scrolls from the same three numbers: a position, the moment
//! it was true (`at_ms`, the mixer's clock) and a speed. A screen works out
//! where it should be from those and its own clock, so nothing is sent while
//! the prompter scrolls and two screens agree to well within a line. The
//! position is in rows, and the rows are made here (`rows.rs`) at one width
//! in characters for every screen, so a row on a phone is the same words as a
//! row on the monitor behind the glass.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod requests;
mod rows;

pub use requests::*;
pub use rows::{markers, rows, Row};

/// The largest script kept, in bytes of text. About two hours read aloud.
pub const MAX_SCRIPT_BYTES: usize = 512 * 1024;
/// The fastest the prompter moves, either way, in rows a second.
pub const MAX_SPEED: f64 = 20.0;
/// The speed a script starts at: about 130 words a minute at 32 characters a row.
pub const DEFAULT_SPEED: f64 = 0.4;
/// The fewest and the most characters a row holds.
pub const LINE_CHARS: std::ops::RangeInclusive<u32> = 12..=80;
pub const DEFAULT_LINE_CHARS: u32 = 32;
/// The widest margin, as a percentage of the screen's width on each side.
pub const MAX_MARGIN: f64 = 30.0;

/// How the text is laid out on every screen.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Look {
    /// The text size, as how many characters a row holds. Fewer is bigger.
    /// Every screen fits this many across its own width, so all of them break
    /// the rows in the same places. 12 to 80; 32 to start.
    pub line_chars: u32,
    /// Space left on each side, as a percentage of the screen's width. 0 to 30.
    pub margin: f64,
    /// Flip the text left to right, for a screen read through beam splitter
    /// glass. A screen may override it for itself.
    pub mirror: bool,
    /// Where the cue line sits, as a fraction of the screen's height from the
    /// top. The row at `position` is drawn on it. 0.1 to 0.9.
    pub cue: f64,
}

impl Default for Look {
    fn default() -> Look {
        Look { line_chars: DEFAULT_LINE_CHARS, margin: 8.0, mirror: false, cue: 0.33 }
    }
}

/// A place in a script a person can jump to: a line that starts with `#`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Marker {
    /// The words after the `#`, or `marker <n>` for a bare `#`.
    pub name: String,
    /// The row it starts on.
    pub row: u32,
}

/// `prompter.get`, and the payload of `event/prompter.changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PrompterState {
    /// The script on the prompter, or null when none is loaded.
    pub script: Option<String>,
    /// Its title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// When its text last changed, in milliseconds since the Unix epoch. A
    /// screen asks `prompter.rows` again when this or `line_chars` moves.
    pub updated_ms: u64,
    /// How many rows it makes at `line_chars`.
    pub rows: u32,
    pub markers: Vec<Marker>,
    /// Where the prompter was at `at_ms`, in rows from the top. 2.5 is half
    /// way through the third row.
    pub position: f64,
    /// When `position` was true, on the mixer's clock.
    pub at_ms: u64,
    /// Rows a second. Negative scrolls back. It applies only while `playing`.
    pub speed: f64,
    pub playing: bool,
    #[serde(flatten)]
    pub look: Look,
    /// The mixer's clock as this was written, so a screen can tell how far
    /// its own clock is from it.
    pub now_ms: u64,
    /// Moves on every change, so a screen can drop an answer older than the
    /// last event it applied.
    pub revision: u64,
}

impl PrompterState {
    /// Where the prompter is at `now_ms`: the position moved on by the speed
    /// while it plays, and never before the top or past the last row.
    pub fn position_at(&self, now_ms: u64) -> f64 {
        let moved = if self.playing { self.speed * now_ms.saturating_sub(self.at_ms) as f64 / 1000.0 } else { 0.0 };
        (self.position + moved).clamp(0.0, self.rows as f64)
    }
}

/// One script, as `prompter.script.get` and `prompter.script.save` answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Script {
    /// A slug: `evening-news`.
    pub id: String,
    pub title: String,
    /// Plain text. A line starting with `#` is a marker, `*word*` is drawn in
    /// italics and `**word**` in bold.
    pub text: String,
    /// When it was last saved, in milliseconds since the Unix epoch.
    pub updated_ms: u64,
}

/// One script in `prompter.script.list`, without its text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptSummary {
    pub id: String,
    pub title: String,
    /// Lines as written, blank ones included.
    pub lines: u32,
    /// The names of its markers, in order.
    pub markers: Vec<String>,
    pub bytes: u32,
    pub updated_ms: u64,
}

/// `prompter.script.list`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptList {
    /// Every script, by id.
    pub scripts: Vec<ScriptSummary>,
    /// The one on the prompter now.
    pub on: Option<String>,
}

/// `prompter.rows`: the script on the prompter, broken into rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RowsAnswer {
    pub script: Option<String>,
    pub updated_ms: u64,
    /// The width the rows were made at.
    pub line_chars: u32,
    pub rows: Vec<Row>,
}

/// `event/prompter.script.changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptChanged {
    pub id: String,
    /// True when the script was removed.
    pub removed: bool,
    pub updated_ms: u64,
}

/// The prompter events.
pub fn events() -> Vec<crate::protocol::EventDef> {
    vec![
        crate::protocol::EventDef {
            name: "prompter.changed",
            since: "1",
            summary: "The prompter moved: a script was loaded, it was played, paused, sped up, \
                      sent to a marker, or its look changed. Carries the whole state. Nothing is \
                      sent while it scrolls; a screen works out where it is from position, \
                      at_ms and speed.",
            ext: None,
            legacy: None,
            payload: crate::method::schema_of::<PrompterState>,
        },
        crate::protocol::EventDef {
            name: "prompter.script.changed",
            since: "1",
            summary: "A script was saved or removed. The text is not carried: ask \
                      prompter.script.get, or prompter.rows for the one on the prompter.",
            ext: None,
            legacy: None,
            payload: crate::method::schema_of::<ScriptChanged>,
        },
    ]
}

#[cfg(test)]
mod tests;
