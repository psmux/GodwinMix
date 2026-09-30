//! The two rendition events, as rows of the protocol's event table.

use super::{RenditionPlanEvent, ShedNote};
use crate::method::schema_of;
use crate::protocol::EventDef;

pub fn events() -> Vec<EventDef> {
    vec![
        EventDef {
            name: "rendition.plan",
            since: "1",
            summary: "The programme's rendition plan changed: an output that asks for a \
                      rendition was added, changed or removed, or the governor stopped or \
                      brought back an encoder. plan is what rendition.plan answers.",
            ext: None,
            legacy: None,
            payload: schema_of::<RenditionPlanEvent>,
        },
        EventDef {
            name: "governor.shed",
            since: "1",
            summary: "The machine ran short while on air and the governor stopped \
                      something to keep what is on air whole: what it was and why. It is \
                      brought back by itself when there is room again.",
            ext: None,
            legacy: None,
            payload: schema_of::<ShedNote>,
        },
    ]
}
