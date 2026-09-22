//! `config.get`, `config.set`, `config.reset` and `config.schema`: the mixer's
//! own settings over the protocol.
//!
//! The keys are dotted, the same spelling a preset plan uses for
//! `ConfigChange.key`. Which keys exist, their types, ranges and when a change
//! takes effect all come from `godwinmix_core::config::{keys, schema}`, so
//! this file is the transport and nothing else. Writes go through
//! `config::settable`, which keeps the operator's comments and writes nothing
//! when the file as it would be does not load.

mod read;
mod write;

use super::handler;
use crate::control::call::Call;
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;

pub use read::{configure, ConfigGetRequest, ConfigGetResult, ConfigKey};
pub use write::{ConfigChanged, ConfigResetRequest, ConfigSetRequest, ConfigSetResult};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "config.get",
            Scope::Admin,
            "The mixer's settings: each key's value in the config file, its default, when a \
             change to it takes effect, and which keys are waiting for a restart. Secrets \
             say only whether one is set.",
            handler(read::get),
        )
        .params(schema_of::<ConfigGetRequest>)
        .result(schema_of::<ConfigGetResult>)
        .mutating(false),
    );

    reg.register(
        MethodDef::new(
            "config.schema",
            Scope::Read,
            "Every setting config.set takes, as one JSON Schema: type, title, description, \
             default, range or choices, and x-gmx-applies (live, next_source or restart).",
            handler(read::schema),
        )
        .result(any_object),
    );

    reg.register(
        MethodDef::new(
            "config.set",
            Scope::Admin,
            "Change settings in the config file, keeping its comments. Every value is checked \
             first and nothing is written unless all of them fit. Live keys take effect at \
             once; the answer says which wait for the next source or a restart.",
            handler(write::set),
        )
        .params(schema_of::<ConfigSetRequest>)
        .result(schema_of::<ConfigSetResult>)
        .destructive(),
    );

    reg.register(
        MethodDef::new(
            "config.reset",
            Scope::Admin,
            "Put settings back to their defaults by taking them out of the config file. \
             Answers like config.set.",
            handler(write::reset),
        )
        .params(schema_of::<ConfigResetRequest>)
        .result(schema_of::<ConfigSetResult>)
        .destructive(),
    );
}
