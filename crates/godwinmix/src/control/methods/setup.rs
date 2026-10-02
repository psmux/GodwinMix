//! `setup.list`, `setup.get` and `setup.start`: the pieces the mixer sets up
//! for itself the first time somebody needs them. See `crate::setup`.
//!
//! A client rarely calls these: adding a web page or picking a camera starts
//! the set up on its own, and `event/setup.changed` reports it. They are for
//! the Try again button after a failure, and for a page that wants to say
//! where everything stands.

use super::{body, handler};
use crate::control::call::Call;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::setup::{SetupRequest, SetupStatus};
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "setup.list",
            Scope::Read,
            "Where each piece the mixer sets up on first use stands: the browser renderer \
             (`web`) and every first party plugin this copy carries.",
            handler(|_call: Call, _| async move { body(crate::setup::list()) }),
        )
        .result(schema_of::<Vec<SetupStatus>>),
    );
    reg.register(
        MethodDef::new("setup.get", Scope::Read, "Where one piece stands, without starting anything.", handler(get))
            .params(schema_of::<SetupRequest>)
            .result(schema_of::<SetupStatus>),
    );
    reg.register(
        MethodDef::new(
            "setup.start",
            Scope::Operate,
            "Set a piece up now, or join the set up already running, and answer at once with \
             where it stands. Progress follows as `event/setup.changed`. Sources waiting on \
             the piece start by themselves when it is ready.",
            handler(start),
        )
        .params(schema_of::<SetupRequest>)
        .result(schema_of::<SetupStatus>),
    );
}

fn piece(call: &Call, params: &Value) -> Result<String, RpcError> {
    let req: SetupRequest = call.params(params)?;
    let piece = req.piece.trim().to_string();
    let known = crate::setup::list().iter().any(|s| s.piece == piece)
        || godwinmix_core::setup::names::known(&piece);
    if !known {
        let ids: Vec<String> = crate::setup::list().into_iter().map(|s| s.piece).collect();
        return Err(RpcError::not_found("setup piece", &piece, &ids));
    }
    Ok(piece)
}

async fn get(call: Call, params: Value) -> Result<Value, RpcError> {
    let piece = piece(&call, &params)?;
    body(crate::setup::status(&piece))
}

async fn start(call: Call, params: Value) -> Result<Value, RpcError> {
    let piece = piece(&call, &params)?;
    match crate::setup::start(&piece) {
        Some(rx) => {
            let now = rx.borrow().clone();
            body(now)
        }
        None => body(crate::setup::status(&piece)),
    }
}
