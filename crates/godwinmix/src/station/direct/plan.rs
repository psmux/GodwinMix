//! Direct outputs that ask for a rendition, planned by the planner the
//! channels use. To it a direct show is a channel with one stream, `main`,
//! whose shape is what `direct.input` last said, so one decode, one scale
//! and one encoder are shared by every output of the show that wants them,
//! and the governor admits each node before the host builds it.

use super::Direct;
use crate::channels::{Live, Record as Channel};
use crate::station::registry::Record;
use godwinmix_protocol::channel_ingest::Rtmps;
use godwinmix_protocol::destination::{DestinationRefusal, StoredDestination};
use godwinmix_protocol::rendition::Cost;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// A direct show as the planner's channel.
fn channel(id: &str) -> Channel {
    Channel {
        id: id.to_string(),
        name: id.to_string(),
        app: id.to_string(),
        enabled: true,
        auto_source: false,
        key_mode: Default::default(),
        protocols: Vec::new(),
        rtmps: Rtmps::default(),
        keys: Vec::new(),
        auto_sources: Vec::new(),
        destinations: Vec::new(),
        extra: BTreeMap::new(),
    }
}

/// Its input as the planner's live stream, once the host said it is live.
fn live(id: &str, input: &Value) -> Option<Live> {
    if input["state"] != "live" {
        return None;
    }
    let mut v = input.clone();
    v["channel"] = json!(id);
    v["app"] = json!(id);
    v["stream"] = json!("main");
    let mut l = Live::from_plugin(&v)?;
    l.channel = id.to_string();
    Some(l)
}

impl Direct {
    /// Plan every direct show's renditions again, from the records and what
    /// the host said of each input. Tickets for nodes that did not change
    /// are kept.
    pub(super) fn replan(&self, shows: &[(Record, Vec<StoredDestination>)]) {
        let channels: Vec<Channel> = shows.iter().map(|(r, _)| channel(&r.id)).collect();
        let lives: Vec<Live> = {
            let seen = self.seen.lock();
            shows.iter().filter_map(|(r, _)| live(&r.id, seen.get(&r.id)?.input.as_ref()?)).collect()
        };
        let outputs: BTreeMap<&str, &Vec<StoredDestination>> = shows.iter().map(|(r, o)| (r.id.as_str(), o)).collect();
        self.transcode.replan(&channels, &lives, |c| outputs.get(c.id.as_str()).map(|o| o.to_vec()).unwrap_or_default());
    }

    /// What a show's outputs would cost against an input shaped like a
    /// broadcast HD feed, taking nothing.
    pub fn price(&self, outputs: &[StoredDestination]) -> Result<Cost, DestinationRefusal> {
        let (assumed, _) = crate::channels::transcode::assumed_input();
        self.transcode.price(outputs, &assumed)
    }
}
