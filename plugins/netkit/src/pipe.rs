//! A GStreamer pipeline with its bus watched on its own thread.
//!
//! Principle 1 of the vision says the programme output never stops, and the
//! rule under it says never do blocking work on a streaming thread or the bus
//! handler. So the bus is popped on a thread of its own, and everything that
//! thread does with a message is: set an atomic, and write one line on stderr
//! through the SDK's [`Reporter`]. Nothing else. A plugin asking
//! [`Pipe::failure`] gets the last error without waiting for anything.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;

/// What the bus has seen. Cheap to clone, safe to read from any thread.
#[derive(Clone, Default)]
pub struct Watch {
    failure: Arc<Mutex<Option<String>>>,
    ended: Arc<AtomicBool>,
}

impl Watch {
    /// The last error the bus reported, if any.
    pub fn failure(&self) -> Option<String> {
        self.failure.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Has the stream ended? An SRT peer going away shows up here.
    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::Relaxed)
    }

    /// Forget an error, for a reconnect that is about to try again.
    pub fn clear(&self) {
        *self.failure.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.ended.store(false, Ordering::Relaxed);
    }

    fn fail(&self, message: String) {
        *self.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(message);
    }
}

/// A pipeline, its bus watch, and the promise that dropping it stops the media.
pub struct Pipe {
    pipeline: gst::Pipeline,
    watch: Watch,
    /// Set when the watch thread should stop popping.
    done: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Pipe {
    /// Build from a `gst-launch` style description.
    ///
    /// The description is the honest way to write these plugins: every one of
    /// them is two or three elements and a property list, and a reader who
    /// knows `gst-launch-1.0` can check it against what the plugin claims.
    pub fn launch(description: &str) -> Result<Pipe, String> {
        crate::init()?;
        let element = gst::parse::launch(description)
            .map_err(|e| format!("could not build the pipeline: {e}\n  {description}"))?;
        let pipeline = element
            .downcast::<gst::Pipeline>()
            .map_err(|_| "the description did not parse into a pipeline".to_string())?;
        Ok(Pipe::wrap(pipeline))
    }

    /// Take an already built pipeline.
    pub fn wrap(pipeline: gst::Pipeline) -> Pipe {
        Pipe {
            pipeline,
            watch: Watch::default(),
            done: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    /// The pipeline, for a plugin that needs an element out of it by name.
    pub fn pipeline(&self) -> &gst::Pipeline {
        &self.pipeline
    }

    /// One element by name, or `None`.
    pub fn by_name(&self, name: &str) -> Option<gst::Element> {
        self.pipeline.by_name(name)
    }

    /// What the bus has seen so far.
    pub fn watch(&self) -> Watch {
        self.watch.clone()
    }

    /// The last error, if the bus has reported one.
    pub fn failure(&self) -> Option<String> {
        self.watch.failure()
    }

    /// Start watching the bus on its own thread, then go to PLAYING.
    ///
    /// `reporter` is optional so the tests can run a pipe with no core behind
    /// it. The watch thread ends when the pipe is dropped.
    pub fn play(&mut self, reporter: Option<Reporter>) -> Result<(), String> {
        self.start_watch(reporter);
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| format!("the pipeline would not start: {e}"))?;
        Ok(())
    }

    fn start_watch(&mut self, reporter: Option<Reporter>) {
        if self.thread.is_some() {
            return;
        }
        let Some(bus) = self.pipeline.bus() else { return };
        let watch = self.watch.clone();
        let done = Arc::clone(&self.done);
        let name = self.pipeline.name().to_string();
        let thread = std::thread::Builder::new()
            .name("gmx-net-bus".into())
            .spawn(move || bus_loop(bus, watch, done, reporter, name))
            .ok();
        self.thread = thread;
    }

    /// Stop the media and let the watch thread go.
    pub fn stop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        let _ = self.pipeline.set_state(gst::State::Null);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Pop messages until the pipe is dropped. One atomic and one log line each.
fn bus_loop(
    bus: gst::Bus,
    watch: Watch,
    done: Arc<AtomicBool>,
    reporter: Option<Reporter>,
    name: String,
) {
    use gst::MessageView;
    let tick = gst::ClockTime::from_mseconds(250);
    while !done.load(Ordering::Relaxed) {
        let Some(message) = bus.timed_pop(Some(tick)) else {
            continue;
        };
        match message.view() {
            MessageView::Error(e) => {
                let text = format!(
                    "{}: {} ({})",
                    e.src().map(|s| s.path_string().to_string()).unwrap_or_else(|| name.clone()),
                    e.error(),
                    e.debug().unwrap_or_default()
                );
                watch.fail(text.clone());
                if let Some(r) = &reporter {
                    r.error(text);
                }
            }
            MessageView::Warning(w) => {
                if let Some(r) = &reporter {
                    r.warn(format!("{}", w.error()));
                }
            }
            MessageView::Eos(_) => {
                watch.ended.store(true, Ordering::Relaxed);
                if let Some(r) = &reporter {
                    r.info("the stream ended");
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipe_runs_and_stops() {
        let mut pipe = Pipe::launch(
            "videotestsrc num-buffers=3 ! video/x-raw,width=32,height=32 ! fakesink name=end",
        )
        .expect("the description parses");
        assert!(pipe.by_name("end").is_some());
        pipe.play(None).expect("it starts");
        pipe.stop();
        assert!(pipe.failure().is_none());
    }

    #[test]
    fn a_bad_description_names_itself_in_the_error() {
        let err = match Pipe::launch("nosuchelement ! fakesink") {
            Ok(_) => panic!("a description naming an element that does not exist must not build"),
            Err(e) => e,
        };
        assert!(err.contains("nosuchelement"), "{err}");
    }

    #[test]
    fn the_bus_watch_records_an_error_without_blocking_the_caller() {
        // A file that is not there is the cheapest way to get an error on the
        // bus without waiting for a network timeout.
        let mut pipe = Pipe::launch(
            "filesrc location=/nonexistent/gmx-netkit-test.ts ! fakesink name=end",
        )
        .expect("the description parses");
        pipe.play(None).ok();
        let watch = pipe.watch();
        for _ in 0..40 {
            if watch.failure().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(watch.failure().is_some(), "the bus watch saw nothing");
        watch.clear();
        assert!(watch.failure().is_none());
    }

    #[test]
    fn eos_is_recorded() {
        let mut pipe =
            Pipe::launch("videotestsrc num-buffers=1 ! fakesink name=end").expect("parses");
        pipe.play(None).expect("it starts");
        let watch = pipe.watch();
        for _ in 0..40 {
            if watch.ended() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(watch.ended(), "no end of stream was seen");
    }
}
