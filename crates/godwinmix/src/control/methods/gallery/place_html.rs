//! How a gallery item comes on and goes off when it is placed.

use godwinmix_core::gallery::{place, Entry};
use godwinmix_protocol::gallery::Zone;
use serde_json::{json, Value};

/// An HTML template plays its own way in, and its item is held on the canvas
/// for its way out (`out_ms`); anything else moves the zone's way.
pub(super) fn enter_of(e: &Entry, zone: Zone) -> Value {
    match html_out_ms(e) {
        Some(_) => json!({"type": "cut"}),
        None => place::enter(zone),
    }
}

pub(super) fn exit_of(e: &Entry, zone: Zone) -> Value {
    match html_out_ms(e) {
        Some(ms) => json!({"type": "hold", "duration_ms": ms, "on_take": true}),
        None => place::exit(zone),
    }
}

fn html_out_ms(e: &Entry) -> Option<u32> {
    let page = e.file().filter(|_| e.item.uri.as_deref().is_some_and(|u| u.starts_with("html:")))?;
    godwinmix_core::gallery::entry::html_template(&page).map(|t| t.info.out_ms.unwrap_or(0))
}
