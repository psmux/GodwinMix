//! `headend.import` and `headend.status` as rows of the method table, and
//! the headend's own setup (`native/table.rs`).
//!
//! The station answers both itself (`calls.rs`). What is registered here is
//! what a core with no station in front of it answers: it follows no
//! Flussonic config, and it has no direct shows to make.

use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::headend::{HeadendImportReport, HeadendImportRequest, HeadendStatus};
use godwinmix_protocol::method::{no_params, schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;

pub const NOT_FOLLOWING: &str = "This station follows no Flussonic config. headend.import makes a \
    config's streams into shows once; godwinmix headend --flussonic FILE follows a file and answers \
    Flussonic's URLs and API.";

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "headend.import",
            Scope::Admin,
            "Make a Flussonic config into headend channels the station keeps, one per stream, \
             each a show without compositing with its input, one HLS output and the transcoder \
             line as its rendition, editable afterwards like any channel set up here. Takes \
             the config's text or a path on the station's machine. A dry run, the default, \
             answers the report (every stream, every line not carried, what the shows \
             would cost) and makes nothing.",
            handler(|_call: Call, _| async move { Err::<serde_json::Value, _>(alone("headend.import")) }),
        )
        .params(schema_of::<HeadendImportRequest>)
        .result(schema_of::<HeadendImportReport>)
        .not_idempotent(),
    );
    reg.register(
        MethodDef::new(
            "headend.status",
            Scope::Read,
            "Whether this station follows a Flussonic config, which file, where \
             Flussonic's URLs and API answer, and how many of its streams run as shows.",
            handler(|_call: Call, _| async move { body(HeadendStatus { note: NOT_FOLLOWING.into(), ..Default::default() }) }),
        )
        .params(no_params)
        .result(schema_of::<HeadendStatus>),
    );
    super::native::table::register(reg);
}

pub(crate) fn alone(method: &str) -> RpcError {
    RpcError::not_in_state(format!(
        "this core runs one show on its own, with no station, so {method} has nowhere to make shows. \
         Start GodwinMix without --show, and call it on the station's control port."
    ))
}
