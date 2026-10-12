//! The mixer without FFmpeg: a programme recorded and decoded back with no
//! gst-libav element anywhere a non libav one exists.
//!
//! On every platform this checks that selection and `decodebin` both take
//! the catalogue's alternatives over libav where this machine has them. The
//! platforms workflow also runs it on a Linux runner with gstreamer1.0-libav
//! not installed and `GMX_EXPECT_NO_LIBAV=1`, where it first proves libav is
//! really absent, so a pass there means the mixer encodes and decodes a
//! programme with no FFmpeg on the machine at all.

use godwinmix_core::catalogue::libav::{in_use, is_libav};
use godwinmix_core::catalogue::select::GstRegistry;
use godwinmix_core::config::OutputConfig;
use godwinmix_core::prelude::*;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn expect_no_libav() -> bool {
    std::env::var("GMX_EXPECT_NO_LIBAV").is_ok_and(|v| v == "1")
}

/// Record a few seconds of the programme and return the file.
async fn record(folder: &std::path::Path) -> (String, MixerHandle, std::thread::JoinHandle<()>) {
    let mut cfg = Config::from_toml("", "no libav test").unwrap();
    cfg.canvas.width = 320;
    cfg.canvas.height = 180;
    let (mut mix, handle, cmd_rx, mut bus_rx) = Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let bus_handle = handle.clone();
    tokio::spawn(async move {
        while let Some(event) = bus_rx.recv().await {
            if bus_handle.send(Command::Bus(event)).is_err() {
                break;
            }
        }
    });
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let mut output = OutputConfig::bare("archive", "record://programme");
    output.params.insert("directory".into(), folder.to_string_lossy().to_string().into());
    output.params.insert("format".into(), "mkv".into());
    handle.request(|ack| Command::AddOutput(Box::new(output), Some(ack))).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let path = loop {
        let status = handle.status().await.unwrap();
        let rec = status.outputs.iter().find(|o| o.id == "archive").unwrap();
        if rec.extra["bytes_muxed"].as_u64().unwrap_or(0) > 20_000 {
            break rec.extra["recording_path"].as_str().unwrap().to_string();
        }
        assert!(Instant::now() < deadline, "the recorder received no programme");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    tokio::time::sleep(Duration::from_secs(2)).await;
    handle.request(|ack| Command::RemoveOutput("archive".into(), Some(ack))).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    (path, handle, thread)
}

/// Decode a file to the end with `decodebin` and return the decoders it
/// plugged.
fn decoders_for(path: &str) -> Vec<String> {
    let uri = glib::filename_to_uri(path, None).unwrap();
    let line = format!("uridecodebin name=d uri=\"{uri}\" d. ! queue ! fakesink sync=false d. ! queue ! fakesink sync=false");
    let pipeline = gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    pipeline.connect_deep_element_added(move |_, _, el| {
        if let Some(f) = el.factory() {
            if f.klass().contains("Decoder") && !f.klass().contains("Bin") {
                s.lock().unwrap().push(f.name().to_string());
            }
        }
    });
    pipeline.set_state(gst::State::Playing).unwrap();
    let msg = pipeline
        .bus()
        .unwrap()
        .timed_pop_filtered(gst::ClockTime::from_seconds(20), &[gst::MessageType::Eos, gst::MessageType::Error]);
    pipeline.set_state(gst::State::Null).unwrap();
    if let Some(gst::MessageView::Error(e)) = msg.as_ref().map(|m| m.view()) {
        panic!("decoding the recording failed: {} {:?}", e.error(), e.debug());
    }
    assert!(msg.is_some(), "decoding the recording did not finish");
    let out = seen.lock().unwrap().clone();
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_programme_is_encoded_and_decoded_without_libav_where_anything_else_can() {
    gst::init().unwrap();
    if expect_no_libav() {
        assert!(gst::Registry::get().find_plugin("libav").is_none(), "GMX_EXPECT_NO_LIBAV=1 but gst-libav is installed");
    }
    let cat = godwinmix_core::catalogue::global();
    let needs: Vec<(&str, String, String)> = in_use(&cat, &GstRegistry);
    println!("taken from libav on this machine: {needs:?}");

    let folder = std::env::temp_dir().join(format!("gmx-no-libav-{}", std::process::id()));
    let (path, handle, thread) = record(&folder).await;
    let decoders = decoders_for(&path);
    println!("decodebin plugged {decoders:?} for {path}");
    assert!(decoders.len() >= 2, "expected a video and an audio decoder, got {decoders:?}");
    for d in &decoders {
        let allowed = needs.iter().any(|(role, _, e)| *role == "decode" && e == d);
        assert!(!is_libav(d) || allowed, "decodebin took {d} though the catalogue prefers another decoder here");
    }

    handle.send(Command::Shutdown).unwrap();
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    let _ = std::fs::remove_dir_all(folder);
}
