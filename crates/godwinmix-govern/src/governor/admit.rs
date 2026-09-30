//! Admission: does this fit, and if not, what would.

use super::Governor;
use crate::advice::{self, Advice};
use crate::headroom::short;
use crate::shed::{Encode, Held, Kind};
use crate::ticket::Ticket;
use godwinmix_protocol::rendition::{Cost, EncoderSlot, VideoShape};
use serde_json::json;
use std::sync::Arc;

/// Something asking to start.
#[derive(Debug, Clone, Default)]
pub struct Claim {
    /// What it is, in words, for the refusal and the shed alert:
    /// `the 1080p60 HEVC rendition for youtube`.
    pub what: String,
    pub cost: Cost,
    pub kind: Kind,
    /// The hardware device it runs on. Filled in from the profile when the
    /// cost uses a device and this is left empty.
    pub device: Option<String>,
    pub encode: Option<Encode>,
}

impl Claim {
    pub fn new(what: &str, cost: Cost) -> Claim {
        Claim { what: what.to_string(), cost, ..Claim::default() }
    }

    pub fn kind(mut self, kind: Kind) -> Claim {
        self.kind = kind;
        self
    }

    pub fn on(mut self, device: &str) -> Claim {
        self.device = Some(device.to_string());
        self
    }
}

/// The answer.
#[derive(Debug)]
pub enum Admit {
    /// Started. The share is held until the ticket is dropped.
    Granted(Ticket),
    /// Not started: `need` does not fit in `have`, and `advice` says what would.
    Refused { need: Cost, have: Cost, advice: Advice },
}

impl Admit {
    pub fn granted(self) -> Option<Ticket> {
        match self {
            Admit::Granted(t) => Some(t),
            Admit::Refused { .. } => None,
        }
    }

    /// The `data` object for a protocol error carrying this refusal.
    pub fn error_data(&self) -> Option<serde_json::Value> {
        match self {
            Admit::Granted(_) => None,
            Admit::Refused { need, have, advice } => Some(json!({ "need": need, "have": have, "short": advice.short, "fits": advice.fits })),
        }
    }
}

impl Governor {
    /// Ask for `cost`. Granted when it fits in what is left after everything
    /// running and the reserve; refused with advice otherwise.
    pub fn admit(&self, cost: Cost, what: &str) -> Admit {
        self.admit_claim(Claim::new(what, cost))
    }

    /// Ask for an encode of `shape` on `slot`, priced from the profile. A
    /// software encoder whose configured preset does not fit is offered at
    /// the slowest preset that does; `Ticket::preset` says which.
    pub fn admit_encode(&self, slot: &EncoderSlot, shape: &VideoShape, what: &str, kind: Kind) -> Admit {
        let profile = self.profile();
        let mut encode = Encode { slot: slot.clone(), shape: *shape, preset: profile.configured_preset(slot) };
        let mut cost = profile.encode_cost(slot, shape);
        if !slot.hardware {
            let room = self.headroom(None).cpu_millicores;
            if cost.cpu_millicores > room {
                if let Some(p) = profile.preset_that_fits(slot, shape, room) {
                    cost = profile.encode_cost_at(slot, shape, Some(&p));
                    encode.preset = Some(p);
                }
            }
        }
        let mut claim = Claim::new(what, cost).kind(kind);
        claim.device = slot.device.clone();
        claim.encode = Some(encode);
        self.admit_claim(claim)
    }

    /// The general form: a claim with its kind and device stated.
    pub fn admit_claim(&self, mut claim: Claim) -> Admit {
        let profile = self.profile();
        if claim.device.is_none() && (claim.cost.device_millis > 0 || claim.cost.device_sessions > 0) {
            claim.device = profile.devices().into_iter().next();
        }
        let load = self.load();
        let mut book = self.inner.book.lock();
        let have = self.have(&book, &load, claim.device.as_deref());
        if short(&claim.cost, &have).is_empty() {
            book.next += 1;
            let id = book.next;
            let preset = claim.encode.as_ref().and_then(|e| e.preset.clone());
            let held = Held { id, what: claim.what, cost: claim.cost, kind: claim.kind, device: claim.device, encode: claim.encode };
            book.held.insert(id, held);
            return Admit::Granted(Ticket::new(id, claim.cost, preset, Arc::downgrade(&self.inner)));
        }
        drop(book);
        let advice = advice::advise(&claim.what, &claim.cost, &have, &profile, |d| {
            let book = self.inner.book.lock();
            self.have(&book, &load, d)
        });
        Admit::Refused { need: claim.cost, have, advice }
    }
}
