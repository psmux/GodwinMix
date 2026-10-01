//! A direct show as a client sees it: its outputs with what the host said
//! of each, its numbers for `show.stats`, and, for a show that composites,
//! its input made its one source.

use crate::station::state::Station;
use godwinmix_protocol::destination::Destination;
use godwinmix_protocol::shows::{OutputStats, ShowState, ShowStats, ShowWork};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// The id the input gets as a source of a show that composites.
pub const INPUT_SOURCE: &str = "input";

impl super::Direct {
    /// A direct show's outputs as a list shows them.
    pub fn output_views(&self, st: &Station, id: &str) -> Vec<Destination> {
        let Some(r) = st.registry.lock().get(id).cloned() else { return Vec::new() };
        let seen = self.seen.lock().get(id).cloned().unwrap_or_default();
        // From the records alone: a list of two hundred shows unseals no key.
        r.outputs
            .iter()
            .map(|d| {
                let live = seen.output(&d.id, d.enabled && !r.compositing);
                let mut view = Destination {
                    id: d.id.clone(),
                    platform: d.platform.clone(),
                    label: d.label.clone(),
                    uri_host: d.uri_host.clone(),
                    has_key: d.has_key,
                    stream: "main".into(),
                    enabled: d.enabled,
                    live,
                    rendition: d.rendition.clone(),
                    plan: None,
                    refused: None,
                };
                if d.rendition.is_some() && d.enabled && !r.compositing {
                    let (plan, refused) = self.transcode.view(id, &d.id);
                    view.plan = plan;
                    view.refused = refused;
                }
                view
            })
            .collect()
    }

    /// One show's numbers. Cheap: what the host last sent, and the plan.
    pub fn stats_of(&self, st: &Station, id: &str) -> ShowStats {
        let seen = self.seen.lock().get(id).cloned().unwrap_or_default();
        let (records, compositing) = st.registry.lock().get(id).map(|r| (r.outputs.clone(), r.compositing)).unwrap_or_default();
        let outputs: Vec<OutputStats> = records
            .iter()
            .filter(|o| o.enabled)
            .map(|o| {
                let counted = seen.output_stats.iter().find(|s| s.id == o.id).cloned();
                let mut row = counted.unwrap_or_else(|| OutputStats { id: o.id.clone(), ..Default::default() });
                if row.state.is_empty() {
                    row.state = serde_json::to_value(seen.output(&o.id, true).state).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                }
                row.rendition_text = match (&o.rendition, self.transcode.view(id, &o.id).0) {
                    (None, _) => "copy".into(),
                    (Some(_), Some(plan)) => text_of(&plan),
                    (Some(_), None) => "waiting for the input".into(),
                };
                row
            })
            .collect();
        let encodes = records.iter().any(|o| o.enabled && o.rendition.is_some());
        let work = match (compositing, encodes) {
            (true, _) => ShowWork::Mix,
            (false, true) => ShowWork::Transcode,
            (false, false) => ShowWork::Copy,
        };
        // A show that mixes is a process of its own, read by `show.stats`.
        let cpu_millicores = (!compositing).then(|| outputs.iter().map(|o| o.cpu_millicores).sum());
        ShowStats { id: id.to_string(), health: self.health_of(st, id), work, cpu_millicores, input: seen.input_stats.clone(), outputs }
    }
}

/// What a plan gave an output, in a few words: `h264 1280x720 30 fps on x264`.
fn text_of(plan: &godwinmix_protocol::destination::DestinationPlan) -> String {
    let Some(video) = &plan.video else { return "copy".into() };
    let shape = godwinmix_render::container::shape_text(video);
    match &plan.encoder {
        Some(e) => format!("{shape} on {e}"),
        None => shape,
    }
}

/// A show that composites and has an input: once the show runs and the
/// host says where the input is in the hub, make it the show's source and
/// put it on programme. Done once per hub path.
pub async fn feed_source(st: &Arc<Station>, id: &str) {
    if st.state_of(id) != Some(ShowState::Running) {
        return;
    }
    // Claimed before asking, so the show coming up and the switch, which
    // both call this, add the source once between them.
    let wanted = {
        let reg = st.registry.lock();
        let Some(r) = reg.get(id).filter(|r| r.compositing && r.input.is_some()) else { return };
        let mut seen = st.direct.seen.lock();
        let Some(s) = seen.get_mut(id) else { return };
        let Some((relay, stream)) = s.relay() else { return };
        if s.source_for.as_deref() == Some(stream.as_str()) {
            return;
        }
        s.source_for = Some(stream.clone());
        (r.name.clone(), relay, stream)
    };
    let (name, relay, stream) = wanted;
    let wait = Duration::from_secs(5);
    let params = json!({
        "id": INPUT_SOURCE, "name": format!("{name} input"), "type": "ingest/rtmp",
        "uri": format!("channel:{stream}"), "relay": relay, "stream": stream,
    });
    // A show started again keeps its sources, and a second add would make
    // `input-2` beside it rather than fail.
    let listed = st.ask_show(id, "source.list", json!({}), wait).await.unwrap_or_default();
    let has = listed.as_array().is_some_and(|rows| rows.iter().any(|s| s["id"] == INPUT_SOURCE));
    if !has {
        match st.ask_show(id, "source.add", params, wait).await {
            Ok(_) => info!(show = id, %stream, "the show's input became its source"),
            Err(e) => {
                if let Some(s) = st.direct.seen.lock().get_mut(id) {
                    s.source_for = None;
                }
                return warn!(show = id, error = %e.message, "the show would not take its input as a source");
            }
        }
        let _ = st.ask_show(id, "program.take", json!({"source": INPUT_SOURCE}), wait).await;
    }
}
