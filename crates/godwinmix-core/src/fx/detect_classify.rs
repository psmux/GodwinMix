//! Deciding what a measured file is, by the rules at the top of `detect`.

use super::{Measured, Stat};

/// What a measured file is.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub kind: godwinmix_protocol::fx::FxKind,
    pub blend: godwinmix_protocol::fx::FxBlend,
    pub cut_at_ms: Option<u64>,
    pub coverage: Option<f64>,
    pub transition: bool,
    pub effect: bool,
}

/// Decide what `m` is, by the rules at the top of this file.
pub fn classify(m: &Measured) -> Verdict {
    use godwinmix_protocol::fx::{FxBlend, FxKind};
    let grey = m.stats.iter().all(|s| s.colour < 0.03);
    if m.still() || (grey && !m.alpha && m.stats.iter().all(|s| s.black < 0.7)) {
        return Verdict { kind: FxKind::Matte, blend: FxBlend::Normal, cut_at_ms: None, coverage: None, transition: true, effect: false };
    }
    let peak = |f: &dyn Fn(&Stat) -> f64| -> (usize, f64) {
        m.stats.iter().enumerate().map(|(i, s)| (i, f(s))).fold((0, -1.0), |a, b| if b.1 > a.1 { b } else { a })
    };
    if m.alpha {
        let (best, cover) = peak(&|s| s.solid);
        let mid = plateau(&m.stats, best, &|s| s.solid);
        return Verdict { kind: FxKind::Stinger, blend: FxBlend::Normal, cut_at_ms: Some(m.at_ms(mid)), coverage: Some(three(cover)), transition: true, effect: true };
    }
    let dark = m.stats.iter().map(|s| s.black).sum::<f64>() / m.stats.len() as f64;
    let ends_dark = [m.stats.first(), m.stats.last()].iter().flatten().any(|s| s.black >= 0.6);
    if dark > 0.3 || ends_dark {
        let (whitest, white) = peak(&|s| s.white);
        let best = if white > 0.0 { plateau(&m.stats, whitest, &|s| s.white) } else { peak(&|s| 1.0 - s.black).0 };
        return Verdict { kind: FxKind::Overlay, blend: FxBlend::Screen, cut_at_ms: Some(m.at_ms(best)), coverage: Some(three(white)), transition: white >= 0.5, effect: true };
    }
    Verdict { kind: FxKind::Stinger, blend: FxBlend::Normal, cut_at_ms: Some(m.duration_ms / 2), coverage: Some(1.0), transition: true, effect: false }
}

/// The middle of the run of frames as covered as frame `first`, the most
/// covered: a cut there has the most room either side when a frame lands
/// early or late.
fn plateau(stats: &[Stat], first: usize, f: &dyn Fn(&Stat) -> f64) -> usize {
    let top = f(&stats[first]) - 0.02;
    let last = stats[first..].iter().position(|s| f(s) < top).map(|n| first + n - 1).unwrap_or(stats.len() - 1);
    (first + last) / 2
}

/// A share to three places: what a person reads, and what is written down.
fn three(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}
