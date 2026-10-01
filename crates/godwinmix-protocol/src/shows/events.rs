//! The show events, as rows of the protocol's event table.

use super::{Health, Show};
use crate::method::schema_of;
use crate::protocol::EventDef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `event/show.changed`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowChanged {
    pub show: Show,
}

/// `event/show.removed`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowRemovedEvent {
    pub id: String,
}

/// The show events, as rows of the protocol's event table.
pub fn events() -> Vec<EventDef> {
    vec![
        EventDef {
            name: "show.changed",
            since: "1",
            summary: "A show was added, renamed, started, stopped, died or came back. \
                      Sent by the station to every client, whichever show it is \
                      looking at.",
            ext: None,
            legacy: None,
            payload: schema_of::<ShowChanged>,
        },
        EventDef {
            name: "show.removed",
            since: "1",
            summary: "A show was removed. Its process was stopped first.",
            ext: None,
            legacy: None,
            payload: schema_of::<ShowRemovedEvent>,
        },
        EventDef {
            name: "show.health",
            since: "1",
            summary: "A show's health changed state, or an alarm began or ended. Never sent \
                      for a number alone: read those with show.stats.",
            ext: None,
            legacy: None,
            payload: schema_of::<ShowHealthEvent>,
        },
    ]
}

/// `event/show.health`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowHealthEvent {
    pub id: String,
    pub health: Health,
}
