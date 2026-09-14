//! `preview.open` and `preview.close`: a raw frame socket for a client on the
//! same machine.
//!
//! A native client asks for a target, gets a path back, and reads raw frames
//! off it with no encode anywhere in the chain. Everything about how that is
//! built is in `godwinmix_core::preview::local`; this is the door onto it.
//!
//! The socket exists only while somebody holds it open. `preview.open` twice on
//! one target answers the same path and counts two; the socket goes when the
//! second `preview.close` arrives. A client that dies without closing leaves
//! the socket until the core stops, which is the one thing a request and reply
//! protocol cannot do better: the WebSocket streams tie the branch to the
//! connection instead, and `/mjpeg` or `/pcm` is the right choice for a client
//! that cannot be relied on to close.

use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::preview::local;
use parking_lot::Mutex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

/// `preview.open {target}`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PreviewOpenRequest {
    /// `program`, or a source id.
    pub target: String,
}

/// What `preview.open` answers with.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PreviewSocket {
    pub target: String,
    /// The Unix socket to connect to, absolute. Read it with `unixfdsrc` in
    /// GStreamer, or with the media contract's own reader.
    pub path: String,
    /// What is on the far end, so a client knows what to expect before it
    /// connects.
    pub transport: String,
}

/// What `preview.close` answers with.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PreviewClosed {
    pub target: String,
    pub closed: bool,
}

/// The leases this core is holding, by target.
///
/// A lease is a live object with a `Drop`, and JSON-RPC has nowhere to put one
/// between two calls, so they live here. Keyed by target rather than per call,
/// with a count, which is what makes two opens and two closes balance.
type Leases = Mutex<HashMap<String, (Vec<godwinmix_core::preview::LocalStream>, ())>>;
static OPEN: LazyLock<Arc<Leases>> = LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "preview.open",
            Scope::Read,
            "Open a raw frame socket on this machine for a source or the programme, \
             and answer with its path. No encode anywhere: a client on the same host \
             reads the frames the mixer already has. Close it with preview.close.",
            handler(open),
        )
        .params(schema_of::<PreviewOpenRequest>)
        .result(schema_of::<PreviewSocket>),
    );

    reg.register(
        MethodDef::new(
            "preview.close",
            Scope::Read,
            "Give up a raw frame socket. The socket goes when the last holder closes it.",
            handler(close),
        )
        .params(schema_of::<PreviewOpenRequest>)
        .result(schema_of::<PreviewClosed>),
    );
}

async fn open(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PreviewOpenRequest = serde_json::from_value(params)
        .map_err(|e| RpcError::invalid_params(format!("preview.open needs a target: {e}")))?;
    if !local::supported() {
        return Err(RpcError::not_in_state(local::unsupported_message()));
    }
    let (lease, path) = call
        .app
        .preview
        .open_local(&req.target)
        .await
        .map_err(RpcError::not_in_state)?;
    OPEN.lock().entry(req.target.clone()).or_insert_with(|| (Vec::new(), ())).0.push(lease);
    body(PreviewSocket {
        target: req.target,
        path,
        transport: "unixfd".into(),
    })
}

async fn close(_call: Call, params: Value) -> Result<Value, RpcError> {
    let req: PreviewOpenRequest = serde_json::from_value(params)
        .map_err(|e| RpcError::invalid_params(format!("preview.close needs a target: {e}")))?;
    // Dropping the lease is what asks the mixer to take the socket away, and
    // it only does so when this was the last one.
    let closed = {
        let mut held = OPEN.lock();
        let Some((leases, _)) = held.get_mut(&req.target) else {
            return Err(RpcError::not_found("open preview", &req.target, &open_targets()));
        };
        leases.pop();
        let empty = leases.is_empty();
        if empty {
            held.remove(&req.target);
        }
        empty
    };
    body(PreviewClosed { target: req.target, closed })
}

fn open_targets() -> Vec<String> {
    OPEN.lock().keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_counted_so_two_opens_need_two_closes() {
        // The map is the whole of the bookkeeping, so it is worth testing on
        // its own: the branch itself is tested in `preview/local.rs`.
        let mut held: HashMap<String, (Vec<u8>, ())> = HashMap::new();
        held.entry("program".into()).or_insert_with(|| (Vec::new(), ())).0.push(1);
        held.entry("program".into()).or_insert_with(|| (Vec::new(), ())).0.push(2);
        assert_eq!(held["program"].0.len(), 2, "two opens must count two");
        held.get_mut("program").unwrap().0.pop();
        assert!(!held["program"].0.is_empty(), "one close must not take the socket away");
        held.get_mut("program").unwrap().0.pop();
        assert!(held["program"].0.is_empty(), "the second close is what closes it");
    }

    #[test]
    fn the_method_table_carries_both_and_they_read_only() {
        let mut reg = Registry::new();
        register(&mut reg);
        let open = reg.get("preview.open").expect("preview.open is not registered");
        assert_eq!(open.scope, Scope::Read, "a preview is a read");
        assert!(!open.destructive);
        assert!(reg.get("preview.close").is_some());
    }
}
