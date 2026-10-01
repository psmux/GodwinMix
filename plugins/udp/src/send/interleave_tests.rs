//! A programme whose audio runs well ahead of its video in the Matroska the
//! core writes, which is what a busy machine or a slow video encoder makes.
//!
//! `mpegtsmux` waits for a buffer on every pad before it writes one, so the
//! branch that is ahead fills its queue and the demuxer stops on it. With
//! queues of a second, two seconds of audio ahead of the video was enough to
//! stop the sender for good: the video queue sat empty behind the demuxer,
//! nothing was read from the FIFO again, and the core's side of the FIFO
//! filled. Caught by dev/bench/scale.sh on 2026-10-01, where it looked like
//! an output stopping when its receiver went away.

use super::tests::{frame_hashes, mkfifo, tools, Scratch};
use super::*;
use serde_json::json;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const FRAMES: u32 = 150;

/// Five seconds of video and seven of audio into streamable Matroska, in real
/// time, with the video starting two seconds after the audio, so the file
/// carries two seconds of audio before the first video frame. That is the
/// shape the core writes when an output is added in the middle of a GOP: the
/// video waits for the next keyframe and the audio does not.
fn audio_ahead_writer(ffmpeg: &Path, fifo: &Path) -> Child {
    let line = format!(
        "-hide_banner -loglevel error -y -re -f lavfi -i sine=frequency=440:duration=7 \
         -itsoffset 2 -f lavfi -i testsrc2=size=320x240:rate=30:duration=5 \
         -map 1:v -map 0:a -c:v libx264 -g 30 -c:a aac -f matroska -live 1 {}",
        fifo.display()
    );
    Command::new(ffmpeg)
        .args(line.split_whitespace())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ffmpeg starts")
}

/// Wait for a child, killing it at the deadline. True when it ended by itself.
fn wait_within(child: &mut Child, within: Duration) -> bool {
    let until = Instant::now() + within;
    while Instant::now() < until {
        if let Ok(Some(_)) = child.try_wait() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}

/// Every datagram into a file, until three seconds go by with none. ffmpeg's
/// own UDP input is not used: with two seconds of audio before any video it
/// gives up looking for the video stream before the video arrives.
fn record(socket: std::net::UdpSocket, into: &Path) -> u64 {
    use std::io::Write;
    socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut file = std::fs::File::create(into).unwrap();
    let (mut buf, mut total) = (vec![0u8; 65536], 0u64);
    let started = Instant::now();
    // The first datagram may be several seconds off; the timeout counts once
    // something has arrived, within a minute overall.
    while started.elapsed() < Duration::from_secs(60) {
        match socket.recv(&mut buf) {
            Ok(n) => {
                file.write_all(&buf[..n]).unwrap();
                total += n as u64;
            }
            Err(_) if total > 0 => break,
            Err(_) => {}
        }
    }
    total
}

#[test]
fn audio_two_seconds_ahead_of_video_does_not_stop_the_sender() {
    let Some((_, ffmpeg)) = tools() else {
        eprintln!("skipping: needs gst-launch-1.0 and ffmpeg");
        return;
    };
    let dir = Scratch::new("audio-ahead");
    let (fifo, got) = (dir.at("programme"), dir.at("got.ts"));
    mkfifo(&fifo);
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    let settings = Settings::from_params(&json!({"uri": format!("udp://127.0.0.1:{port}")})).unwrap();
    let rx = std::thread::spawn({
        let got = got.clone();
        move || record(socket, &got)
    });
    let fd = godwinmix_capture_common::fifo::open_read(&fifo).unwrap();
    let sender = Sender::start(&settings, fd, None).expect("the sender starts");
    let mut tx = audio_ahead_writer(&ffmpeg, &fifo);
    // Seven seconds of programme, written in real time. A sender that has stopped
    // reading leaves the writer blocked on the FIFO for ever.
    let finished = wait_within(&mut tx, Duration::from_secs(30));
    std::thread::sleep(Duration::from_millis(500));
    let sent = sender.bytes_sent();
    drop(sender);
    let received = rx.join().unwrap();
    assert!(finished, "the writer was still blocked on the FIFO after 30 s: the sender stopped reading ({sent} bytes sent)");
    let frames = frame_hashes(&ffmpeg, &got).len();
    assert!(frames as u32 >= FRAMES - 5, "ffmpeg decoded {frames} of {FRAMES} frames; the sender sent {sent} bytes and {received} arrived");
}
