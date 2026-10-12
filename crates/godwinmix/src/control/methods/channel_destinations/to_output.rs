//! `channel.destination.to_output`: the programme sent where a channel's
//! destination points.
//!
//! A tester added Custom RTMP to a channel no encoder sent to, saw it wait,
//! pressed "Send the programme instead" and was asked for the stream key
//! again, because the channel keeps keys sealed and never hands one back.
//! The output that came of it ran at the programme encoder's 6 Mb/s, not
//! the 3 Mb/s they had picked. Here the core reads the key and the format
//! itself and gives both to the new output.

use godwinmix_protocol::destination::{DestinationToOutputRequest, RemoveDestinationRequest, StoredDestination};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::rendition::RenditionChoice;
use serde_json::{json, Value};

use super::ChannelStore;
use crate::control::call::Call;

pub(super) async fn run(call: Call, req: DestinationToOutputRequest) -> Result<Value, RpcError> {
    let store = call.app.channels.clone();
    let (channel, wanted) = (req.id.clone(), req.destination.clone());
    let dest = tokio::task::spawn_blocking(move || find(&*store, &channel, &wanted))
        .await
        .map_err(|e| RpcError::internal(format!("reading the destination stopped: {e}")))??;
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    let taken: Vec<String> = status.outputs.iter().map(|o| o.id.clone()).collect();
    let params = output_params(&dest, &taken);
    let output = super::super::outputs::add(call.clone(), params).await?;
    if req.remove {
        let store = call.app.channels.clone();
        let gone = RemoveDestinationRequest { id: req.id.clone(), destination: req.destination.clone() };
        tokio::task::spawn_blocking(move || super::remove(&*store, &gone, None))
            .await
            .map_err(|e| RpcError::internal(format!("taking the destination off the channel stopped: {e}")))??;
    }
    Ok(output)
}

/// The destination as kept, key and all. The edit leaves the list alone, so
/// nothing is saved.
fn find(store: &dyn ChannelStore, channel: &str, wanted: &str) -> Result<StoredDestination, RpcError> {
    let mut found = None;
    store.edit_destinations(channel, &mut |list| {
        found = list.iter().find(|d| d.id == wanted).cloned();
        if found.is_some() {
            return Ok(());
        }
        let ids: Vec<String> = list.iter().map(|d| d.id.clone()).collect();
        Err(RpcError::not_found("destination", wanted, &ids))
    })?;
    found.ok_or_else(|| RpcError::not_found("destination", wanted, &[]))
}

/// `output.add` params: the same address and key, the same format, and an
/// id not taken yet (`custom`, then `custom-2`).
fn output_params(d: &StoredDestination, taken: &[String]) -> Value {
    let mut id = d.id.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{}-{n}", d.id);
        n += 1;
    }
    let mut params = json!({ "id": id, "uri": d.url(), "policy": d.policy() });
    if let Some(r) = d.rendition.as_ref().filter(|r| !is_copy(r)) {
        params["rendition"] = json!(r);
    }
    params
}

/// A channel's "same as the source" means nothing for the programme, which
/// has its own encoder already.
fn is_copy(r: &RenditionChoice) -> bool {
    matches!(r, RenditionChoice::Preset(p) if p.preset == "copy")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dest(rendition: Value) -> StoredDestination {
        StoredDestination {
            id: "custom".into(),
            platform: "custom".into(),
            label: "Custom RTMP".into(),
            server: "rtmp://streaming.example.in:1935/demo".into(),
            key: Some("sekret".into()),
            stream: "*".into(),
            enabled: true,
            rendition: serde_json::from_value(rendition).ok(),
        }
    }

    #[test]
    fn the_output_gets_the_key_the_format_and_a_free_id() {
        let p = output_params(&dest(json!({"preset": "youtube-720p30"})), &["custom".into()]);
        assert_eq!(p["id"], "custom-2");
        assert_eq!(p["uri"], "rtmp://streaming.example.in:1935/demo/sekret");
        assert_eq!(p["policy"], "own");
        assert_eq!(p["rendition"]["preset"], "youtube-720p30");
    }

    #[test]
    fn same_as_the_source_leaves_the_programme_encoder_as_it_is() {
        let p = output_params(&dest(json!({"preset": "copy"})), &[]);
        assert_eq!(p["id"], "custom");
        assert!(p.get("rendition").is_none(), "{p}");
    }
}
