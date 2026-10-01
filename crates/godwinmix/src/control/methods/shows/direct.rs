//! The wave 4 show methods in the method table: properties, bulk, outputs
//! of a show without compositing, and numbers for many shows at once. A
//! station answers all of them (`crate::station::shows_api`); a core with no
//! station says they need one, as the other show changes do.

use super::needs_station;
use crate::control::call::Call;
use crate::control::methods::handler;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::shows::*;

fn refused(method: &'static str) -> godwinmix_protocol::method::Handler<Call> {
    handler(move |_call: Call, _| async move { Err::<serde_json::Value, _>(needs_station(method)) })
}

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "show.set",
            Scope::Admin,
            "Change a show's name, its input, or whether it composites. Turning compositing \
             on starts a show process whose one source is the input and moves the outputs \
             to it; turning it off hands them back to the direct host, when the show has \
             one source and no scenes in use. The answer says how long the outputs were off.",
            refused("show.set"),
        )
        .params(schema_of::<ShowSetRequest>)
        .result(schema_of::<ShowSetResult>),
    );
    reg.register(
        MethodDef::new(
            "show.add_many",
            Scope::Admin,
            "Make many shows in one call, such as every channel of a headend. The whole \
             batch is checked first. With dry_run (the default) nothing is made: the answer \
             says what would be, what its renditions would cost and whether the governor \
             would admit them. Without it, every show that fits is made and the rest are \
             refused with why; a show is made whole or not at all.",
            refused("show.add_many"),
        )
        .params(schema_of::<ShowAddManyRequest>)
        .result(schema_of::<ShowAddManyResult>)
        .rest_at("POST", "/api/v1/shows/add_many")
        .not_idempotent(),
    );
    reg.register(
        MethodDef::new(
            "show.remove_many",
            Scope::Admin,
            "Stop and remove many shows. Each id that cannot go (main, or one not there) is \
             refused with why, and the rest go.",
            refused("show.remove_many"),
        )
        .params(schema_of::<ShowRemoveManyRequest>)
        .result(schema_of::<ShowRemoveManyResult>)
        .rest_at("POST", "/api/v1/shows/remove_many")
        .destructive(),
    );
    reg.register(
        MethodDef::new(
            "show.stats",
            Scope::Read,
            "Health, input numbers and each output's numbers for many shows in one read, \
             from what the station already holds, so it is cheap to call every second for \
             two hundred shows. `fields` narrows it to health, input or outputs.",
            refused("show.stats"),
        )
        .params(schema_of::<ShowStatsRequest>)
        .result(schema_of::<ShowStatsList>)
        .rest_at("POST", "/api/v1/shows/stats"),
    );
    outputs(reg);
}

fn outputs(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "show.output.add",
            Scope::Admin,
            "Send a show without compositing to another place: an address (SRT, RTMP, UDP, \
             RTP or RIST), or a platform and its key. Left without a rendition it copies the \
             input's bytes; with one it is planned and admitted by the governor. The key is \
             write only.",
            refused("show.output.add"),
        )
        .params(schema_of::<ShowOutputAddRequest>)
        .result(schema_of::<Show>)
        .rest_at("POST", "/api/v1/shows/{show}/outputs")
        .not_idempotent(),
    );
    reg.register(
        MethodDef::new(
            "show.output.set",
            Scope::Admin,
            "Change one output of a show without compositing, naming only what moves: \
             another address, a new key, on or off, copy or a rendition.",
            refused("show.output.set"),
        )
        .params(schema_of::<ShowOutputSetRequest>)
        .result(schema_of::<Show>)
        .rest_at("POST", "/api/v1/shows/{show}/outputs/{output}"),
    );
    reg.register(
        MethodDef::new(
            "show.output.remove",
            Scope::Admin,
            "Stop one output of a show without compositing and forget it, key and all.",
            refused("show.output.remove"),
        )
        .params(schema_of::<ShowOutputRemoveRequest>)
        .result(schema_of::<Show>)
        .rest_at("DELETE", "/api/v1/shows/{show}/outputs/{output}")
        .destructive(),
    );
}
