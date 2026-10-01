//! A direct show as a client sees it: its outputs with what the host said
//! of each, its numbers for `show.stats`, and, for a show that composites,
//! its input made its one source.

use super::outputs;
use crate::station::state::Station;
use godwinmix_protocol::destination::Destination;
use godwinmix_protocol::shows::{OutputStats, ShowStats, ShowState};
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
        outputs::stored(id, &r.outputs)
            .iter()
            .map(|d| {
                let mut view = d.view(seen.output(&d.id, d.enabled && !r.compositing));
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
        let records = st.registry.lock().get(id).map(|r| r.outputs.clone()).unwrap_or_default();
        let outputs = records
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
        ShowStats { id: id.to_string(), health: self.health_of(st, id), input: seen.input_stats.clone(), outputs }
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
    let wanted = {
        let reg = st.registry.lock();
        let Some(r) = reg.get(id).filter(|r| r.compositing && r.input.is_some()) else { return };
        let seen = st.direct.seen.lock();
        let Some((relay, stream)) = seen.get(id).and_then(|s| s.relay()) else { return };
        if seen.get(id).and_then(|s| s.source_for.as_deref()) == Some(stream.as_str()) {
            return;
        }
        (r.name.clone(), relay, stream)
    };
    if st.state_of(id) != Some(ShowState::Running) {
        return;
    }
    let (name, relay, stream) = wanted;
    let wait = Duration::from_secs(5);
    let params = json!({
        "id": INPUT_SOURCE, "name": format!("{name} input"), "type": "ingest/rtmp",
        "uri": format!("channel:{stream}"), "relay": relay, "stream": stream,
    });
    match st.ask_show(id, "source.add", params, wait).await {
        Ok(_) => info!(show = id, %stream, "the show's input became its source"),
        Err(e) if e.message.contains("already") => {}
        Err(e) => return warn!(show = id, error = %e.message, "the show would not take its input as a source"),
    }
    let _ = st.ask_show(id, "program.take", json!({"source": INPUT_SOURCE}), wait).await;
    if let Some(s) = st.direct.seen.lock().get_mut(id) {
        s.source_for = Some(stream);
    }
}
