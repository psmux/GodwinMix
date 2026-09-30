//! One governor for several processes.
//!
//! A station runs each show as a process of its own, and the machine still
//! has one budget (`dev/plans/shows-and-renditions.md`, Decision 3). A show's
//! governor is given a [`Remote`]: every claim goes to the station as an
//! [`Ask`], the station runs it through the real governor with
//! [`Governor::answer`], keeps the ticket, and says what it decided. The
//! show's [`Ticket`] carries the station's number, and dropping it tells the
//! station to drop the real one.
//!
//! When the station cannot be reached, `ask` answers `None` and the show
//! admits against its own book, as a process with no station does. A show
//! whose station has gone is on its way down in any case.

use crate::advice::{Advice, Fit};
use crate::governor::{Admit, Claim, Governor};
use crate::shed::{Encode, Kind};
use crate::ticket::Ticket;
use godwinmix_protocol::rendition::{Cost, EncoderSlot, VideoShape};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A claim as it crosses to the station.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    pub what: String,
    pub cost: Cost,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Present for an encode, which the station prices from its own
    /// calibration: a show never measures the machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode: Option<AskEncode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskEncode {
    pub slot: EncoderSlot,
    pub shape: VideoShape,
}

/// What the station decided.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "lowercase")]
pub enum Answer {
    Granted { ticket: u64, cost: Cost, preset: Option<String> },
    Refused { need: Cost, have: Cost, text: String, short: Vec<String>, fits: Vec<Fit> },
}

/// The show's side of the link to its station.
pub trait Remote: Send + Sync {
    /// Ask the station. `None` when it could not be reached in time.
    fn ask(&self, ask: &Ask) -> Option<Answer>;
    /// The station's ticket is no longer needed. Must not block: it is
    /// called from a ticket's drop, which can be on any thread.
    fn release(&self, ticket: u64);
}

impl Governor {
    /// Send every claim from now on to `remote`. Called once, before the
    /// first claim, by a show that runs under a station.
    pub fn set_remote(&self, remote: Arc<dyn Remote>) {
        *self.inner.remote.write() = Some(remote);
    }

    pub(crate) fn remote(&self) -> Option<Arc<dyn Remote>> {
        self.inner.remote.read().clone()
    }

    /// The station's half: admit an ask from a show against this governor.
    /// The ticket is the caller's to keep for as long as the show holds it.
    pub fn answer(&self, ask: Ask) -> (Answer, Option<Ticket>) {
        let admitted = match &ask.encode {
            Some(e) => self.admit_encode(&e.slot, &e.shape, &ask.what, ask.kind),
            None => {
                let mut claim = Claim::new(&ask.what, ask.cost).kind(ask.kind);
                claim.device = ask.device.clone();
                self.admit_claim(claim)
            }
        };
        match admitted {
            Admit::Granted(t) => {
                let answer = Answer::Granted { ticket: t.id(), cost: t.cost(), preset: t.preset().map(str::to_string) };
                (answer, Some(t))
            }
            Admit::Refused { need, have, advice } => {
                let short = advice.short.iter().map(|s| s.to_string()).collect();
                (Answer::Refused { need, have, text: advice.text, short, fits: advice.fits }, None)
            }
        }
    }
}

/// The ask a claim makes of the station.
pub(crate) fn ask_of(claim: &Claim) -> Ask {
    Ask {
        what: claim.what.clone(),
        cost: claim.cost,
        kind: claim.kind,
        device: claim.device.clone(),
        encode: claim.encode.as_ref().map(|e: &Encode| AskEncode { slot: e.slot.clone(), shape: e.shape }),
    }
}

/// The station's answer, as the show's own governor would have given it.
pub(crate) fn admit_of(answer: Answer, governor: &Governor) -> Admit {
    match answer {
        Answer::Granted { ticket, cost, preset } => {
            Admit::Granted(Ticket::remote(ticket, cost, preset, Arc::downgrade(&governor.inner)))
        }
        Answer::Refused { need, have, text, short, fits } => {
            let short = short.iter().filter_map(|s| crate::headroom::short_name(s)).collect();
            Admit::Refused { need, have, advice: Advice { text, short, fits } }
        }
    }
}

#[cfg(test)]
mod tests;
