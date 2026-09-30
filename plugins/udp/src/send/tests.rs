//! The sender fed the way the core feeds it (streamable Matroska on a FIFO,
//! written in real time) and received by ffmpeg, which is what most of the
//! world's UDP receivers are built on.

use super::*;
use crate::recv::tests::which;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("gmx-udp-send-{name}-{}", std::process::id()));
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

fn tools() -> Option<(PathBuf, PathBuf)> {
    Some((which("gst-launch-1.0")?, which("ffmpeg")?))
}

fn free_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// What the core does: encode live, mux streamable Matroska into the FIFO.
/// The same encode also lands in `reference`, to compare against.
fn core_like_writer(gst: &Path, fifo: &Path, reference: &Path, frames: u32) -> Child {
    let line = format!(
        "videotestsrc is-live=true num-buffers={frames} pattern=ball ! \
         video/x-raw,width=320,height=240,framerate=30/1 ! x264enc tune=zerolatency key-int-max=30 ! \
         h264parse ! tee name=t ! queue ! matroskamux streamable=true ! filesink location={} \
         t. ! queue ! matroskamux ! filesink location={}",
        fifo.display(),
        reference.display()
    );
    Command::new(gst)
        .arg("-q")
        .args(line.split_whitespace())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("gst-launch-1.0 starts")
}

fn ffmpeg_receive(ffmpeg: &Path, uri: &str, out: &Path) -> Child {
    Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-probesize", "65536", "-analyzeduration", "1000000", "-i", uri, "-c", "copy", "-f", "mpegts"])
        .arg(out)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ffmpeg starts")
}

/// The decoded video frame hashes, one per frame, from ffmpeg's framemd5.
fn frame_hashes(ffmpeg: &Path, file: &Path) -> Vec<String> {
    let out = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(file)
        .args(["-map", "0:v", "-fps_mode", "passthrough", "-f", "framemd5", "-"])
        .output()
        .expect("ffmpeg runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.rsplit(',').next().map(|h| h.trim().to_string()))
        .collect()
}

fn mkfifo(path: &Path) {
    let ok = Command::new("mkfifo").arg(path).status().map(|s| s.success()).unwrap_or(false);
    assert!(ok, "mkfifo failed");
}

/// Who is listening at the far end.
#[derive(Clone, Copy, PartialEq)]
enum Far {
    Ffmpeg,
    /// This plugin's own `udp/source`, for RTP: ffmpeg's RTP input throws
    /// away the first GOP while it probes, which says nothing about the sender.
    Ours,
}

fn send_and_receive(name: &str, scheme: &str, extra: serde_json::Value, frames: u32, far: Far) -> Option<(Vec<String>, Vec<String>, u64)> {
    let (gst, ffmpeg) = tools()?;
    let dir = Scratch::new(name);
    let (fifo, reference, got) = (dir.at("programme"), dir.at("reference.mkv"), dir.at("got.ts"));
    mkfifo(&fifo);
    let port = free_port();
    let mut params = json!({"uri": format!("{scheme}://127.0.0.1:{port}")});
    params.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    let settings = Settings::from_params(&params).unwrap();
    // ffmpeg ends on five seconds of silence, which is also what makes it
    // flush the last frame: a TS demuxer only knows a PES is whole when the
    // next one starts, or the input ends.
    let mut rx = (far == Far::Ffmpeg)
        .then(|| ffmpeg_receive(&ffmpeg, &format!("{scheme}://127.0.0.1:{port}?timeout=5000000"), &got));
    let ours = (far == Far::Ours).then(|| {
        let s = crate::recv::settings::Settings::from_params(&json!({"uri": format!("{scheme}://127.0.0.1:{port}")})).unwrap();
        crate::recv::Receiver::start(&s, None, crate::recv::Sink::File(got.clone())).unwrap()
    });
    std::thread::sleep(Duration::from_millis(500));
    let fd = godwinmix_capture_common::fifo::open_read(&fifo).unwrap();
    let sender = Sender::start(&settings, fd, None).expect("the sender starts");
    let mut tx = core_like_writer(&gst, &fifo, &reference, frames);
    let _ = tx.wait();
    std::thread::sleep(Duration::from_millis(500));
    let sent = sender.bytes_sent();
    drop(sender);
    drop(ours);
    if let Some(rx) = rx.as_mut() {
        let _ = rx.wait();
    }
    Some((frame_hashes(&ffmpeg, &reference), frame_hashes(&ffmpeg, &got), sent))
}

#[test]
fn ffmpeg_receives_every_frame_exactly_as_it_was_encoded() {
    let Some((want, got, _)) = send_and_receive("every-frame", "udp", json!({}), 150, Far::Ffmpeg) else {
        eprintln!("skipping: needs gst-launch-1.0 and ffmpeg");
        return;
    };
    assert_eq!(want.len(), 150, "the reference encode is short");
    assert_eq!(got, want, "ffmpeg decoded {} frames, the encoder made {}", got.len(), want.len());
}

#[test]
fn rtp_wrapped_output_arrives_frame_for_frame_at_an_rtp_receiver() {
    let Some((want, got, _)) = send_and_receive("rtp", "rtp", json!({}), 90, Far::Ours) else {
        eprintln!("skipping: needs gst-launch-1.0 and ffmpeg");
        return;
    };
    assert_eq!(got, want);
}

#[test]
fn a_constant_bitrate_is_padded_up_to_the_rate_asked_for() {
    // Three seconds of a small picture is a few hundred kbit/s; asked for
    // 2 Mbit/s, the rest has to be null packets.
    let Some((want, got, sent)) = send_and_receive("cbr", "udp", json!({"cbr_kbps": 2000}), 90, Far::Ffmpeg) else {
        eprintln!("skipping: needs gst-launch-1.0 and ffmpeg");
        return;
    };
    assert_eq!(got, want);
    let mbits = sent as f64 * 8.0 / 1e6;
    assert!((5.0..7.5).contains(&mbits), "3 s at 2 Mbit/s should be about 6 Mbit, sent {mbits:.2}");
}
