//! Live wins: when the machine runs short on air, what goes first.
//!
//! The order is the plan's: thumbnails and previews, then the lowest rung of
//! a ladder upward, then a faster software preset (lower rungs first, the
//! programme last). The programme encode and the top rung of a live output
//! are never dropped; at worst they get a faster preset. Work that did not
//! say what it is (`Kind::Other`) is never dropped either, only sped up.

use crate::profile::Profile;
use godwinmix_protocol::rendition::{Cost, EncoderSlot, VideoShape};
use serde::Serialize;

/// What a piece of work is, which decides when it is shed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum Kind {
    Thumbnail,
    Preview,
    /// One rung of a ladder; 0 is the top and is never dropped.
    Rung { index: u8 },
    Programme,
    #[default]
    Other,
}

/// The encode a ticket runs, so a faster preset can be priced.
#[derive(Debug, Clone, PartialEq)]
pub struct Encode {
    pub slot: EncoderSlot,
    pub shape: VideoShape,
    pub preset: Option<String>,
}

/// One ticket as the book holds it.
#[derive(Debug, Clone)]
pub struct Held {
    pub id: u64,
    pub what: String,
    pub cost: Cost,
    pub kind: Kind,
    /// The hardware device it holds a share of, if any.
    pub device: Option<String>,
    pub encode: Option<Encode>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "action")]
pub enum ShedAction {
    /// Stop it; the ticket is dropped with it.
    Drop,
    /// Keep it, on a faster preset. `cost` is what it costs then; pass it to
    /// `Ticket::lower` once the encoder has changed.
    LowerPreset { to: String, cost: Cost },
}

/// One thing to do, in order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShedStep {
    pub ticket: u64,
    pub what: String,
    pub action: ShedAction,
    /// What doing it gives back.
    pub frees: Cost,
    /// The alert text: what is dropped and why.
    pub why: String,
}

/// The steps that free at least `excess` thousandths of a core, in order.
/// Fewer when there is not that much that may be shed.
pub fn plan(held: &[Held], excess: u32, profile: &Profile) -> Vec<ShedStep> {
    let over = format!("{:.1}", f64::from(excess) / 1000.0);
    let mut steps = Vec::new();
    let mut freed = 0u32;
    for h in drop_order(held) {
        if freed >= excess {
            break;
        }
        let why = format!("Stopped {} ({}) to keep what is on air: the machine was {over} cores over what it can hold.", h.what, label(h.kind));
        freed += h.cost.cpu_millicores;
        steps.push(ShedStep { ticket: h.id, what: h.what.clone(), action: ShedAction::Drop, frees: h.cost, why });
    }
    for h in preset_order(held) {
        if freed >= excess {
            break;
        }
        let Some(e) = &h.encode else { continue };
        if steps.iter().any(|s| s.ticket == h.id) {
            continue;
        }
        let Some(to) = profile.faster_preset(&e.slot, e.preset.as_deref()) else { continue };
        let cost = profile.encode_cost_at(&e.slot, &e.shape, Some(&to));
        let frees = Cost { cpu_millicores: h.cost.cpu_millicores.saturating_sub(cost.cpu_millicores), ..Cost::default() };
        if frees.cpu_millicores == 0 {
            continue;
        }
        let why = format!("Moved {} to the {to} preset, which looks a little softer, because the machine was {over} cores over what it can hold.", h.what);
        freed += frees.cpu_millicores;
        steps.push(ShedStep { ticket: h.id, what: h.what.clone(), action: ShedAction::LowerPreset { to, cost }, frees, why });
    }
    steps
}

fn drop_order(held: &[Held]) -> Vec<&Held> {
    let mut v: Vec<&Held> = held
        .iter()
        .filter(|h| matches!(h.kind, Kind::Thumbnail | Kind::Preview) || matches!(h.kind, Kind::Rung { index } if index > 0))
        .collect();
    // Thumbnails, then previews, then rungs from the bottom; the dearest
    // first within each, so fewer things stop.
    v.sort_by_key(|h| (rank(h.kind), std::cmp::Reverse(h.cost.cpu_millicores)));
    v
}

fn preset_order(held: &[Held]) -> Vec<&Held> {
    let mut v: Vec<&Held> = held.iter().filter(|h| h.encode.as_ref().is_some_and(|e| !e.slot.hardware)).collect();
    v.sort_by_key(|h| (rank(h.kind), std::cmp::Reverse(h.cost.cpu_millicores)));
    v
}

/// Lower sheds sooner.
fn rank(k: Kind) -> (u8, u8) {
    match k {
        Kind::Thumbnail => (0, 0),
        Kind::Preview => (1, 0),
        Kind::Rung { index } => (2, u8::MAX - index),
        Kind::Other => (3, 0),
        Kind::Programme => (4, 0),
    }
}

fn label(k: Kind) -> &'static str {
    match k {
        Kind::Thumbnail => "a thumbnail",
        Kind::Preview => "a preview",
        Kind::Rung { .. } => "a lower rung of its ladder",
        Kind::Programme => "the programme",
        Kind::Other => "other work",
    }
}

#[cfg(test)]
mod tests;
