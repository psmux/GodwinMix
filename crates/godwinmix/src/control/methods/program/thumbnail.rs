//! `program.thumbnail`: what is on air as a small JPEG, for a monitoring wall.
//!
//! The answer has the shape the direct host's `direct.thumbnail` has, so a
//! station serves a show that composites and a show that does not the same
//! way. The picture comes from a branch on the raw programme tee that runs
//! only for ten seconds after an ask, one frame a second
//! (`godwinmix_core::preview::thumb`), and builds no mosaic.

use super::super::handler;
use crate::control::call::Call;
use base64::Engine;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry};
use godwinmix_protocol::requests::ThumbnailRequest;
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "program.thumbnail",
            Scope::Read,
            "What is on air as a small JPEG in base64, {jpeg, width, height, at_ms}, or \
             {pending: true} while the first picture is on its way. An ask keeps one picture \
             a second coming for ten seconds; nothing runs between asks.",
            handler(thumbnail),
        )
        .params(schema_of::<ThumbnailRequest>)
        .result(any_object),
    );
}

async fn thumbnail(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ThumbnailRequest = call.params(&params)?;
    let width = req.width.unwrap_or(godwinmix_core::preview::thumb::BRANCH_WIDTH);
    let asked = call.app.preview.programme_thumbnail(width).await.map_err(RpcError::not_in_state)?;
    Ok(match asked {
        Some(t) => json!({
            "jpeg": base64::engine::general_purpose::STANDARD.encode(&t.jpeg[..]),
            "width": t.width,
            "height": t.height,
            "at_ms": t.at_ms,
        }),
        None => json!({"pending": true, "retry_after_ms": 1000}),
    })
}
