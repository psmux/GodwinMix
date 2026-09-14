//! Record a real programme through a real FIFO and check the file plays.
//!
//! This is the test the conformance harness cannot run. The core registers
//! only `source` provides today, so `gmx plugin test` takes an output as far
//! as the handshake and no further; everything past that, which is all of the
//! recording, is checked here.
//!
//! What it does is exactly what the core does: make a FIFO, start the plugin
//! with `GMX_MEDIA` pointing at it, answer the handshake, call `start`, and
//! write streamable Matroska carrying H.264 and AAC into the FIFO. Then it
//! stops the plugin the way the core does and plays the file back with
//! `decodebin`, which is the same question `gst-discoverer-1.0` asks.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;

/// Seconds of programme to record. Written as fast as the encoder manages
/// rather than in real time, so this costs a second or two and not ten.
const SECONDS: u32 = 10;
const FPS: u32 = 30;

#[test]
fn ten_seconds_of_programme_are_recorded_and_the_file_plays() {
    gst::init().expect("GStreamer starts");
    let Some(encoder) = ["x264enc", "vtenc_h264", "avenc_h264_videotoolbox"]
        .into_iter()
        .find(|e| gst::ElementFactory::find(e).is_some())
    else {
        eprintln!("no H.264 encoder on this machine, so there is no programme to record");
        return;
    };

    let work = temp_dir("gmx-record");
    let fifo = work.join("media.programme");
    make_fifo(&fifo);
    let recordings = work.join("recordings");

    let mut plugin = spawn_plugin(&fifo);
    let lines = drain_stderr(&mut plugin);
    let mut stdin = plugin.stdin.take().expect("the plugin has stdin");

    // The handshake, as the core writes it.
    expect(&lines, "initialize");
    send(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0", "id": 0,
            "result": {
                "core": "godwinmix", "version": "0.2.0", "api_level": 1, "api_compatible": 1,
                "canvas": {"width": 640, "height": 360, "fps": FPS},
                "transport": "container", "media": fifo.to_string_lossy(),
                "instance": "archive", "provide": "output",
                "params": {
                    "directory": recordings.to_string_lossy(),
                    "pattern": "service-{date}",
                    "format": "mp4"
                }
            }
        }),
    );
    expect(&lines, "initialized");

    send(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "start",
            "params": {
                "canvas": {"width": 640, "height": 360, "fps": FPS},
                "transport": "container", "media": fifo.to_string_lossy()
            }
        }),
    );
    expect(&lines, "\"id\":1");

    // The programme itself: what the core's own tee produces, into the FIFO.
    write_programme(&fifo, encoder);
    // The encoder finishes before the recorder has read everything out of the
    // FIFO. The core stops an output long after the programme ends; this is
    // the same gap, and without it the file is closed mid stream.
    std::thread::sleep(Duration::from_secs(2));

    send(
        &mut stdin,
        &serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "stop"}),
    );
    expect(&lines, "\"id\":2");
    send(
        &mut stdin,
        &serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {"reason": "done"}}),
    );
    let _ = plugin.wait();

    // Everything the plugin said, which is where a failure here explains
    // itself: it logs which streams it linked and what it could not.
    while let Ok(line) = lines.try_recv() {
        eprintln!("plugin: {}", &line[..line.len().min(300)]);
    }
    let file = only_recording(&recordings);
    let size = std::fs::metadata(&file)
        .expect("the recording is on disk")
        .len();
    assert!(
        size > 20_000,
        "the recording is {size} bytes, which is not ten seconds of video"
    );
    let (frames, seconds) = play(&file);
    assert!(
        frames > (SECONDS * FPS) as u64 * 8 / 10,
        "only {frames} frames played back out of {} expected",
        SECONDS * FPS
    );
    assert!(
        seconds >= 8.0,
        "the recording is {seconds:.1} s long, wanted about {SECONDS}"
    );

    let _ = std::fs::remove_dir_all(&work);
}

// --- the core's half --------------------------------------------------------

/// Streamable Matroska carrying H.264 and AAC, which is what `SidecarOutput`
/// writes into the FIFO.
fn write_programme(fifo: &std::path::Path, encoder: &str) {
    let audio = if gst::ElementFactory::find("avenc_aac").is_some() {
        "audiotestsrc num-buffers=470 ! audioconvert ! avenc_aac ! queue ! mux."
    } else {
        ""
    };
    let description = format!(
        "videotestsrc num-buffers={frames} ! video/x-raw,width=640,height=360,framerate={FPS}/1 \
         ! videoconvert ! {encoder} ! h264parse ! queue ! matroskamux name=mux streamable=true \
         ! filesink location={path} sync=false async=false {audio}",
        frames = SECONDS * FPS,
        path = fifo.display(),
    );
    let pipeline = gst::parse::launch(&description)
        .expect("the programme pipeline parses")
        .downcast::<gst::Pipeline>()
        .expect("a pipeline");
    pipeline
        .set_state(gst::State::Playing)
        .expect("the programme plays");
    let bus = pipeline.bus().expect("a bus");
    let message = bus.timed_pop_filtered(
        gst::ClockTime::from_seconds(60),
        &[gst::MessageType::Eos, gst::MessageType::Error],
    );
    if let Some(m) = &message {
        if let gst::MessageView::Error(e) = m.view() {
            panic!(
                "the programme would not encode: {} ({:?})",
                e.error(),
                e.debug()
            );
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
}

/// Play the recording back. Frames out and how long it turned out to be.
fn play(file: &std::path::Path) -> (u64, f64) {
    let description = format!(
        "filesrc location={} ! decodebin ! videoconvert ! fakesink name=out sync=false",
        file.display()
    );
    let pipeline = gst::parse::launch(&description)
        .expect("the playback pipeline parses")
        .downcast::<gst::Pipeline>()
        .expect("a pipeline");
    let sink = pipeline.by_name("out").expect("the sink is named");
    let frames = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counter = std::sync::Arc::clone(&frames);
    sink.static_pad("sink")
        .expect("a sink pad")
        .add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        })
        .expect("the probe attaches");
    pipeline.set_state(gst::State::Playing).expect("it plays");
    let bus = pipeline.bus().expect("a bus");
    let message = bus.timed_pop_filtered(
        gst::ClockTime::from_seconds(60),
        &[gst::MessageType::Eos, gst::MessageType::Error],
    );
    if let Some(m) = &message {
        if let gst::MessageView::Error(e) = m.view() {
            panic!(
                "the recording will not play: {} ({:?})",
                e.error(),
                e.debug()
            );
        }
    }
    let duration = pipeline
        .query_duration::<gst::ClockTime>()
        .map(|d| d.seconds_f64())
        .unwrap_or(0.0);
    let _ = pipeline.set_state(gst::State::Null);
    (frames.load(std::sync::atomic::Ordering::Relaxed), duration)
}

// --- plumbing ---------------------------------------------------------------

fn spawn_plugin(fifo: &std::path::Path) -> Child {
    let root = env!("CARGO_MANIFEST_DIR");
    Command::new(env!("CARGO_BIN_EXE_gmx-file-record"))
        .current_dir(root)
        .env("GMX_PLUGIN", "file-record")
        .env("GMX_PROVIDE", "output")
        .env("GMX_INSTANCE", "archive")
        .env("GMX_API_LEVEL", "1")
        .env("GMX_PLUGIN_ROOT", root)
        .env("GMX_MEDIA", fifo)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the plugin starts")
}

fn drain_stderr(plugin: &mut Child) -> Receiver<String> {
    let (tx, rx) = channel();
    let stderr = plugin.stderr.take().expect("the plugin has stderr");
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn send(stdin: &mut std::process::ChildStdin, value: &serde_json::Value) {
    writeln!(stdin, "{value}").expect("the plugin is still reading stdin");
    stdin.flush().ok();
}

/// Wait for a line containing `needle`, showing everything said if it never
/// comes.
fn expect(lines: &Receiver<String>, needle: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut seen = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match lines.recv_timeout(left) {
            Ok(line) => {
                eprintln!("plugin: {}", &line[..line.len().min(300)]);
                let matched = line.contains(needle);
                seen.push(line);
                if matched {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {
                panic!(
                    "never saw a line containing {needle}. It said:\n  {}",
                    seen.join("\n  ")
                );
            }
        }
    }
}

fn temp_dir(prefix: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a working directory");
    dir
}

fn make_fifo(path: &std::path::Path) {
    let status = Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("mkfifo runs on a Unix machine");
    assert!(
        status.success(),
        "could not make the FIFO at {}",
        path.display()
    );
}

fn only_recording(folder: &std::path::Path) -> std::path::PathBuf {
    let files: Vec<_> = std::fs::read_dir(folder)
        .unwrap_or_else(|e| {
            panic!(
                "the recording folder {} is not there: {e}",
                folder.display()
            )
        })
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    assert_eq!(files.len(), 1, "expected one recording, found {files:?}");
    files.into_iter().next().expect("checked above")
}
