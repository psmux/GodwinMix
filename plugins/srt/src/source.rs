//! The pipeline, and the honest answer to "is anybody sending".
//!
//! Three elements and nothing else:
//!
//! ```text
//!   srtsrc ──► queue ──► fdsink fd=1
//! ```
//!
//! SRT carries MPEG-TS, so what leaves on stdout is the encoded stream exactly
//! as it arrived. The plugin does not demux it, does not decode it and does not
//! look at it. The core's container transport is `fdsrc ! decodebin`, so the
//! decode happens once, in the core, on the core's hardware aware path. That is
//! why the manifest says `media = { video = "container", audio = "container" }`:
//! this plugin moves bytes, it does not make pictures.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gmx_netkit::pipe::Pipe;
use gmx_netkit::stats::SrtNumbers;
use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::uri::Settings;

/// How often the statistics are read and the cached health updated. Once a
/// second is a property read per second, which is nothing, and it means the
/// core's `health` call is answered from a value at most a second old without
/// the call itself touching the pipeline.
const POLL_MS: u64 = 1000;

/// Where the received bytes go.
///
/// In a running plugin this is always stdout, which is what the container
/// transport means. The tests point it at a file instead, because a test
/// harness owns its own stdout and a megabyte of MPEG-TS in the middle of the
/// test output helps nobody.
#[derive(Debug, Clone)]
pub enum Sink {
    Stdout,
    /// Only the tests build this one, which is why the lint is waived here
    /// rather than the variant being deleted: the alternative is a test that
    /// writes a megabyte of MPEG-TS into the test harness's own stdout.
    #[allow(dead_code)]
    File(std::path::PathBuf),
}

/// A running SRT receiver: the pipeline, and the thread that reads its numbers.
pub struct Receiver {
    pipe: Pipe,
    src: gst::Element,
    stop: Arc<AtomicBool>,
    poller: Option<std::thread::JoinHandle<()>>,
}

impl Receiver {
    /// Build the pipeline and go to PLAYING.
    ///
    /// The passphrase is set as a property rather than put in the address, so
    /// there is no code path that prints it by printing the address.
    pub fn start(settings: &Settings, reporter: Option<Reporter>) -> Result<Receiver, String> {
        Receiver::start_into(settings, reporter, Sink::Stdout)
    }

    /// The same, with somewhere else to put the bytes.
    pub fn start_into(
        settings: &Settings,
        reporter: Option<Reporter>,
        sink: Sink,
    ) -> Result<Receiver, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(&["srtsrc", "queue", "fdsink"])?;

        let pipeline = gst::Pipeline::with_name("gmx-srt-source");
        let src = element("srtsrc", "src")?;
        src.set_property("uri", settings.address());
        if !settings.passphrase.is_empty() {
            src.set_property("passphrase", &settings.passphrase);
        }
        // `latency` and `mode` are already in the address, but a pasted uri may
        // have carried its own and the property must not fight it. Only the
        // ones the address does not mention are set here.
        set_bool(&src, "auto-reconnect", settings.auto_reconnect);
        // Never block the pipeline waiting for a peer: a listener with nobody
        // connected must still reach PLAYING so `health` can say "waiting".
        set_bool(&src, "wait-for-connection", false);
        if settings.mode == "listener" {
            // Keep the port open when a sender goes away, so the next one is
            // picked up without the supervisor having to rebuild anything.
            set_bool(&src, "keep-listening", true);
        }

        // One second of slack between the network and the pipe. It absorbs a
        // core that is busy for a moment; beyond that SRT's own receive buffer
        // does the work, and dropping there is better than growing here.
        let queue = element("queue", "q")?;
        queue.set_property("max-size-time", 1_000_000_000u64);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-buffers", 0u32);

        let sink = match &sink {
            Sink::Stdout => {
                let out = element("fdsink", "out")?;
                out.set_property("fd", 1i32);
                out
            }
            Sink::File(path) => {
                let out = element("filesink", "out")?;
                out.set_property("location", path.to_string_lossy().to_string());
                out
            }
        };
        set_bool(&sink, "sync", false);
        set_bool(&sink, "async", false);

        pipeline
            .add_many([&src, &queue, &sink])
            .map_err(|e| format!("could not assemble the pipeline: {e}"))?;
        gst::Element::link_many([&src, &queue, &sink])
            .map_err(|e| format!("could not link srtsrc to the pipe: {e}"))?;

        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter.clone())?;

        let stop = Arc::new(AtomicBool::new(false));
        let poller = spawn_poll(src.clone(), pipe.watch(), reporter, Arc::clone(&stop));
        Ok(Receiver { pipe, src, stop, poller })
    }

    /// What the link is doing, read fresh.
    pub fn health(&self) -> Health {
        health_of(&self.src, &self.pipe.watch())
    }

    /// The link statistics as JSON, for the `stats` call.
    pub fn stats(&self) -> serde_json::Value {
        SrtNumbers::read(&self.src).unwrap_or_default().json()
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.poller.take() {
            let _ = thread.join();
        }
        self.pipe.stop();
    }
}

/// The health poll thread. One property read, one atomic store, per second.
fn spawn_poll(
    src: gst::Element,
    watch: gmx_netkit::pipe::Watch,
    reporter: Option<Reporter>,
    stop: Arc<AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    let reporter = reporter?;
    std::thread::Builder::new()
        .name("gmx-srt-stats".into())
        .spawn(move || {
            let mut last = String::new();
            while !stop.load(Ordering::Relaxed) {
                let health = health_of(&src, &watch);
                // The state is what the core acts on, so a change in it is
                // worth a notification; a change in the detail alone is not.
                let state = format!("{:?}", health.state);
                if state != last {
                    last = state;
                    reporter.health_changed(health);
                } else {
                    reporter.set_health(health);
                }
                std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
            }
        })
        .ok()
}

/// Bus error beats statistics; statistics beat guessing.
fn health_of(src: &gst::Element, watch: &gmx_netkit::pipe::Watch) -> Health {
    if let Some(failure) = watch.failure() {
        return Health::failing(failure);
    }
    let Some(numbers) = SrtNumbers::read(src) else {
        return Health::degraded(
            "this build of srtsrc publishes no statistics, so the link cannot be reported. \
             The stream itself is unaffected.",
        );
    };
    if numbers.moving > 0 {
        let mut health = Health::ok();
        health.detail = Some(numbers.phrase());
        return health;
    }
    Health::degraded(
        "no packets have arrived yet. Check that the sender is running and that its \
         address, port, passphrase and stream id match this source's settings.",
    )
}

fn element(factory: &str, name: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .name(name)
        .build()
        .map_err(|e| format!("could not make '{factory}': {e}"))
}

/// Set a boolean property only if this build of the element has it.
fn set_bool(element: &gst::Element, name: &str, value: bool) {
    if element.find_property(name).is_some() {
        element.set_property(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `usable` rather than `exists`: the Windows GStreamer installer
    /// registers `srtsrc` and ships a `gstsrt.dll` that will not load, so the
    /// registry says yes and the first `make` says no.
    fn has_srt() -> bool {
        gmx_netkit::init().is_ok() && gmx_netkit::elements::usable("srtsrc")
    }

    #[test]
    fn a_listener_reaches_playing_with_nobody_connected_and_reports_waiting() {
        if !has_srt() {
            eprintln!("skipped: this build of GStreamer has no srtsrc");
            return;
        }
        // Port 0 lets the operating system pick, so two runs never collide.
        let settings = Settings::from_params(&json!({
            "mode": "listener", "host": "127.0.0.1", "port": 0
        }));
        // fd 1 is the test harness's own stdout here, which is harmless: a
        // listener with no sender produces no bytes.
        let receiver = match Receiver::start(&settings, None) {
            Ok(r) => r,
            Err(e) => panic!("a listener on an ephemeral port must start: {e}"),
        };
        let health = receiver.health();
        assert_ne!(
            health.state,
            godwinmix_sdk::wire::HealthState::Failing,
            "{:?}",
            health.detail
        );
        assert!(receiver.stats().get("moving").is_some());
    }

    /// A free UDP port on the loopback, so two runs of the suite never collide.
    fn free_port() -> u16 {
        std::net::UdpSocket::bind("127.0.0.1:0")
            .expect("the loopback has a free port")
            .local_addr()
            .expect("a bound socket has an address")
            .port()
    }

    /// The repository has no `which` crate, so this is the same four lines the
    /// core's own tests use.
    fn which(program: &str) -> Option<std::path::PathBuf> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    }

    #[test]
    fn a_real_sender_on_the_loopback_arrives_and_the_link_reports_itself() {
        if !has_srt() {
            eprintln!("skipping: this build of GStreamer has no srtsrc");
            return;
        }
        if !gmx_netkit::elements::exists("x264enc") || !gmx_netkit::elements::exists("mpegtsmux") {
            eprintln!("skipping: this build of GStreamer cannot encode MPEG-TS to send");
            return;
        }
        let Some(launcher) = which("gst-launch-1.0") else {
            eprintln!("skipping: gst-launch-1.0 is not on PATH");
            return;
        };

        let port = free_port();
        let file = std::env::temp_dir().join(format!("gmx-srt-test-{}.ts", std::process::id()));
        let settings = Settings::from_params(&json!({
            "mode": "listener", "host": "127.0.0.1", "port": port, "latency_ms": 120
        }));
        let receiver = Receiver::start_into(&settings, None, Sink::File(file.clone()))
            .expect("the listener starts");

        let mut sender = std::process::Command::new(launcher)
            .args([
                "-q",
                "videotestsrc",
                "is-live=true",
                "!",
                "video/x-raw,width=320,height=240,framerate=30/1",
                "!",
                "x264enc",
                "tune=zerolatency",
                "key-int-max=30",
                "!",
                "mpegtsmux",
                "!",
                "srtsink",
                &format!("uri=srt://127.0.0.1:{port}?mode=caller&latency=120"),
                "wait-for-connection=false",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("gst-launch-1.0 starts");

        let mut bytes = 0u64;
        let mut healthy = false;
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            bytes = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
            healthy = receiver.health().state == godwinmix_sdk::wire::HealthState::Ok;
            if bytes > 100_000 && healthy {
                break;
            }
        }
        let health = receiver.health();
        let stats = receiver.stats();
        let _ = sender.kill();
        let _ = sender.wait();
        drop(receiver);
        let _ = std::fs::remove_file(&file);

        assert!(bytes > 100_000, "only {bytes} bytes arrived over SRT");
        assert!(healthy, "health stayed {:?}: {:?}", health.state, health.detail);
        assert!(
            stats["moving"].as_i64().unwrap_or(0) > 0,
            "the link reported nothing moving: {stats}"
        );
    }

    #[test]
    fn a_missing_element_is_named_with_where_it_comes_from() {
        gmx_netkit::init().expect("gstreamer");
        let err = gmx_netkit::elements::require(&["srtsrc", "not-an-element"]).unwrap_err();
        assert!(err.contains("not-an-element"), "{err}");
    }
}
