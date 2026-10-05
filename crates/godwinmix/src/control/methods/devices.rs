//! `token.create`, `token.list` and `token.revoke`: device tokens, so a phone
//! is signed in with one scan instead of a config edit and a restart. The
//! work is in `crate::devices`; a station answers the same three itself.

use super::handler;
use crate::control::call::Call;
use godwinmix_protocol::devices::{TokenCreateRequest, TokenCreated, TokenList, TokenRevokeRequest, TokenRevoked};
use godwinmix_protocol::method::{no_params, schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

async fn run(call: Call, params: Value) -> Result<Value, godwinmix_protocol::error::RpcError> {
    crate::devices::call(&call.app.tokens, call.method, params).await
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "token.create",
            Scope::Admin,
            "Make a token for one phone or tablet, with the read, operate (the default) or \
             admin scope. The secret is in this answer and nowhere else: the mixer keeps only \
             a digest of it. Open the page at https://<host>:<port>/#token=<token> to sign \
             the device in.",
            handler(run),
        )
        .params(schema_of::<TokenCreateRequest>)
        .result(schema_of::<TokenCreated>)
        .not_idempotent(),
    );
    reg.register(
        MethodDef::new(
            "token.list",
            Scope::Admin,
            "Every device token: its id, label, scope and when it was made. Never a secret.",
            handler(run),
        )
        .params(no_params)
        .result(schema_of::<TokenList>)
        .mutating(false),
    );
    reg.register(
        MethodDef::new(
            "token.revoke",
            Scope::Admin,
            "Take a device token back. The device's next call is refused, including on a \
             connection it already has open.",
            handler(run),
        )
        .params(schema_of::<TokenRevokeRequest>)
        .result(schema_of::<TokenRevoked>)
        .destructive(),
    );
}
