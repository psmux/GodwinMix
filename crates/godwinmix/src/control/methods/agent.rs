//! The document an agent reads before it decides anything.
//!
//! 05 section 6 and 09 section 5: `agent.state` in two shapes. `concise` is
//! the default and is budgeted in bytes, because every read is charged for in
//! somebody's context window: under 250 bytes at 2 sources, 500 at 6 and
//! 1,200 at 16, measured by a test here. `detailed` adds the audio peak per
//! source, the plugin statistics and the last five takes, for the moments
//! when a number is not enough and a picture is too much.
//!
//! The concise document leaves out everything that is at its usual value. A
//! camera that is live, has video and has sound is four fields; one that has
//! lost its audio says so. That is what keeps sixteen sources inside the
//! budget without shortening the field names into something nobody can read.

use super::{body, handler};
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::{MixerStatus, OutputState, SourceState};
use crate::control::call::Call;
use godwinmix_core::snapshot::Tracker;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One snapshot URL pattern rather than three addresses: `program`, `sheet`
/// and a source id all go in the same place.
pub const SNAPSHOT_URL: &str = "/api/v1/snapshot/{id}";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResponseFormat {
    #[default]
    Concise,
    Detailed,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct AgentStateRequest {
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// The concise document.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Concise {
    /// What is on air. `null` is the slate.
    pub program: Option<String>,
    /// How much the programme picture is changing, 0 to 1, when a snapshot
    /// tracker is running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_motion: Option<f64>,
    pub uptime_secs: u64,
    pub sources: Vec<ConciseSource>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ConciseOutput>,
    /// Substitute a source id, `program` or `sheet` for `{id}`. Absent when
    /// this build has no stills.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<&'static str>,
    /// Present only while a safety rule is holding the programme, so an agent
    /// that cannot take knows why before it tries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held: Option<String>,
}

/// A source, carrying only what is not the ordinary case.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConciseSource {
    pub id: String,
    pub state: SourceState,
    /// Only when it is not the id, which is most of the time on a machine
    /// somebody has named their cameras on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion: Option<f64>,
    /// Absent means it has video. Present and true means it does not.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_video: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_audio: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub superimposed: bool,
    /// How long since the last frame, only once the picture has stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_idle_ms: Option<u64>,
    /// Only in the detailed document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_peak_db: Option<f64>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConciseOutput {
    pub id: String,
    pub state: OutputState,
    #[serde(skip_serializing_if = "is_zero")]
    pub reconnects: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// A picture is worth looking at once the picture has stopped moving.
const IDLE_WORTH_SAYING_MS: u64 = 1_000;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "agent.state",
            Scope::Read,
            "The compact document written for agents: the programme, each source's state \
             and a motion score saying how much its picture is changing.",
            handler(|call: Call, params| async move {
                let req: AgentStateRequest = call.params(&params).unwrap_or_default();
                let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
                let mut document = body(concise(&call, &status))?;
                if req.response_format == ResponseFormat::Detailed {
                    detail(&call, &status, &mut document);
                }
                Ok(document)
            }),
        )
        .params(schema_of::<AgentStateRequest>)
        .result(any_object)
        .mutating(false)
        .tool(
            "agent_state",
            Tier::Minimal,
            "Compact state written for agents, a few hundred bytes: the programme source, \
             each source's id and state, and a motion score saying how much its picture is \
             changing, so you can tell a live camera from a frozen or black one without \
             looking at it. A working source says nothing about its video or sound; one \
             that has lost either says so. Start here. `detailed` adds the audio peak per \
             source and the last five takes.",
        ),
    );
}

/// Fold the status and the motion scores into the budgeted document.
pub fn concise(call: &Call, status: &MixerStatus) -> Concise {
    document(status, call.snapshots.as_ref(), held_reason(call))
}

/// Why the programme cannot be taken right now, if it cannot.
fn held_reason(call: &Call) -> Option<String> {
    call.app.safety.check(&call.token).err().map(|r| r.message)
}

/// The same without a `Call`, so the event push can build it too.
pub fn document(
    status: &MixerStatus,
    snapshots: &Tracker,
    held: Option<String>,
) -> Concise {
    let latest = snapshots.latest();
    let stills = snapshots.enabled() && status.multiview.enabled;
    let score = |pick: godwinmix_core::snapshot::Pick| -> Option<f64> {
        let l = latest.as_ref()?;
        let motion = l.motion.as_ref()?;
        let c = godwinmix_core::snapshot::find_cell(&l.cells, &pick)?;
        motion.get(c.index as usize).copied()
    };
    Concise {
        program: status.program.clone(),
        program_motion: score(godwinmix_core::snapshot::Pick::Program),
        uptime_secs: status.uptime_secs,
        sources: status
            .sources
            .iter()
            .map(|s| ConciseSource {
                id: s.id.clone(),
                state: s.state,
                name: (s.name != s.id).then(|| s.name.clone()),
                motion: score(godwinmix_core::snapshot::Pick::Source(s.id.clone())),
                no_video: !s.has_video,
                no_audio: !s.has_audio,
                superimposed: s.superimposed(),
                video_idle_ms: s.video_idle_ms.filter(|ms| *ms >= IDLE_WORTH_SAYING_MS),
                audio_peak_db: None,
            })
            .collect(),
        outputs: status
            .outputs
            .iter()
            .map(|o| ConciseOutput { id: o.id.clone(), state: o.state, reconnects: o.reconnects })
            .collect(),
        snapshot: stills.then_some(SNAPSHOT_URL),
        held,
    }
}

/// Everything `detailed` adds, merged into the concise body.
fn detail(call: &Call, status: &MixerStatus, document: &mut Value) {
    let Some(map) = document.as_object_mut() else { return };
    map.insert("backend".into(), serde_json::to_value(&status.backend).unwrap_or(Value::Null));
    map.insert(
        "recent_takes".into(),
        serde_json::to_value(call.app.history.recent(5)).unwrap_or(Value::Null),
    );
    map.insert(
        "safety".into(),
        serde_json::json!({
            "min_hold_ms": call.app.safety.limits_for(&call.token).min_hold_ms,
            "max_takes_per_minute": call.app.safety.limits_for(&call.token).max_takes_per_minute,
            "flash_guard": call.app.safety.limits_for(&call.token).flash_guard,
        }),
    );
    let telemetry = godwinmix_core::telemetry::telemetry().read();
    map.insert(
        "telemetry".into(),
        serde_json::json!({
            "shot": telemetry.shot,
            "black": telemetry.black,
            "freeze": telemetry.freeze(godwinmix_core::telemetry::DEFAULT_FREEZE_MS),
            "lufs_s": telemetry.lufs_s,
            "lufs_i": telemetry.lufs_i,
            "silence": telemetry.silence(godwinmix_core::telemetry::DEFAULT_SILENCE_MS),
            "measuring": godwinmix_core::telemetry::telemetry().wanted(),
        }),
    );
    // The audio peak per source, which the audit noted was missing.
    if let Some(sources) = map.get_mut("sources").and_then(Value::as_array_mut) {
        for (entry, source) in sources.iter_mut().zip(&status.sources) {
            let Some(entry) = entry.as_object_mut() else { continue };
            if let Some(peak) = call.app.peaks.get(&source.id) {
                entry.insert("audio_peak_db".into(), serde_json::json!(peak));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document_of(n: usize) -> Concise {
        Concise {
            program: Some("cam1".into()),
            program_motion: Some(0.12),
            uptime_secs: 942,
            sources: (1..=n)
                .map(|i| ConciseSource {
                    id: format!("cam{i}"),
                    state: SourceState::Live,
                    name: None,
                    motion: Some(0.1),
                    no_video: false,
                    no_audio: false,
                    superimposed: false,
                    video_idle_ms: None,
                    audio_peak_db: None,
                })
                .collect(),
            outputs: vec![ConciseOutput {
                id: "yt".into(),
                state: OutputState::Live,
                reconnects: 0,
            }],
            snapshot: Some(SNAPSHOT_URL),
            held: None,
        }
    }

    /// The budget from the brief and from 09 section 5: the document an agent
    /// reads before every decision is measured, and the test fails rather than
    /// letting it grow.
    #[test]
    fn the_concise_document_stays_inside_its_byte_budget() {
        for (sources, budget) in [(2usize, 250usize), (6, 500), (16, 1_200)] {
            let text = serde_json::to_string(&document_of(sources)).unwrap();
            assert!(
                text.len() < budget,
                "at {sources} sources the concise document is {} bytes, budget {budget}:\n{text}",
                text.len()
            );
        }
    }

    /// A camera that is working says nothing about it; one that has lost its
    /// sound says so. That is what keeps sixteen sources inside the budget.
    #[test]
    fn a_source_carries_only_what_is_not_the_ordinary_case() {
        let mut doc = document_of(1);
        let text = serde_json::to_string(&doc).unwrap();
        assert!(!text.contains("no_audio"), "{text}");
        assert!(!text.contains("superimposed"), "{text}");
        assert!(!text.contains("\"name\""), "a name equal to the id is not worth a field: {text}");
        assert!(!text.contains("held"), "{text}");

        doc.sources[0].no_audio = true;
        doc.sources[0].name = Some("Wide".into());
        doc.sources[0].video_idle_ms = Some(4_000);
        doc.held = Some("held".into());
        let text = serde_json::to_string(&doc).unwrap();
        assert!(text.contains("\"no_audio\":true"), "{text}");
        assert!(text.contains("\"name\":\"Wide\""), "{text}");
        assert!(text.contains("\"video_idle_ms\":4000"), "{text}");
        assert!(text.contains("\"held\""), "{text}");
    }

    #[test]
    fn the_response_format_defaults_to_concise() {
        let req: AgentStateRequest = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(req.response_format, ResponseFormat::Concise);
        let req: AgentStateRequest =
            serde_json::from_value(serde_json::json!({ "response_format": "detailed" })).unwrap();
        assert_eq!(req.response_format, ResponseFormat::Detailed);
    }

}
