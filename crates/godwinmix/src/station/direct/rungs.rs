//! An HLS output of a direct show that asks for a ladder: each rung its own
//! destination to the planner and the listener, and every rung one variant
//! of the output's master playlist.
//!
//! ```text
//!   stored: hls "flussonic", ladder [1080p, 720p]
//!     planner, listener:  "flussonic" (1080p)   "flussonic~720p" (720p)
//!                              │                       │
//!                    one decode, one scale and one encode each, one pair each
//!                              │                       │
//!     packager:  the master's first variant ◄──────────┴──► its second
//! ```
//!
//! The first rung keeps the output's own id, so everything that reads one
//! rendition of the output (its plan, its view, the GPU it runs on) reads
//! the first rung and needs nothing new. A further rung is `<id>~<rung>`:
//! an output's id is a slug and never has a `~`, so a rung's never meets
//! another output's.

use godwinmix_protocol::destination::StoredDestination;
use godwinmix_protocol::rendition::{RenditionChoice, RenditionRequest};

/// Between an output's id and its rung's name.
pub const SEP: char = '~';

/// The requests of an HLS output's ladder, first rung first. Empty for one
/// that is no ladder, or not HLS.
fn ladder(d: &StoredDestination) -> Vec<RenditionRequest> {
    if d.platform != super::hls::spec::SCHEME {
        return Vec::new();
    }
    let Some(choice) = d.rendition.as_ref() else { return Vec::new() };
    let rungs = godwinmix_render::presets::expand(&d.id, choice).ok().flatten().unwrap_or_default();
    if rungs.len() > 1 { rungs } else { Vec::new() }
}

/// One rung as a destination of its own, asking for that rung alone.
fn rung(d: &StoredDestination, id: String, r: RenditionRequest) -> StoredDestination {
    let request = RenditionRequest { id: String::new(), ..r };
    StoredDestination { id, rendition: Some(RenditionChoice::Request(request)), ..d.clone() }
}

/// The further rungs of an HLS output, after the first, as destinations.
pub fn further(d: &StoredDestination) -> Vec<StoredDestination> {
    let prefix = format!("{}-", d.id);
    ladder(d)
        .into_iter()
        .skip(1)
        .map(|r| {
            let name = r.id.strip_prefix(&prefix).unwrap_or(&r.id).to_string();
            rung(d, format!("{}{SEP}{name}", d.id), r)
        })
        .collect()
}

/// The outputs as the planner and the listener see them: an HLS output's
/// ladder as its first rung under its own id and each further rung beside
/// it. Every other output as it is.
pub fn expand(outs: Vec<StoredDestination>) -> Vec<StoredDestination> {
    let mut out = Vec::with_capacity(outs.len());
    for d in outs {
        let rungs = ladder(&d);
        let Some(first) = rungs.first().cloned() else {
            out.push(d);
            continue;
        };
        let more = further(&d);
        out.push(rung(&d, d.id.clone(), first));
        out.extend(more);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hls(rendition: serde_json::Value) -> StoredDestination {
        StoredDestination {
            id: "flussonic".into(),
            platform: "hls".into(),
            label: "flussonic".into(),
            server: "hls://flussonic".into(),
            key: None,
            stream: "main".into(),
            enabled: true,
            rendition: Some(serde_json::from_value(rendition).unwrap()),
        }
    }

    #[test]
    fn a_ladder_is_its_first_rung_under_the_output_s_id_and_one_destination_per_further_rung() {
        let d = hls(json!({"ladder": [{"video": {"width": 1920, "height": 1080}}, {"video": {"width": 1280, "height": 720}}]}));
        let out = expand(vec![d.clone()]);
        let ids: Vec<&str> = out.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["flussonic", "flussonic~720p"]);
        let height = |o: &StoredDestination| match &o.rendition {
            Some(RenditionChoice::Request(r)) => r.video.as_ref().and_then(|v| v.height),
            _ => None,
        };
        assert_eq!((height(&out[0]), height(&out[1])), (Some(1080), Some(720)));
        assert_eq!(further(&d).len(), 1);
        // One rendition, and any output that is not HLS, are left as they are.
        let single = hls(json!({"video": {"height": 720}}));
        assert_eq!(expand(vec![single.clone()]), vec![single]);
        let push = StoredDestination { platform: "srt".into(), ..d };
        assert_eq!(expand(vec![push.clone()]), vec![push]);
    }
}
