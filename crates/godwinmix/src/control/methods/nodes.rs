//! `node.*`: the machines this core hosts plugins on.
//!
//! Five methods and nothing clever. A node is enrolled once with a one time
//! token, it dials in and stays, and from then on it is a name in `place` on a
//! source. What an operator does here is mint a token, look at what is
//! connected, and remove a machine that has gone for good.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::node;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::method::Tier;
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "node.list",
            Scope::Read,
            "Every node this core knows about: the ones connected now, the ones that have \
             gone quiet, and the ones the config expects that have never dialled in.",
            handler(list),
        )
        .result(schema_of::<NodeListing>)
        .tool(
            "list_nodes",
            Tier::Search,
            "List the machines hosting plugins for this mixer. Each one reports whether it is \
             online, how long since its last heartbeat, how far its clock sits from the \
             programme clock, and which sources it is running. A source placed on a node that \
             is offline is holding a freeze frame and then the slate.",
        ),
    );
    reg.register(
        MethodDef::new(
            "node.get",
            Scope::Read,
            "One node: its clock offset, how long since its last heartbeat, the plugins it \
             has, and the instances it is hosting.",
            handler(get),
        )
        .params(schema_of::<NodeName>)
        .result(schema_of::<node::NodeView>),
    );
    reg.register(
        MethodDef::new(
            "node.enrol",
            Scope::Admin,
            "Mint a one time enrolment token for a node. The answer carries the command to \
             run on the other machine. The token is good for one enrolment and expires.",
            handler(enrol),
        )
        .params(schema_of::<EnrolRequest>)
        .not_idempotent(),
    );
    reg.register(
        MethodDef::new(
            "node.remove",
            Scope::Admin,
            "Forget a node. Its bridge is closed, every token minted for a plugin on it is \
             revoked, and its certificate stops working. Sources placed on it go to the \
             slate until they are moved or the node enrols again.",
            handler(remove),
        )
        .params(schema_of::<NodeName>)
        .destructive(),
    );
    reg.register(
        MethodDef::new(
            "node.discover",
            Scope::Read,
            "Look for nodes on the local network over mDNS. A network without multicast \
             finds nothing and the [nodes] table in the config is the way there.",
            handler(discover),
        )
        .params(schema_of::<DiscoverRequest>)
        .result(schema_of::<DiscoverAnswer>),
    );
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeName {
    /// The node's name, as it was enrolled.
    pub id: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeListing {
    pub nodes: Vec<node::NodeView>,
    /// Whether this core is listening for nodes at all.
    pub listening: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct EnrolRequest {
    /// What the node will call itself. A slug: it goes in `place` and in the
    /// node's certificate.
    pub name: String,
    /// Where the node is, for the record. The node always dials the core, so
    /// this is what `node.list` shows before it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// How long the token is good for. Default one hour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DiscoverRequest {
    /// How long to listen. Capped at 4.5 seconds, so the call stays inside the
    /// five second ceiling every method is held to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DiscoverAnswer {
    pub found: Vec<node::discovery::Found>,
}

async fn list(_call: Call, _params: Value) -> Result<Value, RpcError> {
    match node::runtime::get() {
        Some(runtime) => body(NodeListing { nodes: runtime.nodes.views(), listening: true }),
        None => body(NodeListing { nodes: Vec::new(), listening: false }),
    }
}

async fn get(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: NodeName = call.params(&params)?;
    let runtime = runtime()?;
    match runtime.nodes.view(&req.id) {
        Some(view) => body(view),
        None => Err(RpcError::not_found("node", &req.id, &runtime.nodes.names())),
    }
}

async fn enrol(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: EnrolRequest = call.params(&params)?;
    let name = req.name.trim().to_string();
    if name.is_empty() || name.contains(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
        return Err(RpcError::invalid_params(format!(
            "`{}` is not a node name. Use letters, digits and hyphens: it goes in a \
             certificate and in `place = \"node:<name>\"`.",
            req.name
        )));
    }
    let runtime = runtime()?;
    if call.dry_run {
        return Ok(call.dry_run_answer(
            true,
            vec![format!("mint a one time enrolment token for the node `{name}`")],
        ));
    }
    let ttl = std::time::Duration::from_secs(
        req.ttl_secs.unwrap_or(node::enrol::DEFAULT_TTL.as_secs()).clamp(60, 86_400),
    );
    // Enrolment is a task because the operator's next step is on another
    // machine and may take as long as it takes to walk there. The token is
    // minted at once and the task finishes when the node dials in, so
    // `task.get` answers "still waiting" until it does.
    let tickets = runtime.tickets.clone();
    let nodes = runtime.nodes.clone();
    let fingerprint = runtime.ca.fingerprint();
    let ticket = tickets
        .mint(&name, ttl)
        .map_err(|e| RpcError::new(ErrorCode::InternalError, format!("{e:#}")))?;
    nodes.expect(&name, req.address.clone());
    let token = format!("{fingerprint}.{}", ticket.token);
    let extra = json!({
        "name": name,
        "token": token,
        "expires_unix": ticket.expires_unix,
        "command": format!("godwinmix node --core <this core>:<node port> --name {name} --token {token}"),
    });
    let waiting = name.clone();
    Ok(super::tasks::spawn_task(&call.app.tasks, "node.enrol", Some(extra), move |ctx| async move {
        // Wait for the node to arrive, up to the token's life, checking four
        // times a second. Cancelling the task does not cancel the token.
        let deadline = std::time::Instant::now() + ttl;
        while std::time::Instant::now() < deadline {
            if ctx.cancelled() {
                return Ok(json!({ "enrolled": false, "reason": "the wait was cancelled" }));
            }
            if nodes.is_online(&waiting) {
                return Ok(json!({ "enrolled": true, "node": waiting }));
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        Err(format!(
            "`{waiting}` did not enrol before the token expired. Mint another with `gmx node \
             token --name {waiting}`"
        ))
    }))
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: NodeName = call.params(&params)?;
    let runtime = runtime()?;
    let Some(view) = runtime.nodes.view(&req.id) else {
        return Err(RpcError::not_found("node", &req.id, &runtime.nodes.names()));
    };
    if call.dry_run {
        let mut diff = vec![format!("forget the node {} ({})", view.name, view.state)];
        for instance in &view.instances {
            diff.push(format!("{} would go to the slate", instance.instance));
        }
        return Ok(call.dry_run_answer(true, diff));
    }
    runtime.nodes.remove(&req.id);
    runtime.tickets.forget(&req.id);
    godwinmix_core::plugin::remote::forget(&req.id);
    let revoked = call.app.tokens.revoke_node(&req.id);
    tracing::warn!(node = %req.id, revoked, "a node was removed");
    body(json!({
        "removed": true,
        "node": req.id,
        "tokens_revoked": revoked,
        "instances": view.instances.len(),
    }))
}

async fn discover(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: DiscoverRequest = call.params(&params)?;
    let within =
        std::time::Duration::from_millis(req.timeout_ms.unwrap_or(2_000).min(4_500));
    let found = tokio::task::spawn_blocking(move || node::discovery::browse(within))
        .await
        .map_err(|e| RpcError::new(ErrorCode::InternalError, format!("discovery panicked: {e}")))?;
    match found {
        Ok(found) => body(DiscoverAnswer { found }),
        Err(e) => Err(RpcError::new(
            ErrorCode::InternalError,
            format!(
                "{e:#}. A network without multicast finds nothing this way; list the node in \
                 the [nodes] table instead"
            ),
        )),
    }
}

fn runtime() -> Result<std::sync::Arc<node::runtime::Runtime>, RpcError> {
    node::runtime::get().ok_or_else(|| {
        RpcError::new(
            ErrorCode::NotInState,
            "this core has no node bridge, so it has no nodes. Add a [nodes] table to the \
             config with `listen` and restart.",
        )
    })
}
