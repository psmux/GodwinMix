//! Which NVIDIA card a channel's conversion runs on.
//!
//! A rendition names a card (`gpu = 1`), asks for `auto`, or says nothing,
//! which is `[hardware] gpus`: the first card, or `spread` as `auto` is.
//! `auto` puts the channel on the card with the least work when it is first
//! planned, and it stays there for as long as it runs: a move restarts its
//! encode, which breaks its HLS output's run of segments. Only
//! `gpu.rebalance` moves a running channel, and then the fewest that even
//! the cards out.
//!
//! The work on a card is what the governor holds there (every ticket on
//! `nvidia` or `nvidia:1`, the channels' encodes and anything else), plus
//! an estimate for each channel put on it that holds nothing yet because
//! its input has not arrived. Fifty channels starting at once are dealt out
//! evenly that way, before the first of them is live.

use std::collections::BTreeMap;

use godwinmix_govern::Governor;
use godwinmix_protocol::rendition::GpuPick;

use super::cards::{device_index, Cards};
use super::plan::Want;

/// What a channel's renditions ask of the cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Nothing: the first card, as before there were several.
    First,
    Auto,
    On(u32),
}

/// The first rendition that names a card wins; one channel's renditions
/// share one decode, so they share one card.
pub fn ask(wants: &[Want], spread: bool) -> Ask {
    let picks: Vec<GpuPick> = wants.iter().filter_map(|w| w.request.video.as_ref()?.gpu).collect();
    if let Some(n) = picks.iter().find_map(|p| p.index()) {
        return Ask::On(n);
    }
    if !picks.is_empty() || spread {
        return Ask::Auto;
    }
    Ask::First
}

/// Where a channel is, and whether `auto` put it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub card: u32,
    pub auto: bool,
}

/// Work on each card: thousandths of its throughput held, and sessions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Work {
    pub millis: u64,
    pub sessions: u64,
}

/// What the governor holds on each card.
pub fn held(gov: &Governor, cards: &[u32]) -> BTreeMap<u32, Work> {
    let mut out: BTreeMap<u32, Work> = cards.iter().map(|c| (*c, Work::default())).collect();
    for h in gov.held() {
        let Some(card) = h.device.as_deref().and_then(device_index) else { continue };
        if let Some(w) = out.get_mut(&card) {
            w.millis += u64::from(h.cost.device_millis);
            w.sessions += u64::from(h.cost.device_sessions);
        }
    }
    out
}

/// The card with the least work, counting `waiting` (cards of channels that
/// hold nothing yet) at the average a running channel holds.
pub fn least(cards: &Cards, work: &BTreeMap<u32, Work>, running: usize, waiting: &[u32]) -> u32 {
    let total: Work = work.values().fold(Work::default(), |a, w| Work { millis: a.millis + w.millis, sessions: a.sessions + w.sessions });
    let n = running.max(1) as u64;
    let each = Work { millis: (total.millis / n).max(1), sessions: (total.sessions / n).max(1) };
    let score = |c: u32| {
        let w = work.get(&c).copied().unwrap_or_default();
        let k = waiting.iter().filter(|x| **x == c).count() as u64;
        (w.millis + k * each.millis, w.sessions + k * each.sessions, c)
    };
    cards.list.iter().copied().min_by_key(|c| score(*c)).unwrap_or(0)
}

/// One channel as a rebalance sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub card: u32,
    /// What it holds on its card; at least 1, so an idle one still counts.
    pub weight: u64,
    /// May be moved: placed by `auto` and asked about.
    pub movable: bool,
}

/// The fewest moves that even the cards out: while moving one movable
/// channel from the busiest card to the idlest makes the gap smaller, move
/// the one whose weight is nearest half the gap. Answers (id, from, to).
pub fn rebalance(items: &[Item], cards: &[u32], other: &BTreeMap<u32, u64>) -> Vec<(String, u32, u32)> {
    let mut on: BTreeMap<String, u32> = items.iter().map(|i| (i.id.clone(), i.card)).collect();
    let mut moves: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    if cards.len() < 2 {
        return Vec::new();
    }
    for _ in 0..items.len() * 2 {
        let load = |c: u32, on: &BTreeMap<String, u32>| -> u64 {
            other.get(&c).copied().unwrap_or(0) + items.iter().filter(|i| on.get(&i.id) == Some(&c)).map(|i| i.weight).sum::<u64>()
        };
        let busiest = *cards.iter().max_by_key(|c| (load(**c, &on), std::cmp::Reverse(**c))).expect("two cards");
        let idlest = *cards.iter().min_by_key(|c| (load(**c, &on), **c)).expect("two cards");
        let gap = load(busiest, &on) - load(idlest, &on);
        let best = items
            .iter()
            .filter(|i| i.movable && on.get(&i.id) == Some(&busiest) && i.weight < gap)
            .min_by_key(|i| (i.weight.abs_diff(gap / 2), i.id.clone()));
        let Some(item) = best else { break };
        on.insert(item.id.clone(), idlest);
        let from = moves.get(&item.id).map_or(item.card, |m| m.0);
        moves.insert(item.id.clone(), (from, idlest));
    }
    moves.into_iter().filter(|(_, (from, to))| from != to).map(|(id, (from, to))| (id, from, to)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, card: u32, weight: u64) -> Item {
        Item { id: id.into(), card, weight, movable: true }
    }

    #[test]
    fn fifty_one_channels_on_one_card_are_split_with_the_fewest_moves() {
        let items: Vec<Item> = (0..51).map(|i| item(&format!("ch{i:02}"), 0, 30)).collect();
        let moves = rebalance(&items, &[0, 1], &BTreeMap::new());
        assert_eq!(moves.len(), 25, "{moves:?}");
        assert!(moves.iter().all(|(_, from, to)| *from == 0 && *to == 1));
    }

    #[test]
    fn even_cards_are_left_alone_and_a_pinned_channel_never_moves() {
        let even = [item("a", 0, 30), item("b", 1, 30)];
        assert!(rebalance(&even, &[0, 1], &BTreeMap::new()).is_empty());
        let pinned = [Item { movable: false, ..item("a", 0, 30) }, Item { movable: false, ..item("b", 0, 30) }];
        assert!(rebalance(&pinned, &[0, 1], &BTreeMap::new()).is_empty());
        // Work that is not a channel (a programme encode) counts as load.
        let moves = rebalance(&[item("a", 1, 30), item("b", 1, 30)], &[0, 1], &BTreeMap::from([(0, 0)]));
        assert_eq!(moves.len(), 1);
        assert!(rebalance(&[item("a", 1, 30), item("b", 1, 30)], &[0, 1], &BTreeMap::from([(0, 60)])).is_empty());
    }
}
