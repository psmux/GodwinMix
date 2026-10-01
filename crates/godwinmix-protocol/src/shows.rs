//! Shows: several independent programmes on one machine.
//!
//! The process a person starts is the station. It owns the control port, the
//! page, the ingest hub and the governor, and runs each show as a process of
//! its own (`dev/plans/wave3-contract.md`). These are the shapes of the
//! `show.*` methods the station answers, and of the two events it sends every
//! client. Everything else a client calls addresses one show, chosen with
//! `?show=<id>` or the `show` field of `core.subscribe`.

use crate::method::schema_of;
use crate::protocol::EventDef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod props;
mod requests;

pub use props::*;
pub use requests::*;

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

/// `event/show.changed`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowChanged {
    pub show: Show,
}

/// `event/show.removed`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowRemovedEvent {
    pub id: String,
}

/// The two show events, as rows of the protocol's event table.
pub fn events() -> Vec<EventDef> {
    vec![
        EventDef {
            name: "show.changed",
            since: "1",
            summary: "A show was added, renamed, started, stopped, died or came back. \
                      Sent by the station to every client, whichever show it is \
                      looking at.",
            ext: None,
            legacy: None,
            payload: schema_of::<ShowChanged>,
        },
        EventDef {
            name: "show.removed",
            since: "1",
            summary: "A show was removed. Its process was stopped first.",
            ext: None,
            legacy: None,
            payload: schema_of::<ShowRemovedEvent>,
        },
        EventDef {
            name: "show.health",
            since: "1",
            summary: "A show's health changed state, or an alarm began or ended. Never sent \
                      for a number alone: read those with show.stats.",
            ext: None,
            legacy: None,
            payload: schema_of::<ShowHealthEvent>,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_reads_a_name_a_show_or_a_project() {
        let named: ShowFrom = serde_json::from_value(json!("empty")).unwrap();
        assert_eq!(named, ShowFrom::Named("empty".into()));
        let project: ShowFrom = serde_json::from_value(json!({"project": {"name": "x"}})).unwrap();
        assert!(matches!(project, ShowFrom::Project { .. }));
    }

    #[test]
    fn a_show_says_its_state_in_lowercase() {
        let show = Show {
            id: "main".into(),
            name: "Main".into(),
            state: ShowState::Running,
            on_air: None,
            programme_kbps: 0,
            cpu_millicores: 0,
            memory_mib: 0,
            restarts: 0,
            error: None,
            compositing: true,
            input: None,
            outputs: vec![],
            health: Health::default(),
        };
        assert_eq!(serde_json::to_value(&show).unwrap()["state"], "running");
    }

    #[test]
    fn a_show_written_before_wave_four_composites() {
        let show: Show = serde_json::from_value(json!({
            "id": "main", "name": "Main", "state": "running", "on_air": null,
            "programme_kbps": 0, "cpu_millicores": 0
        }))
        .unwrap();
        assert!(show.compositing);
        assert!(show.input.is_none() && show.outputs.is_empty());
    }
}
