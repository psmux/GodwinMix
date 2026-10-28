//! The cards as the channels' renditions use them: which card a channel is
//! planned on (`place.rs` decides), where each one is, and the explicit
//! rebalance that moves running channels to even the cards out.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use godwinmix_core::config::GpuSpread;
use godwinmix_protocol::machine::{GpuMove, GpuRebalance};

use super::cards::device_index;
use super::place::{self, Item, Placed};
use super::plan::Want;
use super::state::{ChannelState, State};
use super::Transcode;

/// What a channel holds on NVIDIA cards, in thousandths of a card.
fn on_cards(ch: Option<&ChannelState>) -> u64 {
    ch.map_or(0, |ch| ch.held.values().filter(|h| device_index(&h.node.device).is_some()).map(|h| u64::from(h.ticket.cost().device_millis)).sum())
}

impl Transcode {
    /// `[hardware] gpus`: what a rendition that names no card does.
    pub fn set_gpus(&self, gpus: GpuSpread) {
        self.spread.store(gpus == GpuSpread::Spread, Ordering::Relaxed);
    }

    pub(super) fn spreads(&self) -> bool {
        self.spread.load(Ordering::Relaxed)
    }

    /// The NVIDIA cards a channel may be put on.
    pub fn gpus(&self) -> Vec<u32> {
        self.machine().cards.list.clone()
    }

    /// Each channel on a card, by channel id.
    pub fn placements(&self) -> BTreeMap<String, u32> {
        self.state.lock().placed.iter().map(|(id, p)| (id.clone(), p.card)).collect()
    }

    /// The card `id` is planned on: the one it asks for, the one `auto` put
    /// it on before, or, the first time, the one with the least work.
    pub(super) fn card_for(&self, state: &mut State, id: &str, wants: &[Want]) -> Option<u32> {
        let cards = &self.machine().cards;
        if cards.list.is_empty() {
            return None;
        }
        let ask = place::ask(wants, self.spreads());
        let placed = match ask {
            place::Ask::First => {
                state.placed.remove(id);
                return None;
            }
            place::Ask::On(n) if cards.has(n) => Placed { card: n, auto: false },
            _ => match state.placed.get(id) {
                Some(p) => Placed { auto: true, ..*p },
                None => Placed { card: self.least(state, id), auto: true },
            },
        };
        state.placed.insert(id.to_string(), placed);
        Some(placed.card)
    }

    /// The card with the least work for a channel not on one yet.
    fn least(&self, state: &State, id: &str) -> u32 {
        let cards = &self.machine().cards;
        let work = place::held(&self.governor.get(), &cards.list);
        let holds = |c: &str| state.channels.get(c).is_some_and(|ch| ch.held.values().any(|h| device_index(&h.node.device).is_some()));
        let running = state.placed.keys().filter(|c| holds(c)).count();
        let waiting: Vec<u32> = state.placed.iter().filter(|(c, _)| c.as_str() != id && !holds(c)).map(|(_, p)| p.card).collect();
        place::least(cards, &work, running, &waiting)
    }

    /// Let `auto` move these from now on, though a card was named for them
    /// until now: what a bulk spread does before it evens the cards out.
    pub fn unpin(&self, ids: &[String]) {
        let mut state = self.state.lock();
        for id in ids {
            if let Some(p) = state.placed.get_mut(id) {
                p.auto = true;
            }
        }
    }

    /// The card with the least work now, None with no NVIDIA card.
    pub(super) fn idlest(&self) -> Option<u32> {
        let cards = &self.machine().cards;
        (!cards.list.is_empty()).then(|| place::least(cards, &place::held(&self.governor.get(), &cards.list), 0, &[]))
    }

    /// Move the fewest channels placed by `auto` (of `only`, when given)
    /// that even the cards out. The caller replans, which restarts the
    /// conversion of each one that moved.
    pub fn rebalance(&self, only: Option<&[String]>, dry_run: bool) -> GpuRebalance {
        let cards = self.gpus();
        let total = place::held(&self.governor.get(), &cards);
        let mut state = self.state.lock();
        let items: Vec<Item> = state
            .placed
            .iter()
            .map(|(id, p)| Item {
                id: id.clone(),
                card: p.card,
                weight: on_cards(state.channels.get(id)).max(1),
                movable: p.auto && only.is_none_or(|o| o.contains(id)),
            })
            .collect();
        let mut other: BTreeMap<u32, u64> = total.iter().map(|(c, w)| (*c, w.millis)).collect();
        for (id, p) in &state.placed {
            if let Some(o) = other.get_mut(&p.card) {
                *o = o.saturating_sub(on_cards(state.channels.get(id)));
            }
        }
        let moves = place::rebalance(&items, &cards, &other);
        if !dry_run {
            for (id, _, to) in &moves {
                if let Some(p) = state.placed.get_mut(id) {
                    *p = Placed { card: *to, auto: true };
                }
            }
        }
        GpuRebalance {
            kept: (items.len() - moves.len()) as u32,
            moved: moves.into_iter().map(|(id, from, to)| GpuMove { id, from: Some(from), to }).collect(),
            dry_run,
            gpus: cards,
        }
    }
}
