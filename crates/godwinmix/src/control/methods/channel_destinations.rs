//! A channel's destinations: channel.destination.add, .set and .remove.
//!
//! The methods reach channels through one trait, [`ChannelStore`], which
//! the channel registry (`crate::channels`) implements: it finds the
//! channel, runs the edit under its own lock, seals the addresses and keys,
//! persists, hands the listener the new table (which starts, restarts or
//! stops each restream) and sends `event/channel.changed`. The rules of what
//! an edit may say are pure functions in `rules.rs`.

mod rules;
mod store;

use godwinmix_protocol::destination::{
    AddDestinationRequest, RemoveDestinationRequest, SetDestinationRequest,
};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

use super::handler;
use crate::control::call::Call;
pub use store::{ChannelStore, Edit};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "channel.destination.add",
            Scope::Admin,
            "Send a channel's stream on to YouTube, Facebook, Twitch, an RTMP server or an SRT \
             receiver as it arrives. Nothing is decoded or encoded. The key is write only.",
            handler(|call: Call, params| async move {
                let req: AddDestinationRequest = call.params(&params)?;
                blocking(&call, move |s| add(s, &req)).await
            }),
        )
        .params(schema_of::<AddDestinationRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "channel.destination.set",
            Scope::Admin,
            "Change one of a channel's destinations, naming only what moves: a new key, \
             another server, which stream it sends, on or off. A key left out is kept.",
            handler(|call: Call, params| async move {
                let req: SetDestinationRequest = call.params(&params)?;
                blocking(&call, move |s| set(s, &req)).await
            }),
        )
        .params(schema_of::<SetDestinationRequest>)
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "channel.destination.remove",
            Scope::Admin,
            "Stop sending a channel's stream to one destination and forget it. The publisher \
             and the other destinations are not touched.",
            handler(|call: Call, params| async move {
                let req: RemoveDestinationRequest = call.params(&params)?;
                let dry = call.clone();
                blocking(&call, move |s| remove(s, &req, dry.dry_run.then_some(&dry))).await
            }),
        )
        .params(schema_of::<RemoveDestinationRequest>)
        .result(any_object)
        .destructive(),
    );
}

/// Run an edit on the blocking pool: it seals a key, writes a file and
/// waits for the listener to take the new table.
async fn blocking<F>(call: &Call, work: F) -> Result<Value, RpcError>
where
    F: FnOnce(&dyn ChannelStore) -> Result<Value, RpcError> + Send + 'static,
{
    let channels = call.app.channels.clone();
    tokio::task::spawn_blocking(move || work(&*channels))
        .await
        .map_err(|e| RpcError::internal(format!("the destination edit stopped: {e}")))?
}

fn add(store: &dyn ChannelStore, req: &AddDestinationRequest) -> Result<Value, RpcError> {
    store.edit_destinations(&req.id, &mut |list| rules::add(list, req).map(|_| ()))
}

fn set(store: &dyn ChannelStore, req: &SetDestinationRequest) -> Result<Value, RpcError> {
    store.edit_destinations(&req.id, &mut |list| rules::set(list, req))
}

/// With `dry_run` the edit runs on a copy, so the store sees nothing change
/// and the answer says what would have gone.
fn remove(
    store: &dyn ChannelStore,
    req: &RemoveDestinationRequest,
    dry_run: Option<&Call>,
) -> Result<Value, RpcError> {
    let mut gone = None;
    let channel = store.edit_destinations(&req.id, &mut |list| {
        let mut copy = list.clone();
        gone = Some(rules::remove(&mut copy, &req.id, &req.destination)?);
        if dry_run.is_none() {
            *list = copy;
        }
        Ok(())
    })?;
    match (dry_run, gone) {
        (Some(call), Some(d)) => Ok(call.dry_run_answer(
            true,
            vec![format!("stop sending channel '{}' to {} ({})", req.id, d.label, d.view(Default::default()).uri_host)],
        )),
        _ => Ok(channel),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn req<T: serde::de::DeserializeOwned>(v: Value) -> T {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn the_three_methods_go_through_the_store_and_never_hand_back_a_key() {
        let s = store::MemoryStore::default();
        s.open("methods-test");
        let added = add(&s, &req(json!({"id": "methods-test", "platform": "youtube", "key": "sekret-1"}))).unwrap();
        let text = added.to_string();
        assert!(!text.contains("sekret"), "{text}");
        assert_eq!(added["destinations"][0]["has_key"], true);
        assert_eq!(added["destinations"][0]["uri_host"], "rtmp://a.rtmp.youtube.com");

        let off = set(&s, &req(json!({"id": "methods-test", "destination": "youtube", "enabled": false}))).unwrap();
        assert_eq!(off["destinations"][0]["enabled"], false);
        assert_eq!(off["destinations"][0]["has_key"], true, "a key left out is kept");

        let gone = remove(&s, &req(json!({"id": "methods-test", "destination": "youtube"})), None).unwrap();
        assert_eq!(gone["destinations"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn a_channel_that_is_not_there_is_named_as_missing() {
        let err = add(&store::MemoryStore::default(), &req(json!({"id": "nowhere", "platform": "twitch", "key": "k"}))).unwrap_err();
        assert_eq!(err.data["kind"], "channel");
        assert_eq!(err.data["id"], "nowhere");
    }

    #[test]
    fn the_methods_need_admin_and_each_have_a_route() {
        let reg = super::super::registry();
        for name in ["channel.destination.add", "channel.destination.set", "channel.destination.remove"] {
            let m = reg.iter().find(|m| m.name == name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(m.scope, Scope::Admin, "{name}");
            // Where the transform rule puts them. If `channel` joins the
            // collections, all three would land on one path and this fails
            // first: give them `rest_at` routes then.
            let path = &m.rest.as_ref().unwrap().path;
            assert!(path.starts_with("/api/v1/channel/destination/"), "{path}");
        }
    }
}
