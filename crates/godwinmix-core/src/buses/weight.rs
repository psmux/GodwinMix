//! How loud one source is on one bus, worked out per buffer.
//!
//! A bus reads each source after its fader and meter and before its mute (see
//! `send.rs`). What it does with that depends on its kind:
//!
//! | kind | a source's weight |
//! |---|---|
//! | sends | its send level, or 0 while it is muted |
//! | minus | what the programme gives it (audio follows video, take fades), 0 for the one left out |
//! | cue | 1.0 when it is marked cue, muted or not; while nothing is marked, the programme |
//!
//! "What the programme gives it" is the source's own `amix` pad volume, read
//! as it is, so a mix minus follows every take and crossfade without being
//! told about one. The weight is written to the bus mixer's pad for that
//! source from a probe on the buffer going to it, which runs on the bus's own
//! queue thread and never on the programme's.
//!
//! A change of weight is spread over a few buffers rather than jumped, so a
//! send moved from 0 to 1 does not click.

use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// The most a weight moves in one buffer. Ten millisecond buffers take a full
/// swing from 0 to 1 in about 80 ms.
const STEP: f64 = 0.125;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Sends,
    Minus(String),
    Cue,
}

/// What every link on one bus reads: its kind and its sends. Changed on the
/// mixer thread, read on the queue threads.
#[derive(Debug)]
pub struct Weights {
    pub mode: Mode,
    pub sends: BTreeMap<String, f64>,
}

pub type Shared = Arc<Mutex<Weights>>;

/// The sources marked cue, shared by every cue bus.
pub type CueMarks = Arc<Mutex<Vec<String>>>;

/// What one link needs to read the programme's side of its source.
pub struct Programme {
    pub source: String,
    /// The source's mute, after the point the bus reads from.
    pub amute: gst::Element,
    /// The source's pad on the programme mixer.
    pub apad: gst::Pad,
}

/// The weight `source` should have on a bus, now.
pub fn target(w: &Weights, cue: &[String], p: &Programme) -> f64 {
    let muted = || p.amute.property::<bool>("mute");
    let programme = || if muted() { 0.0 } else { p.apad.property::<f64>("volume") };
    match &w.mode {
        Mode::Sends => {
            let send = w.sends.get(&p.source).copied().unwrap_or(0.0);
            if send == 0.0 || muted() {
                0.0
            } else {
                send
            }
        }
        Mode::Minus(out) if out == &p.source => 0.0,
        Mode::Minus(_) => programme(),
        Mode::Cue if cue.is_empty() => programme(),
        Mode::Cue => {
            if cue.iter().any(|c| c == &p.source) {
                1.0
            } else {
                0.0
            }
        }
    }
}

/// One step from `have` towards `want`.
pub fn step(have: f64, want: f64) -> f64 {
    have + (want - have).clamp(-STEP, STEP)
}

/// Keep `sink`, the bus mixer's pad for this source, at the source's weight
/// and on the source's timeline. Installed on the tee pad that feeds it.
pub fn follow(pad: &gst::Pad, sink: &gst::Pad, shared: Shared, cue: CueMarks, p: Programme) {
    let last = Arc::new(AtomicU64::new(0f64.to_bits()));
    let (sink_b, p) = (sink.clone(), Arc::new(p));
    let p_ev = p.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        let want = {
            let marks = cue.lock();
            target(&shared.lock(), &marks, &p)
        };
        let have = f64::from_bits(last.load(Ordering::Relaxed));
        if (want - have).abs() > 1e-6 {
            let next = step(have, want);
            last.store(next.to_bits(), Ordering::Relaxed);
            sink_b.set_property("volume", next);
        }
        gst::PadProbeReturn::Ok
    });
    // The programme pad carries the shift that puts this source on the
    // programme's timeline (see `TimelineAligner`). It is decided upstream of
    // here, by the time a segment arrives, so the bus pad copies it before the
    // segment reaches it.
    let sink_e = sink.clone();
    pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        if let Some(gst::PadProbeData::Event(e)) = &info.data {
            if matches!(e.view(), gst::EventView::Segment(_)) {
                sink_e.set_offset(p_ev.apad.offset());
            }
        }
        gst::PadProbeReturn::Ok
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programme(source: &str, pad_volume: f64, muted: bool) -> Programme {
        gst::init().unwrap();
        let amute = gst::ElementFactory::make("volume").build().unwrap();
        amute.set_property("mute", muted);
        let mix = gst::ElementFactory::make("audiomixer").build().unwrap();
        let apad = mix.request_pad_simple("sink_%u").unwrap();
        apad.set_property("volume", pad_volume);
        Programme { source: source.into(), amute, apad }
    }

    fn weights(mode: Mode, sends: &[(&str, f64)]) -> Weights {
        Weights { mode, sends: sends.iter().map(|(k, v)| (k.to_string(), *v)).collect() }
    }

    #[test]
    fn a_send_is_its_level_and_nothing_while_muted() {
        let w = weights(Mode::Sends, &[("cam1", 0.5)]);
        assert_eq!(target(&w, &[], &programme("cam1", 0.0, false)), 0.5);
        assert_eq!(target(&w, &[], &programme("cam1", 1.0, true)), 0.0);
        assert_eq!(target(&w, &[], &programme("cam2", 1.0, false)), 0.0);
    }

    #[test]
    fn a_mix_minus_follows_the_programme_and_leaves_its_source_out() {
        let w = weights(Mode::Minus("guest-1".into()), &[]);
        assert_eq!(target(&w, &[], &programme("host", 0.7, false)), 0.7);
        assert_eq!(target(&w, &[], &programme("guest-1", 1.0, false)), 0.0);
    }

    #[test]
    fn cue_hears_what_is_marked_even_muted_and_the_programme_otherwise() {
        let w = weights(Mode::Cue, &[]);
        let marked = vec!["mic".to_string()];
        assert_eq!(target(&w, &marked, &programme("mic", 0.0, true)), 1.0);
        assert_eq!(target(&w, &marked, &programme("cam1", 1.0, false)), 0.0);
        assert_eq!(target(&w, &[], &programme("cam1", 1.0, false)), 1.0);
    }

    #[test]
    fn a_weight_moves_in_steps() {
        assert_eq!(step(0.0, 1.0), STEP);
        assert_eq!(step(1.0, 0.0), 1.0 - STEP);
        assert_eq!(step(0.5, 0.55), 0.55);
    }
}
