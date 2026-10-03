//! The output fed the way the core feeds it (streamable Matroska on a FIFO,
//! written in real time) and pulled by real RTSP clients: ffmpeg over TCP and
//! over UDP, and GStreamer's `uridecodebin`, which is what the core's own
//! `hls/source` opens an `rtsp://` address with.

use crate::feed::Track;
use crate::output::serve;
use crate::server::launch;
use crate::settings::Settings;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

fn which(tool: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?.to_str()?.split(':').map(|d| Path::new(d).join(tool)).find(|p| p.is_file())
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("gmx-rtsp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    fn at(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Wait for a receiver, and kill it past `secs` so a test never hangs.
fn wait_or_kill(child: &mut Child, secs: u64) {
    let until = std::time::Instant::now() + Duration::from_secs(secs);
    while std::time::Instant::now() < until {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// What the core does: encode live, mux streamable Matroska into the FIFO.
/// The same encode also lands in `reference`, to compare against.
fn core_like_writer(fifo: &Path, reference: &Path, frames: u32) -> Child {
    let line = format!(
        "videotestsrc is-live=true num-buffers={frames} pattern=ball ! \
         video/x-raw,format=I420,width=320,height=240,framerate=30/1 ! x264enc tune=zerolatency key-int-max=30 ! \
         h264parse ! tee name=t ! queue ! mux. t. ! queue ! matroskamux ! filesink location=\"{}\" \
         audiotestsrc is-live=true num-buffers={} ! avenc_aac ! aacparse ! queue ! mux. \
         matroskamux name=mux streamable=true ! filesink location=\"{}\"",
        reference.display().to_string().replace('\\', "/"),
        // As long as the video: 1024 samples a buffer at 44.1 kHz.
        frames * 44_100 / 1024 / 30,
        fifo.display().to_string().replace('\\', "/")
    );
    Command::new("gst-launch-1.0").arg("-q").args(line.split_whitespace()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap()
}

/// The decoded video frame hashes, one per frame, from ffmpeg's framemd5.
fn hashes(file: &Path) -> Vec<String> {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(file)
        .args(["-map", "0:v", "-fps_mode", "passthrough", "-f", "framemd5", "-"])
        .output()
        .unwrap();
    md5_lines(&String::from_utf8_lossy(&out.stdout))
}

fn md5_lines(text: &str) -> Vec<String> {
    text.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.rsplit(',').next().map(|h| h.trim().to_string())).collect()
}

/// Serve 8 s of programme and pull it with `pull`; the frames that arrived,
/// the frames the encoder made, and what `pull` printed.
fn serve_and_pull(name: &str, pull: impl FnOnce(&str, &Path) -> Child) -> Option<(Vec<String>, String)> {
    for tool in ["gst-launch-1.0", "ffmpeg", "mkfifo"] {
        if which(tool).is_none() {
            eprintln!("skipping: needs {tool}");
            return None;
        }
    }
    let dir = Scratch::new(name);
    let (fifo, reference, got) = (dir.at("programme"), dir.at("reference.mkv"), dir.at("got.md5"));
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let port = free_port();
    let s = Settings::from_params(&json!({"port": port, "path": "live", "bind": "127.0.0.1"})).unwrap();
    let fd = godwinmix_capture_common::fifo::open_read(&fifo).unwrap();
    let running = serve(&s, fd, None).expect("the server starts");
    let mut tx = core_like_writer(&fifo, &reference, 240);
    // A player that asks before the programme has arrived is told 404, and
    // tries again; this one waits until it would be answered.
    let until = std::time::Instant::now() + Duration::from_secs(10);
    while !running.0.mounted() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut rx = pull(&s.url("127.0.0.1"), &got);
    let _ = tx.wait();
    wait_or_kill(&mut rx, 15);
    drop(running);
    Some((hashes(&reference), std::fs::read_to_string(&got).unwrap_or_default()))
}

fn ffmpeg_pull(transport: &'static str) -> impl FnOnce(&str, &Path) -> Child {
    move |url, out| {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-timeout", "5000000", "-rtsp_transport", transport, "-i", url, "-t", "4", "-map", "0:v", "-fps_mode", "passthrough", "-f", "framemd5"])
            .arg(out)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }
}

/// Every frame that arrived is one the encoder made, in order and with none
/// missing between the first and the last.
fn assert_a_clean_run(want: &[String], got: &[String], at_least: usize) {
    assert!(got.len() >= at_least, "only {} frames arrived; wanted {at_least}", got.len());
    let start = want.iter().position(|h| h == &got[0]).expect("the first frame received was never encoded");
    assert_eq!(&want[start..start + got.len()], got, "the frames are not the encoder's, in order");
}

#[test]
fn ffmpeg_pulls_over_tcp_and_gets_the_encoders_frames_untouched() {
    let Some((want, got)) = serve_and_pull("tcp", ffmpeg_pull("tcp")) else { return };
    assert_a_clean_run(&want, &md5_lines(&got), 90);
}

#[test]
fn ffmpeg_pulls_over_udp_as_well() {
    let Some((want, got)) = serve_and_pull("udp", ffmpeg_pull("udp")) else { return };
    assert_a_clean_run(&want, &md5_lines(&got), 90);
}

/// The core pulls `rtsp://` with `hls/source`, which is `uridecodebin`: the
/// same element here, decoding what this output serves.
#[test]
fn uridecodebin_decodes_it_as_the_cores_own_rtsp_source_would() {
    let pull = |url: &str, out: &Path| {
        let line = format!("uridecodebin uri={url} caps=video/x-raw(ANY) ! fakesink silent=false");
        Command::new("gst-launch-1.0")
            .arg("-v")
            .args(line.split_whitespace())
            .stdout(std::fs::File::create(out).unwrap())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let Some((_, got)) = serve_and_pull("uridecode", pull) else { return };
    let frames = got.lines().filter(|l| l.contains("last-message = chain")).count();
    assert!(frames >= 60, "uridecodebin decoded {frames} frames from the RTSP output; wanted 60");
}

#[test]
fn the_launch_line_packetises_what_the_programme_carries() {
    gmx_netkit::init().unwrap();
    let caps = gstreamer::Caps::new_empty_simple("video/x-h264");
    let v = Track { caps: caps.clone(), parse: "h264parse", pay: "rtph264pay" };
    let a = Track { caps, parse: "aacparse", pay: "rtpmp4gpay" };
    let both = launch(Some(v.clone()), Some(a.clone())).unwrap();
    assert!(both.contains("rtph264pay name=pay0 pt=96 config-interval=-1"), "{both}");
    assert!(both.contains("rtpmp4gpay name=pay1 pt=97"), "{both}");
    assert!(launch(None, Some(a)).unwrap().contains("rtpmp4gpay name=pay0 pt=96"), "audio alone is pay0");
    assert!(launch(None, None).is_none());
}

#[test]
fn the_shipped_manifest_passes_the_validator_the_harness_runs() {
    use godwinmix_sdk::prelude::*;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = Manifest::load(root.join("gmx-plugin.toml")).expect("the manifest must validate");
    assert_eq!(manifest.plugin.name, "rtsp");
}
