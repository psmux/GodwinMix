//! Shows: several independent programmes on one machine.
//!
//! The process a person starts is the station. It owns the control port, the
//! page, the ingest hub and the governor, and runs each show as a process of
//! its own (`dev/plans/wave3-contract.md`). These are the shapes of the
//! `show.*` methods the station answers, and of the two events it sends every
//! client. Everything else a client calls addresses one show, chosen with
//! `?show=<id>` or the `show` field of `core.subscribe`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod bulk;
mod events;
mod props;
mod requests;
mod stats;

pub use bulk::*;
pub use events::*;
pub use props::*;
pub use requests::*;
pub use stats::*;

use crate::destination::Destination;

fn yes() -> bool {
    true
}

/// Where a show is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ShowState {
    /// Its process is starting and has not said it is ready yet.
    Starting,
    /// Up, and answering on its own socket.
    Running,
    /// Stopped by a person. Its config is kept and `show.start` brings it back.
    Stopped,
    /// It kept dying and the station stopped restarting it. `error` says why.
    Failed,
}

/// One show, as `show.list` and `event/show.changed` carry it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Show {
    /// A slug: `main`, `second-room`.
    pub id: String,
    pub name: String,
    pub state: ShowState,
    /// The scene, or the source, on its programme. None while it shows the
    /// slate or is not running.
    pub on_air: Option<String>,
    /// What its outputs are sending, summed, in kilobits a second.
    pub programme_kbps: u64,
    /// Its process's CPU, in thousandths of one core, measured between two
    /// reads of `show.list`. Zero on the first read and while it is stopped.
    pub cpu_millicores: u32,
    /// Its process's resident memory, in MiB.
    #[serde(default)]
    pub memory_mib: u64,
    /// How many times the station has started it again after it died.
    #[serde(default)]
    pub restarts: u32,
    /// Why it is not running, when it is not and a person did not ask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// true: scenes, transitions and a programme encode, in a process of
    /// its own. false: one input straight to its outputs, in the shared
    /// direct host, with no compositor.
    #[serde(default = "yes")]
    pub compositing: bool,
    /// What it takes in. A show that composites makes it its one source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    /// The outputs of a show without compositing. A show that composites
    /// keeps its outputs inside it, under `output.*` with `?show=<id>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<Destination>,
    #[serde(default)]
    pub health: Health,
}

/// `show.list`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowList {
    pub shows: Vec<Show>,
    /// The show a client reaches when it names none: the first one, which is
    /// the one the station was started with.
    pub current: String,
}

/// What a new show starts from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ShowFrom {
    /// `"empty"`, or the id of a show to copy.
    Named(String),
    /// A file `project.export` wrote, opened into the new show.
    Project { project: Value },
}

/// `show.add`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowAddRequest {
    /// What a person calls it. The id is made from it.
    pub name: String,
    /// `"empty"` (the default), the id of a show to copy, or
    /// `{project: <file>}`. A copy takes the show's settings, sources and
    /// scenes, and leaves its outputs behind so nothing goes out twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<ShowFrom>,
    /// Left out: true, a show with scenes and a programme, as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compositing: Option<bool>,
    /// What it takes in. Needed by a show without compositing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    /// Where a show without compositing sends its input.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ShowOutputSpec>,
}

impl From<ShowAdd> for ShowAddRequest {
    fn from(a: ShowAdd) -> Self {
        ShowAddRequest { name: a.name, from: a.from, compositing: a.compositing, input: a.input, outputs: a.outputs }
    }
}

/// `show.rename`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowRenameRequest {
    pub id: String,
    pub name: String,
}

/// `show.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowRemoved {
    pub removed: String,
}

#[cfg(test)]
mod tests;
