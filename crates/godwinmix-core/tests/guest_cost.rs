//! What a live guest costs the mixer: their 720p camera taken in over WebRTC
//! and decoded, their mix minus bus, and their return (the programme encode
//! packetised, the bus made AAC then Opus) sent back over WebRTC.
//!
//! A measurement, not a check, so it is ignored by default:
//!
//! ```text
//! GMX_GUESTS=4 cargo test -p godwinmix-core --release --test guest_cost -- --ignored --nocapture guests_cost
//! ```
//!
//! The guests' browsers are not in this process: each is this same test
//! binary started again as a child (`peer_process`), holding a sender of
//! 1280x720 H.264 at 30 frames and Opus, as the publisher page sends, and a
//! receiver that decodes the return. Their SDP crosses on the child's stdin
//! and stdout. So what is measured is the mixer's CPU and nothing else.
//!
//! The programme is a 1280x720 canvas at 30 with one test source on air and
//! a `whep/output` with no viewer, so the programme encoder runs in the
//! baseline too: it is shared by every output, and a first guest on a mixer
//! with no other output starts it. `GMX_GUEST_CANVAS` changes the canvas.

use godwinmix_core::buses::AudioCall;
use godwinmix_core::config::{Config, OutputConfig, SourceConfig};
use godwinmix_core::mixer::{self, Command, Mixer, MixerHandle};
use godwinmix_protocol::buses::BusConfig;
use gstreamer as gst;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command as Process, Stdio};
use std::time::{Duration, Instant};

#[path = "guest_cost/peer.rs"]
mod peer;

fn cpu_seconds() -> f64 {
    godwinmix_govern::load::sys::process_cpu_ns().unwrap_or(0) as f64 / 1e9
}

async fn measure(secs: u64) -> f64 {
    let (c0, t0) = (cpu_seconds(), Instant::now());
    tokio::time::sleep(Duration::from_secs(secs)).await;
    (cpu_seconds() - c0) / t0.elapsed().as_secs_f64() * 100.0
}

fn whep(id: &str, bus: Option<&str>) -> OutputConfig {
    let mut cfg = OutputConfig::bare(id, "");
    cfg.type_id = Some("whep/output".into());
    cfg.audio = bus.map(str::to_string);
    cfg.params.insert("stun".into(), toml::Value::String(String::new()));
    cfg
}

/// One guest's browser, in a child process, through its two offers.
struct Browser {
    child: Child,
    lines: BufReader<std::process::ChildStdout>,
}

impl Browser {
    fn start() -> Browser {
        let me = std::env::current_exe().expect("this test binary");
        let mut child = Process::new(me)
            .args(["--ignored", "--exact", "peer_process", "--nocapture"])
            .env("GMX_GUEST_PEER", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the browser starts");
        let lines = BufReader::new(child.stdout.take().unwrap());
        Browser { child, lines }
    }

    /// The next offer the child prints, `OFFER <sdp with | for line ends>`.
    fn offer(&mut self) -> String {
        loop {
            let mut line = String::new();
            assert!(self.lines.read_line(&mut line).unwrap() > 0, "the browser ended");
            if let Some(sdp) = line.trim_end().strip_prefix("OFFER ") {
                return sdp.replace('|', "\r\n");
            }
        }
    }

    fn answer(&mut self, sdp: &str) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "ANSWER {}", sdp.replace("\r\n", "|").replace('\n', "|")).unwrap();
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn guest_in(handle: &MixerHandle, id: &str) -> Browser {
    let source = format!("guest-{id}");
    let cfg = SourceConfig::bare(&source, &godwinmix_core::guest::source::uri_of(id));
    handle.request(|ack| Command::AddSource(Box::new(cfg), Some(ack))).await.expect("add the guest");
    let mut bus = BusConfig::named(&source);
    bus.minus = Some(source.clone());
    handle.audio(AudioCall::AddGuest(bus)).await.expect("the mixer answers").expect("the return bus");
    let out = whep(&format!("{source}-return"), Some(&source));
    handle.request(|ack| Command::AddOutput(Box::new(out), Some(ack))).await.expect("the return");
    let mut browser = Browser::start();
    let shape = godwinmix_core::guest::Shape { fps: godwinmix_core::config::FrameRate::whole(30), rate: 48_000, channels: 2 };
    let lan = godwinmix_core::preview::whep::IceConfig { stun: String::new(), turn: Vec::new() };
    let (_, answer) = godwinmix_core::guest::offer(id, &browser.offer(), shape, &lan).expect("the guest is answered");
    browser.answer(&answer);
    let offer = browser.offer();
    let until = Instant::now() + Duration::from_secs(10);
    let (_, _, answer) = loop {
        match godwinmix_core::whep::offer(&format!("{source}-return"), &offer) {
            Ok(a) => break a,
            Err(e) if Instant::now() < until => {
                let _ = e;
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            Err(e) => panic!("the return was refused: {}", e.message),
        }
    };
    browser.answer(&answer);
    browser
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "a measurement; see the top of the file"]
async fn guests_cost_this_much() {
    gst::init().unwrap();
    godwinmix_core::webrtc_ice::set_tcp(false);
    let canvas = std::env::var("GMX_GUEST_CANVAS").unwrap_or_else(|_| "1280x720".into());
    let (w, h) = canvas.split_once('x').expect("GMX_GUEST_CANVAS is WIDTHxHEIGHT");
    let n: usize = std::env::var("GMX_GUESTS").ok().and_then(|v| v.parse().ok()).unwrap_or(2).clamp(1, 8);
    let cfg: Config = toml::from_str(&format!(
        "[canvas]\nwidth = {w}\nheight = {h}\nfps = 30\nsample_rate = 48000\nchannels = 2\n\n[control]\nbind = \"127.0.0.1:0\"\n"
    ))
    .unwrap();
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).expect("build a mixer");
    mix.start().expect("start");
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let host = SourceConfig::bare("host", "test://smpte");
    handle.request(|ack| Command::AddSource(Box::new(host), Some(ack))).await.expect("add the host");
    handle.request(|ack| Command::Take { source: Some("host".into()), at_running_time_ms: None, ack: Some(ack) }).await.expect("take");
    handle.request(|ack| Command::AddOutput(Box::new(whep("monitor", None)), Some(ack))).await.expect("the monitor");
    // Long enough for the programme encoder and the monitor to settle, so
    // the baseline is not a programme still starting.
    tokio::time::sleep(Duration::from_secs(10)).await;
    let base = measure(15).await;
    let mut browsers = Vec::new();
    for i in 1..=n {
        browsers.push(guest_in(&handle, &format!("g{i}")).await);
    }
    let is_live = |i: usize| godwinmix_core::guest::link(&format!("g{i}")) == godwinmix_core::guest::Link::Live;
    let until = Instant::now() + Duration::from_secs(30);
    while !(1..=n).all(is_live) && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    let live = (1..=n).filter(|i| godwinmix_core::guest::link(&format!("g{i}")) == godwinmix_core::guest::Link::Live).count();
    let with = measure(15).await;
    let status = handle.status().await.expect("status");
    let stalled: Vec<String> = status
        .sources
        .iter()
        .filter(|s| s.id.starts_with("guest-") && !matches!(s.state, godwinmix_protocol::types::SourceState::Live))
        .map(|s| format!("{} {:?}", s.id, s.state))
        .collect();
    let pictures = (1..=n).filter(|i| sent_size_arrives(&format!("g{i}"))).count();
    drop(browsers);
    handle.send(Command::Shutdown).ok();
    assert_eq!(pictures, n, "every guest's 1280x720 picture was decoded");
    let _ = thread.join();
    println!(
        "{canvas} 30 programme: no guest {base:.1} percent of a core; {live} of {n} guests live {with:.1}; each guest {:.1}",
        (with - base) / n as f64
    );
    assert_eq!(live, n, "every guest went live");
    assert!(stalled.is_empty(), "every guest's source stayed live through the measurement: {stalled:?}");
}

/// Whether the guest's own 1280x720 picture is on their inter channel, which
/// only a decoded frame of theirs can be: the source alone makes 320x240.
fn sent_size_arrives(id: &str) -> bool {
    use gstreamer::prelude::*;
    let channel = godwinmix_core::guest::channel(id);
    let reader = gst::parse::launch(&format!("intervideosrc channel={channel} ! appsink name=out sync=false max-buffers=2 drop=true")).unwrap();
    let reader = reader.downcast::<gst::Pipeline>().unwrap();
    let out = reader.by_name("out").unwrap();
    reader.set_state(gst::State::Playing).unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    let mut seen = false;
    while !seen && Instant::now() < until {
        let sample = out.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&100_000_000u64]);
        seen = sample.and_then(|s| s.caps().and_then(|c| c.structure(0).and_then(|st| st.get::<i32>("width").ok()))) == Some(1280);
    }
    let _ = reader.set_state(gst::State::Null);
    seen
}

/// The guest's browser when this binary is started as one. Does nothing in
/// an ordinary run.
#[test]
#[ignore = "run by guests_cost_this_much as a child process"]
fn peer_process() {
    if std::env::var("GMX_GUEST_PEER").is_err() {
        return;
    }
    peer::run();
}
