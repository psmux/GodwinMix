//! Start and stop recording through the real mixer's command queue.
use godwinmix_core::config::OutputConfig;
use godwinmix_core::prelude::*;
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_a_recording_finishes_a_playable_file_without_stopping_the_mixer() {
    gstreamer::init().unwrap();
    let mut cfg = Config::from_toml("", "recording test").unwrap();
    cfg.canvas.width = 320;
    cfg.canvas.height = 180;
    let (mut mix, handle, cmd_rx, mut bus_rx) = Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let bus_handle = handle.clone();
    tokio::spawn(async move {
        while let Some(event) = bus_rx.recv().await {
            if bus_handle.send(Command::Bus(event)).is_err() { break; }
        }
    });
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());
    let folder = std::env::temp_dir().join(format!("gmx-record-live-{}", std::process::id()));
    let mut output = OutputConfig::bare("archive", "record://programme");
    output.params.insert("directory".into(), folder.to_string_lossy().to_string().into());
    handle.request(|ack| Command::AddOutput(Box::new(output), Some(ack))).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    let path = loop {
        let status = handle.status().await.unwrap();
        let record = status.outputs.iter().find(|o| o.id == "archive").unwrap();
        if record.extra["bytes_muxed"].as_u64().unwrap_or(0) > 1000 {
            break record.extra["recording_path"].as_str().unwrap().to_string();
        }
        assert!(Instant::now() < deadline, "recorder received no programme");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    tokio::time::sleep(Duration::from_secs(2)).await;
    let start = Instant::now();
    handle.request(|ack| Command::RemoveOutput("archive".into(), Some(ack))).await.unwrap();
    assert!(start.elapsed() < Duration::from_secs(2), "file drain blocked the control queue");
    assert!(handle.status().await.unwrap().outputs.is_empty());
    tokio::time::sleep(Duration::from_secs(2)).await;
    let info = gstreamer_pbutils::Discoverer::new(gstreamer::ClockTime::from_seconds(5)).unwrap()
        .discover_uri(glib::filename_to_uri(&path, None).unwrap().as_str()).unwrap();
    assert!(!info.video_streams().is_empty(), "the file has no playable video");
    assert!(!info.audio_streams().is_empty(), "the file has no playable audio");
    handle.send(Command::Shutdown).unwrap();
    tokio::task::spawn_blocking(move || thread.join()).await.unwrap().unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}
