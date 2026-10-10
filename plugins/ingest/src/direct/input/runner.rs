//! The life of a GStreamer backed input: connect, carry tags, count, and
//! when it fails or ends, wait a little and connect again, until stopped.
//!
//! Everything here runs on the input's own thread. The bus is popped with a
//! timeout, so a stop is seen within a quarter of a second, and the numbers
//! go out once a second whether or not anything arrives.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;

use super::super::{Sink, StopSignal};
use super::loss::Loss;
use super::outlet::Shared;
use super::pads::Pads;
use super::stats::{InputStats, State};
use crate::tagger;

/// What each kind of input builds every time it connects.
pub trait Plan: Send {
    /// Add the transport to `pipeline` and hand what it produces to
    /// `pads::parse_into`. Answers where its loss is counted.
    fn build(&mut self, pipeline: &gst::Pipeline, pads: &Arc<Pads>) -> Result<Loss, String>;
    /// The address, as a person would write it, for messages.
    fn address(&self) -> String;
    fn program(&self) -> Option<u16> {
        None
    }
    /// A file: start again at its end.
    fn looped(&self) -> bool {
        false
    }
    /// Hand tags on at the clock's pace rather than as they come: a file,
    /// and HLS or DASH, which arrive a segment at a time.
    fn paced(&self) -> bool {
        self.looped()
    }
    /// Reconnect after this long with no frame. `None` keeps waiting, for a
    /// socket that stays open whether anyone sends to it or not.
    fn stall_ms(&self) -> Option<u64> {
        Some(10_000)
    }
}

/// A frame within this long, and the input is live.
const LIVE_MS: u64 = 3_000;
const POP: gst::ClockTime = gst::ClockTime::from_mseconds(250);

enum Ended {
    Stopped,
    Again,
    Failed(String),
}

pub fn run(mut plan: impl Plan, sink: Sink, stop: StopSignal) {
    let out = Shared::new(sink);
    let mut stats = InputStats { program: plan.program(), ..InputStats::default() };
    let mut totals = (0u64, 0u64);
    let mut wait = Duration::from_secs(1);
    while !stop.is_stopped() {
        out.new_session();
        let started = Instant::now();
        let why = match session(&mut plan, &out, &mut stats, &mut totals, &stop) {
            Ended::Stopped => break,
            Ended::Again if started.elapsed() > Duration::from_secs(1) => continue,
            Ended::Again => format!(
                "{} ended within a second of starting, so there is nothing to loop. Check that it plays in a media player.",
                plan.address()
            ),
            Ended::Failed(why) => why,
        };
        // A connection that held for a while starts its waits again.
        if started.elapsed() > Duration::from_secs(10) {
            wait = Duration::from_secs(1);
        }
        stats.state = State::Retrying;
        stats.error = Some(why);
        out.publish(&mut stats);
        if stop.wait(wait) {
            break;
        }
        wait = (wait * 2).min(Duration::from_secs(10));
    }
}

fn session(plan: &mut impl Plan, out: &Shared, stats: &mut InputStats, totals: &mut (u64, u64), stop: &StopSignal) -> Ended {
    let pipeline = gst::Pipeline::with_name(&format!("direct-in-{}", std::process::id()));
    let to = tagger::share(out.inlet());
    let pads = Pads::new(to.clone(), plan.paced(), plan.program());
    let ended = match plan.build(&pipeline, &pads) {
        Ok(loss) => {
            let ended = match pipeline.set_state(gst::State::Playing) {
                Ok(_) => watch(plan, &pipeline, out, &pads, &loss, stats, *totals, stop),
                Err(_) => Ended::Failed(bus_error(&pipeline).unwrap_or_else(|| format!("{} would not start", plan.address()))),
            };
            let (cc, lost) = loss.now();
            *totals = (totals.0 + cc, totals.1 + lost);
            ended
        }
        Err(e) => Ended::Failed(e),
    };
    tagger::close(&to);
    let _ = pipeline.set_state(gst::State::Null);
    ended
}

#[allow(clippy::too_many_arguments)]
fn watch(plan: &impl Plan, pipeline: &gst::Pipeline, out: &Shared, pads: &Pads, loss: &Loss, stats: &mut InputStats, totals: (u64, u64), stop: &StopSignal) -> Ended {
    let Some(bus) = pipeline.bus() else { return Ended::Failed("the pipeline has no bus".into()) };
    let started = Instant::now();
    let mut tick = Instant::now();
    loop {
        if stop.is_stopped() {
            return Ended::Stopped;
        }
        if let Some(msg) = bus.timed_pop_filtered(POP, &[gst::MessageType::Error, gst::MessageType::Eos]) {
            return match msg.view() {
                gst::MessageView::Eos(_) if plan.looped() => Ended::Again,
                gst::MessageView::Eos(_) => Ended::Failed(format!("{} ended the stream", plan.address())),
                gst::MessageView::Error(e) => Ended::Failed(format!("{}: {}", plan.address(), e.error())),
                _ => continue,
            };
        }
        if tick.elapsed() < Duration::from_secs(1) {
            continue;
        }
        tick = Instant::now();
        eprintln!("DIAG watch tick");
        let quiet = out.quiet_ms();
        eprintln!("DIAG watch quiet read");
        let notes = pads.notes();
        eprintln!("DIAG watch notes read");
        (stats.state, stats.error) = match quiet {
            Some(q) if q < LIVE_MS => (State::Live, (!notes.is_empty()).then(|| format!("left out: {notes}"))),
            Some(q) => (State::Retrying, Some(format!("nothing from {} for {} s", plan.address(), q / 1000))),
            None => (State::Connecting, (!notes.is_empty()).then(|| format!("{} carries {notes}", plan.address()))),
        };
        out.publish_with(stats, |s| loss.fill(s, totals));
        eprintln!("DIAG stats fps={} kbps={} key={:?} quiet={:?} cc={} lost={} err={:?}", stats.fps, stats.kbps, stats.keyframe_ms, stats.last_frame_ms, stats.cc_errors, stats.packets_lost, stats.error);
        let silent = quiet.unwrap_or(started.elapsed().as_millis() as u64);
        if let Some(limit) = plan.stall_ms().filter(|&l| silent > l) {
            return Ended::Failed(format!("nothing arrived from {} for {} s", plan.address(), limit / 1000));
        }
    }
}

fn bus_error(pipeline: &gst::Pipeline) -> Option<String> {
    let msg = pipeline.bus()?.timed_pop_filtered(gst::ClockTime::from_mseconds(200), &[gst::MessageType::Error])?;
    match msg.view() {
        gst::MessageView::Error(e) => Some(e.error().to_string()),
        _ => None,
    }
}
