//! `vitals.get` and `vitals.set`: this show's own health and the thresholds
//! its alarms are judged by (`crates/godwinmix-core/src/vitals/`).
//!
//! For a show that composites. The station keeps a show's alarm settings
//! with the show and hands them over with `vitals.set` when it starts it; a
//! change applies within a second, with nothing restarted. A direct show's
//! settings go in its row of the direct table instead.

use super::handler;
use crate::control::call::Call;
use godwinmix_core::vitals::{self, VitalsConfig};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "vitals.get",
            Scope::Read,
            "This show's health (its state and alarms, null in the first second) and the \
             thresholds they are judged by.",
            handler(get),
        )
        .result(any_object),
    );
    reg.register(
        MethodDef::new(
            "vitals.set",
            Scope::Operate,
            "Change the alarm thresholds, or whether a mosaic is kept up for the black and \
             freeze checks while nobody is looking. Fields left out keep their defaults; a \
             duration of 0 switches that check off. Applies within a second.",
            handler(set),
        )
        .params(schema_of::<VitalsConfig>)
        .result(any_object),
    );
}

fn answer() -> Value {
    let shared = vitals::process();
    json!({"health": shared.health(), "settings": shared.settings()})
}

async fn get(_call: Call, _params: Value) -> Result<Value, RpcError> {
    Ok(answer())
}

async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let cfg: VitalsConfig = call.params(&params)?;
    vitals::process().set(cfg);
    Ok(answer())
}
