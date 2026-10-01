//! A receiver that goes away and comes back. UDP has no far end to refuse:
//! sending to a port nobody is listening on is not an error, the sender
//! carries on at the programme's rate, and a receiver that binds the port
//! again picks the stream up where it is.

use super::tests::{mkfifo, tools, Scratch};
use super::*;
use serde_json::json;
use std::net::UdpSocket;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Bytes that arrive on `socket` in `window`.
fn take_for(socket: &UdpSocket, window: Duration) -> u64 {
    socket.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let (mut buf, mut total) = (vec![0u8; 65536], 0u64);
    let until = Instant::now() + window;
    while Instant::now() < until {
        if let Ok(n) = socket.recv(&mut buf) {
            total += n as u64;
        }
    }
    total
}

#[test]
fn a_receiver_that_goes_away_and_comes_back_gets_the_stream_again() {
    let Some((_, ffmpeg)) = tools() else {
        eprintln!("skipping: needs gst-launch-1.0 and ffmpeg");
        return;
    };
    let dir = Scratch::new("receiver-away");
    let fifo = dir.at("programme");
    mkfifo(&fifo);
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    let settings = Settings::from_params(&json!({"uri": format!("udp://127.0.0.1:{port}")})).unwrap();
    let fd = godwinmix_capture_common::fifo::open_read(&fifo).unwrap();
    let sender = Sender::start(&settings, fd, None).expect("the sender starts");
    // Twenty seconds of programme in real time, audio and video, the way the
    // core writes it.
    let line = format!(
        "-hide_banner -loglevel error -y -re -f lavfi -i sine=frequency=440:duration=20 \
         -f lavfi -i testsrc2=size=320x240:rate=30:duration=20 \
         -map 1:v -map 0:a -c:v libx264 -g 30 -c:a aac -f matroska -live 1 {}",
        fifo.display()
    );
    let mut writer = Command::new(&ffmpeg)
        .args(line.split_whitespace())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ffmpeg starts");

    std::thread::sleep(Duration::from_secs(2));
    let before = take_for(&socket, Duration::from_secs(3));
    // The receiver goes away. Nothing is listening on the port now.
    drop(socket);
    let sent_at_drop = sender.bytes_sent();
    std::thread::sleep(Duration::from_secs(4));
    let sent_while_away = sender.bytes_sent() - sent_at_drop;
    let health_while_away = sender.health();
    // And comes back on the same port.
    let socket = UdpSocket::bind(("127.0.0.1", port)).expect("the port is free again");
    let after = take_for(&socket, Duration::from_secs(3));

    let _ = writer.kill();
    let _ = writer.wait();
    drop(sender);
    assert!(before > 100_000, "the receiver got {before} bytes before it went away");
    assert!(sent_while_away > 100_000, "the sender sent {sent_while_away} bytes while nobody listened; it should carry on");
    assert_eq!(
        health_while_away.state,
        godwinmix_sdk::wire::HealthState::Ok,
        "nobody listening is not a fault: {:?}",
        health_while_away.detail
    );
    assert!(after > 100_000, "the receiver got {after} bytes after it came back");
}
