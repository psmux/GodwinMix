//! A recording started into a running show is as long as the time it ran.
//!
//! A tester recorded ten seconds of a mixer that had been up two minutes and
//! the file claimed 2:05: the muxer wrote the programme's running time where
//! the recording's own time belonged. Both containers are checked, started
//! together after the programme has run a while.
use godwinmix_core::config::OutputConfig;
use godwinmix_core::prelude::*;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
use std::time::{Duration, Instant};

/// How long the programme runs before Record is pressed. Long enough that a
/// file stamped from the programme's clock is obviously too long.
const BEFORE: Duration = Duration::from_secs(8);
/// How long the recordings run.
const RECORD: Duration = Duration::from_secs(3);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recording_started_late_starts_at_zero_and_lasts_as_long_as_it_ran() {
    gst::init().unwrap();
    let mut cfg = Config::from_toml("", "recording duration test").unwrap();
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
    tokio::time::sleep(BEFORE).await;

    let folder = std::env::temp_dir().join(format!("gmx-record-duration-{}", std::process::id()));
    for format in ["mp4", "mkv"] {
        let mut output = OutputConfig::bare(&format!("rec-{format}"), "record://programme");
        let dir = folder.to_string_lossy().to_string();
        output.params.insert("directory".into(), dir.into());
        output.params.insert("format".into(), format.into());
        handle
            .request(|ack| Command::AddOutput(Box::new(output), Some(ack)))
            .await
            .unwrap();
    }
    let started = Instant::now();
    tokio::time::sleep(RECORD).await;
    let mut paths = Vec::new();
    for format in ["mp4", "mkv"] {
        let status = handle.status().await.unwrap();
        let id = format!("rec-{format}");
        let record = status.outputs.iter().find(|o| o.id == id).unwrap();
        paths.push(record.extra["recording_path"].as_str().unwrap().to_string());
        handle
            .request(|ack| Command::RemoveOutput(id.clone(), Some(ack)))
            .await
            .unwrap();
    }
    let ran = started.elapsed();
    tokio::time::sleep(Duration::from_secs(3)).await;
    handle.send(Command::Shutdown).unwrap();
    tokio::task::spawn_blocking(move || thread.join())
        .await
        .unwrap()
        .unwrap();

    let found: Vec<_> = paths.iter().map(|path| measure(path, ran)).collect();
    for (path, duration, firsts) in &found {
        check(path, *duration, *firsts, ran);
    }
    std::fs::remove_dir_all(folder).unwrap();
}

/// The duration a player reads, and the first video and audio times it shows.
fn measure(path: &str, ran: Duration) -> (String, gst::ClockTime, [Duration; 2]) {
    let uri = glib::filename_to_uri(path, None).unwrap();
    let info = gstreamer_pbutils::Discoverer::new(gst::ClockTime::from_seconds(10))
        .unwrap()
        .discover_uri(uri.as_str())
        .unwrap();
    let duration = info.duration().expect("the file has a duration");
    eprintln!("{path}: duration {duration}, recorded about {ran:?}");
    let firsts = first_times(path);
    eprintln!("{path}: first video and audio at {firsts:?}");
    if path.ends_with(".mkv") {
        eprintln!("{path}: first cluster at {:?}", first_cluster(path));
    }
    (path.to_string(), duration, firsts)
}

fn check(path: &str, duration: gst::ClockTime, firsts: [Duration; 2], ran: Duration) {
    let secs = duration.seconds_f64();
    assert!(
        secs <= ran.as_secs_f64() + 1.0 && secs >= 1.0,
        "{path} claims {duration} for about {ran:?} of recording"
    );
    if path.ends_with(".mkv") {
        let cluster = first_cluster(path);
        assert!(cluster < Duration::from_millis(500), "{path}: the first cluster is at {cluster:?}");
    }
    let [video, audio] = firsts;
    assert!(video < Duration::from_millis(500), "{path}: the picture starts at {video:?}");
    assert!(audio < Duration::from_millis(500), "{path}: the sound starts at {audio:?}");
    let apart = video.abs_diff(audio);
    assert!(apart < Duration::from_millis(100), "{path}: sound and picture start {apart:?} apart");
}

/// Demux the file and read the stream time of the first video and the first
/// audio buffer, the positions a player shows for them. The queues let one
/// sink preroll while the demuxer is still reading the other's fragment.
fn first_times(path: &str) -> [Duration; 2] {
    let location = path.replace('\\', "/");
    let pipeline = gst::parse::launch(&format!(
        "filesrc location=\"{location}\" ! parsebin name=p \
         queue name=qv max-size-time=0 max-size-buffers=0 max-size-bytes=0 \
         ! appsink name=v sync=false caps=video/x-h264 \
         queue name=qa max-size-time=0 max-size-buffers=0 max-size-bytes=0 \
         ! appsink name=a sync=false caps=audio/mpeg"
    ))
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let parse = pipeline.by_name("p").unwrap();
    let weak = pipeline.downgrade();
    parse.connect_pad_added(move |_, pad| {
        let Some(pipeline) = weak.upgrade() else { return };
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let video = caps.structure(0).is_some_and(|s| s.name().starts_with("video/"));
        let sink = if video { "qv" } else { "qa" };
        let sink = pipeline.by_name(sink).unwrap();
        let _ = pad.link(&sink.static_pad("sink").unwrap());
    });
    pipeline.set_state(gst::State::Playing).unwrap();
    let first = |name: &str| {
        let sink = pipeline.by_name(name).unwrap().downcast::<AppSink>().unwrap();
        let sample = sink
            .try_pull_preroll(gst::ClockTime::from_seconds(10))
            .unwrap_or_else(|| panic!("{path} has no {name} stream"));
        let segment = sample.segment().unwrap().downcast_ref::<gst::ClockTime>().unwrap();
        let pts = sample.buffer().unwrap().pts().unwrap();
        let at = segment.to_stream_time(pts).unwrap_or(pts);
        Duration::from_nanos(at.nseconds())
    };
    let times = [first("v"), first("a")];
    pipeline.set_state(gst::State::Null).unwrap();
    times
}

/// The timecode written on a Matroska file's first cluster. GStreamer's
/// demuxer subtracts it before anything downstream sees a time, so only the
/// bytes show it, and other players start their seek bar there.
fn first_cluster(path: &str) -> Duration {
    let bytes = std::fs::read(path).unwrap();
    let at = bytes
        .windows(4)
        .position(|w| w == [0x1f, 0x43, 0xb6, 0x75])
        .expect("the file has a cluster");
    let (_, size) = vint(&bytes[at + 4..]);
    let rest = &bytes[at + 4 + size..];
    assert_eq!(rest[0], 0xe7, "a cluster opens with its timecode");
    let (len, size) = vint(&rest[1..]);
    let value = rest[1 + size..1 + size + len as usize]
        .iter()
        .fold(0u64, |acc, b| acc << 8 | u64::from(*b));
    // GStreamer writes the default timecode scale, a millisecond.
    Duration::from_millis(value)
}

/// An EBML variable length integer: its value and how many bytes it took.
fn vint(bytes: &[u8]) -> (u64, usize) {
    let size = bytes[0].leading_zeros() as usize + 1;
    let first = u64::from(bytes[0]) & (0xff >> size);
    let value = bytes[1..size].iter().fold(first, |acc, b| acc << 8 | u64::from(*b));
    (value, size)
}
