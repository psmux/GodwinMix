//! Live wins: when the machine runs short on air, a channel's transcodes
//! go before the programme does, and come back when there is room again.
//!
//! The governor decides the order and does nothing itself
//! (`Governor::shed`). This acts on the steps that name a ticket a channel
//! holds: the destinations that node served are marked shed with the
//! governor's sentence, left out of the next plan, and so out of the
//! listener's table. A mark is lifted once the machine has had
//! [`HOLD_OFF`] with nothing to shed, and the destination is planned and
//! admitted again like any other; if the room is still not there, the
//! governor refuses it and says so.

use std::time::{Duration, Instant};

use godwinmix_govern::ShedAction;

use super::outcome::Outcome;
use super::Transcode;

/// How long a shed destination waits before it is tried again.
pub const HOLD_OFF: Duration = Duration::from_secs(30);
/// How often a destination the governor refused is asked about again.
pub const RETRY: Duration = Duration::from_secs(10);

/// What one look at the machine found.
#[derive(Debug, Default, PartialEq)]
pub struct Tick {
    /// Replan and hand the listener its table again.
    pub replan: bool,
    /// Alerts to raise, one per destination shed.
    pub alerts: Vec<String>,
}

impl Transcode {
    /// Whether there is anything to watch: a ticket held, a mark to lift, or
    /// a refusal to ask about again.
    pub fn busy(&self) -> bool {
        let state = self.state.lock();
        !state.shed.is_empty()
            || state.channels.values().any(|c| {
                !c.held.is_empty() || c.outcomes.values().any(|o| matches!(o, Outcome::Refused(no) if no.code == "governor"))
            })
    }

    /// Whether the governor has turned a destination away, which is worth
    /// asking about again now and then.
    pub fn refused(&self) -> bool {
        let state = self.state.lock();
        state.channels.values().any(|c| c.outcomes.values().any(|o| matches!(o, Outcome::Refused(no) if no.code == "governor")))
    }

    /// One look: shed what the governor says to, lift marks that have
    /// waited long enough, and ask again about refusals now and then.
    pub fn tick(&self) -> Tick {
        self.tick_at(Instant::now())
    }

    /// The same, as if it were `now`.
    pub fn tick_at(&self, now: Instant) -> Tick {
        let steps = self.governor.get().shed();
        let mut out = Tick::default();
        let mut state = self.state.lock();
        for step in steps.iter().filter(|s| s.action == ShedAction::Drop) {
            let mut marks = Vec::new();
            for (channel, ch) in &state.channels {
                for h in ch.held.values().filter(|h| h.ticket.id() == step.ticket) {
                    marks.extend(h.node.serves.iter().map(|d| (channel.clone(), d.clone())));
                }
            }
            for key in marks {
                let why = format!("{} Destination `{}` of channel `{}` comes back when there is room.", step.why, key.1, key.0);
                out.alerts.push(why.clone());
                state.shed.insert(key, (why, now));
                out.replan = true;
            }
        }
        if steps.is_empty() {
            let before = state.shed.len();
            state.shed.retain(|_, (_, at)| now.duration_since(*at) < HOLD_OFF);
            out.replan |= state.shed.len() != before;
        }
        let refused = state
            .channels
            .values()
            .any(|c| c.outcomes.values().any(|o| matches!(o, Outcome::Refused(no) if no.code == "governor")));
        if refused && state.retried.is_none_or(|at| now.duration_since(at) >= RETRY) {
            state.retried = Some(now);
            out.replan = true;
        }
        out
    }
}
