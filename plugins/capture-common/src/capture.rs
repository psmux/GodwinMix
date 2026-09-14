//! Running a capture pipeline and knowing whether it is working.
//!
//! One `Capture` owns one `gst::Pipeline`, a thread that drains the bus, and a
//! pad probe that counts buffers. That is all the machinery the four capture
//! plugins need between them.
//!
//! Two rules it exists to keep:
//!
//! * Nothing here blocks a streaming thread. The pad probe is one relaxed
//!   atomic add. The bus thread wakes ten times a second and almost always
//!   goes straight back to sleep.
//! * `health` answers from numbers, not from hope. A pipeline in PLAYING that
//!   has produced nothing for two seconds is degraded, and says so before the
//!   core's own buffer watch has to guess.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::{Health, HealthState};

/// How long `start` waits for the pipeline to say it is playing, or to fail.
///
/// A camera that is not there, or one the operating system will not hand over
/// without a permission the operator has not granted, fails inside this and
/// the error names the device. Anything slower than this is reported as a
/// start that worked and a `health` that is not ok yet, because the contract
/// says `start` returns when the producer is running and not when the first
/// frame lands.
const READY_WITHIN: Duration = Duration::from_secs(2);

/// No buffer for this long, while playing, is degraded.
const STALL_AFTER: Duration = Duration::from_millis(2_000);

/// A running capture pipeline.
#[derive(Debug)]
pub struct Capture {
    pipeline: gst::Pipeline,
    buffers: Arc<AtomicU64>,
    /// Milliseconds since `started` at the most recent buffer.
    last_ms: Arc<AtomicU64>,
    fault: Arc<Mutex<Option<String>>>,
    stop_bus: Arc<AtomicBool>,
    bus_thread: Option<std::thread::JoinHandle<()>>,
    started: Instant,
}

/// Parse a `gst-launch` description into a pipeline, with an error a person
/// can act on.
///
/// Set `GMX_PIPELINE_DEBUG` in the environment and the description is printed
/// on stderr before it is parsed, which the core logs at `info` tagged with
/// the instance. It is the one line you want when a capture produces nothing:
/// paste it after `gst-launch-1.0` and the failure is in front of you.
pub fn build(description: &str) -> Result<gst::Pipeline, String> {
    crate::init()?;
    if std::env::var_os("GMX_PIPELINE_DEBUG").is_some() {
        eprintln!("gmx pipeline: {description}");
    }
    gst::parse::launch(description)
        .map_err(|e| {
            format!("could not build the capture pipeline: {e}. The pipeline was: {description}")
        })?
        .downcast::<gst::Pipeline>()
        .map_err(|_| "the description did not produce a pipeline".to_string())
}

impl Capture {
    /// Take the pipeline to PLAYING and start watching it.
    ///
    /// `count_on` names an element whose source pad gets the buffer counter,
    /// usually the queue in front of the sink. A pipeline with nothing worth
    /// counting passes `None` and `health` reports on the bus alone.
    pub fn start(
        pipeline: gst::Pipeline,
        count_on: Option<&str>,
        reporter: Option<Reporter>,
    ) -> Result<Capture, String> {
        let buffers = Arc::new(AtomicU64::new(0));
        let last_ms = Arc::new(AtomicU64::new(0));
        let started = Instant::now();
        if let Some(name) = count_on {
            attach_counter(&pipeline, name, &buffers, &last_ms, started)?;
        }

        let fault = Arc::new(Mutex::new(None));
        let stop_bus = Arc::new(AtomicBool::new(false));
        let bus_thread = spawn_bus_watch(&pipeline, &fault, &stop_bus, reporter);

        let mut capture = Capture {
            pipeline,
            buffers,
            last_ms,
            fault,
            stop_bus,
            bus_thread,
            started,
        };
        if let Err(e) = capture.play() {
            capture.stop();
            return Err(e);
        }
        Ok(capture)
    }

    fn play(&mut self) -> Result<(), String> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|_| self.why("the pipeline would not start"))?;
        // Blocks only until the state change finishes, which for a live
        // source is as soon as the device is open. A device that is not there
        // fails inside this and the bus carries the reason.
        let (result, _, _) = self.pipeline.state(gst::ClockTime::from_mseconds(
            READY_WITHIN.as_millis() as u64,
        ));
        match result {
            Ok(_) => Ok(()),
            // Still going. Not an error: `health` will say whether it arrived.
            Err(gst::StateChangeError) if self.fault().is_none() => Ok(()),
            Err(_) => Err(self.why("the pipeline would not reach playing")),
        }
    }

    /// The fault the bus reported, with a lead in, or the lead in alone.
    fn why(&self, lead: &str) -> String {
        match self.fault() {
            Some(detail) => format!("{lead}: {detail}"),
            None => format!("{lead}, and said nothing about why"),
        }
    }

    /// How many buffers have left the counted element.
    pub fn buffers(&self) -> u64 {
        self.buffers.load(Ordering::Relaxed)
    }

    /// The last error or end of stream the bus carried, if there was one.
    pub fn fault(&self) -> Option<String> {
        self.fault.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// What to answer `health` with. `what` names the thing being captured, so
    /// the message reads as a sentence: "no frames from the camera for 2.4 s".
    pub fn health(&self, what: &str) -> Health {
        if let Some(detail) = self.fault() {
            return Health::failing(format!(
                "{what}: {detail}. The supervisor will restart this process."
            ));
        }
        let elapsed = self.started.elapsed();
        let buffers = self.buffers();
        // A pipeline that is not playing is the answer to almost every "it
        // produced one buffer and stopped", and it is worth saying outright
        // rather than leaving the reader to guess from a count.
        let state = self.pipeline.current_state();
        if state != gst::State::Playing && elapsed > STALL_AFTER {
            return Health::degraded(format!(
                "{what} is in {state:?} rather than playing after {:.1} s, with {buffers} \
                 buffer(s) so far",
                elapsed.as_secs_f32()
            ));
        }
        if buffers == 0 {
            return if elapsed < STALL_AFTER {
                Health {
                    state: HealthState::Ok,
                    detail: Some(format!("{what} is starting")),
                    latency_ms: None,
                }
            } else {
                Health::degraded(format!(
                    "no data from {what} after {:.1} s. Check that nothing else on this \
                     machine has the device open.",
                    elapsed.as_secs_f32()
                ))
            };
        }
        let quiet =
            elapsed.saturating_sub(Duration::from_millis(self.last_ms.load(Ordering::Relaxed)));
        if quiet > STALL_AFTER {
            return Health::degraded(format!(
                "nothing from {what} for {:.1} s, after {buffers} buffers",
                quiet.as_secs_f32()
            ));
        }
        Health {
            state: HealthState::Ok,
            detail: Some(format!("{buffers} buffers from {what}")),
            latency_ms: None,
        }
    }

    /// The pipeline, for a plugin that has a property to change while running.
    pub fn pipeline(&self) -> &gst::Pipeline {
        &self.pipeline
    }

    /// Wait, at most `within`, for the first buffer to leave the counted
    /// element. Answers whether one did.
    ///
    /// `start` is allowed to return before the first frame, and for a plugin
    /// that draws its own pictures it should. A camera is different: the
    /// operating system takes a second or two to hand one over, and a `start`
    /// that returned immediately would have the core build its half of the
    /// pipeline, reach playing, and then wait on an empty socket. The
    /// conformance harness counts the frames in the three seconds after
    /// `start` and that cold second is most of its missing tenth.
    ///
    /// Returns early on a fault, so a camera that is not there costs nothing.
    pub fn wait_for_data(&self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if self.buffers() > 0 {
                return true;
            }
            if self.fault().is_some() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.buffers() > 0
    }

    /// Send an end of stream and wait briefly for it to reach the sink.
    ///
    /// A recording needs this: a muxer killed in PLAYING leaves a file with no
    /// index. A camera does not care, and pays a few milliseconds for it.
    pub fn drain(&self, within: Duration) {
        if self.pipeline.current_state() != gst::State::Playing {
            return;
        }
        if !self.pipeline.send_event(gst::event::Eos::new()) {
            return;
        }
        if let Some(bus) = self.pipeline.bus() {
            let _ = bus.timed_pop_filtered(
                gst::ClockTime::from_mseconds(within.as_millis() as u64),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
        }
    }

    /// Stop the pipeline and join the bus thread. Idempotent.
    pub fn stop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        self.stop_bus.store(true, Ordering::Relaxed);
        if let Some(handle) = self.bus_thread.take() {
            let _ = handle.join();
        }
    }
}

/// Open a capture, retrying while the device is still being handed back.
///
/// `restart-in-place` replaces the process behind a running pipeline, and the
/// new one asks the operating system for a camera or a screen that the old one
/// let go of milliseconds ago. On macOS in particular the device is not free
/// yet and the open fails outright. Nothing is wrong except the timing, so it
/// is tried again rather than reported.
///
/// Each attempt builds a fresh pipeline (the caller's closure), starts it, and
/// waits `first_data_within` for a buffer. A fault is a reason to try again; a
/// device that has simply not produced anything yet is not, because a slow
/// camera is allowed and `health` is where that is reported.
pub fn open_with_retry<F>(
    attempts: u32,
    gap: Duration,
    first_data_within: Duration,
    reporter: Option<&Reporter>,
    mut build: F,
) -> Result<Capture, String>
where
    F: FnMut() -> Result<Capture, String>,
{
    let mut last = String::new();
    for attempt in 1..=attempts.max(1) {
        match build() {
            Ok(capture) => {
                if capture.wait_for_data(first_data_within) {
                    return Ok(capture);
                }
                match capture.fault() {
                    // Producing nothing yet is not a failure to open.
                    None => return Ok(capture),
                    Some(detail) => last = detail,
                }
            }
            Err(detail) => last = detail,
        }
        if attempt < attempts.max(1) {
            if let Some(r) = reporter {
                r.warn(format!(
                    "attempt {attempt} did not open: {last}. Trying again."
                ));
            }
            std::thread::sleep(gap);
        }
    }
    Err(last)
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One relaxed add per buffer on the named element's source pad.
fn attach_counter(
    pipeline: &gst::Pipeline,
    element: &str,
    buffers: &Arc<AtomicU64>,
    last_ms: &Arc<AtomicU64>,
    started: Instant,
) -> Result<(), String> {
    let target = pipeline
        .by_name(element)
        .ok_or_else(|| format!("the pipeline has no element named '{element}' to count on"))?;
    let pad = target
        .static_pad("src")
        .ok_or_else(|| format!("'{element}' has no source pad to count on"))?;
    let count = Arc::clone(buffers);
    let last = Arc::clone(last_ms);
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        count.fetch_add(1, Ordering::Relaxed);
        last.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    })
    .ok_or_else(|| format!("could not watch '{element}' for buffers"))?;
    Ok(())
}

/// Drain the bus on its own thread so nothing here runs on a streaming thread.
fn spawn_bus_watch(
    pipeline: &gst::Pipeline,
    fault: &Arc<Mutex<Option<String>>>,
    stop: &Arc<AtomicBool>,
    reporter: Option<Reporter>,
) -> Option<std::thread::JoinHandle<()>> {
    let bus = pipeline.bus()?;
    let fault = Arc::clone(fault);
    let stop = Arc::clone(stop);
    std::thread::Builder::new()
        .name("gmx-capture-bus".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(100)) else {
                    continue;
                };
                match message.view() {
                    gst::MessageView::Error(e) => {
                        let detail = describe(e);
                        if let Some(r) = &reporter {
                            r.error(&detail);
                        }
                        *fault.lock().unwrap_or_else(|p| p.into_inner()) = Some(detail);
                    }
                    gst::MessageView::Warning(w) => {
                        if let Some(r) = &reporter {
                            r.warn(format!("{}", w.error()));
                        }
                    }
                    gst::MessageView::Eos(_) => {
                        let detail = "the device stopped sending".to_string();
                        if let Some(r) = &reporter {
                            r.warn(&detail);
                        }
                        *fault.lock().unwrap_or_else(|p| p.into_inner()) = Some(detail);
                    }
                    _ => {}
                }
            }
        })
        .ok()
}

/// A bus error as one line that names the element and the debug string.
fn describe(e: &gst::message::Error) -> String {
    let source = e
        .src()
        .map(|s| s.path_string().to_string())
        .unwrap_or_else(|| "the pipeline".into());
    match e.debug() {
        Some(debug) => format!("{} ({source}: {debug})", e.error()),
        None => format!("{} ({source})", e.error()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_pattern_runs_counts_and_stops() {
        let pipeline = build(
            "videotestsrc is-live=true ! video/x-raw,width=32,height=32,framerate=60/1 ! \
             queue name=gmx-video-queue ! fakesink sync=false",
        )
        .expect("the description parses");
        let mut capture =
            Capture::start(pipeline, Some("gmx-video-queue"), None).expect("it plays");
        std::thread::sleep(Duration::from_millis(250));
        assert!(capture.buffers() > 0, "no buffers in 250 ms at 60 fps");
        assert_eq!(capture.health("the test pattern").state, HealthState::Ok);
        assert!(capture.fault().is_none());
        capture.stop();
        capture.stop(); // idempotent
    }

    #[test]
    fn a_pipeline_that_cannot_start_says_why_and_leaves_nothing_running() {
        // A file that is not there fails the state change, and the bus
        // carries the reason.
        let pipeline = build(
            "filesrc location=/nonexistent/gmx-capture-test.mkv ! fakesink name=gmx-video-queue",
        )
        .expect("the description parses");
        let err = Capture::start(pipeline, None, None).expect_err("there is no such file");
        assert!(
            err.to_lowercase().contains("playing") || err.contains("start"),
            "{err}"
        );
    }

    #[test]
    fn waiting_for_the_first_buffer_returns_as_soon_as_one_lands() {
        let pipeline = build(
            "videotestsrc is-live=true ! video/x-raw,width=32,height=32,framerate=60/1 ! \
             queue name=gmx-video-queue ! fakesink sync=false",
        )
        .expect("the description parses");
        let capture = Capture::start(pipeline, Some("gmx-video-queue"), None).expect("it plays");
        let waited = Instant::now();
        assert!(
            capture.wait_for_data(Duration::from_secs(2)),
            "no buffer in two seconds"
        );
        assert!(
            waited.elapsed() < Duration::from_secs(2),
            "it waited the whole timeout"
        );
    }

    #[test]
    fn a_device_that_frees_up_on_the_second_try_is_opened_rather_than_reported() {
        let tries = std::sync::atomic::AtomicU32::new(0);
        let capture = open_with_retry(
            3,
            Duration::from_millis(10),
            Duration::from_millis(400),
            None,
            || {
                if tries.fetch_add(1, Ordering::Relaxed) == 0 {
                    return Err("the device is busy".into());
                }
                let pipeline = build(
                    "videotestsrc is-live=true ! video/x-raw,width=16,height=16,framerate=60/1 \
                     ! queue name=gmx-video-queue ! fakesink sync=false",
                )?;
                Capture::start(pipeline, Some("gmx-video-queue"), None)
            },
        )
        .expect("the second attempt works");
        assert!(capture.buffers() > 0);
        assert_eq!(tries.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_device_that_never_frees_up_reports_the_last_reason() {
        let err = open_with_retry(
            2,
            Duration::from_millis(1),
            Duration::from_millis(10),
            None,
            || Err("the device is busy".into()),
        )
        .expect_err("it never opens");
        assert_eq!(err, "the device is busy");
    }

    #[test]
    fn counting_on_an_element_that_is_not_there_names_it() {
        let pipeline = build("videotestsrc ! fakesink").expect("the description parses");
        let err = Capture::start(pipeline, Some("no-such-queue"), None).expect_err("no element");
        assert!(err.contains("no-such-queue"), "{err}");
    }

    #[test]
    fn a_bad_description_quotes_itself() {
        let err = build("thereisnosuchelement ! fakesink").expect_err("no such element");
        assert!(err.contains("thereisnosuchelement"), "{err}");
    }

    #[test]
    fn a_pipeline_producing_nothing_goes_degraded_rather_than_pretending() {
        let pipeline = build(
            "videotestsrc num-buffers=1 is-live=true ! video/x-raw,width=16,height=16 ! \
             queue name=gmx-video-queue ! fakesink sync=false",
        )
        .expect("the description parses");
        let capture = Capture::start(pipeline, Some("gmx-video-queue"), None).expect("it plays");
        std::thread::sleep(STALL_AFTER + Duration::from_millis(400));
        // One buffer then end of stream: the bus says so, which is failing,
        // and if the message has not landed yet the stall check catches it.
        assert_ne!(capture.health("the test pattern").state, HealthState::Ok);
    }
}
