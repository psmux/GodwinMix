//! Both halves against servers that behave like the real ones: an Icecast
//! server in this test that checks the source login and keeps what it is
//! sent, and a radio station that streams MP3 with ICY song titles. What
//! arrives is decoded by GStreamer and measured.

use crate::radio::{Radio, Sink};
#[cfg(unix)]
use crate::send::Sender;
#[cfg(unix)]
use crate::settings::Settings;
#[cfg(unix)]
use base64::Engine;
use gstreamer as gst;
use gstreamer::prelude::*;
#[cfg(unix)]
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// An Icecast server: answers a source that logs in as `source:hackme` and
/// keeps every byte it is sent; refuses anyone else.
/// The port, what the mount was sent, and the source's request.
#[cfg(unix)]
type Server = (u16, Arc<Mutex<Vec<u8>>>, Arc<Mutex<String>>);

#[cfg(unix)]
fn icecast() -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (got, head) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(String::new())));
    let (g, h) = (got.clone(), head.clone());
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let (g, h) = (g.clone(), h.clone());
            std::thread::spawn(move || take(conn, &g, &h));
        }
    });
    (port, got, head)
}

#[cfg(unix)]
fn take(mut conn: std::net::TcpStream, got: &Mutex<Vec<u8>>, head: &Mutex<String>) {
    let mut raw = Vec::new();
    let mut buf = vec![0u8; 8192];
    while !String::from_utf8_lossy(&raw).contains("\r\n\r\n") {
        match conn.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
        }
    }
    let text = String::from_utf8_lossy(&raw).to_string();
    let (request, rest) = text.split_once("\r\n\r\n").unwrap();
    if request.starts_with("PUT") || request.starts_with("SOURCE") {
        *head.lock().unwrap() = request.to_string();
    }
    let login = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode("source:hackme"));
    if !request.contains(&login) {
        let _ = conn.write_all(b"HTTP/1.0 401 Unauthorized\r\n\r\n");
        return;
    }
    let _ = conn.write_all(b"HTTP/1.0 200 OK\r\n\r\n");
    got.lock().unwrap().extend_from_slice(rest.as_bytes());
    while let Ok(n) = conn.read(&mut buf) {
        if n == 0 {
            break;
        }
        got.lock().unwrap().extend_from_slice(&buf[..n]);
    }
}

/// Seconds of sound in `bytes`, decoded the way a listener's player would.
fn seconds_of(bytes: &[u8], demux: &str) -> f64 {
    let path = std::env::temp_dir().join(format!("gmx-icecast-{}-{}.bin", std::process::id(), bytes.len()));
    std::fs::write(&path, bytes).unwrap();
    let line = format!("filesrc location=\"{}\" ! {demux} ! audioconvert ! audio/x-raw,format=F32LE ! fakesink name=end sync=false", path.display().to_string().replace('\\', "/"));
    let p = gst::parse::launch(&line).unwrap();
    let samples = Arc::new(Mutex::new(0u64));
    let (s, pad) = (samples.clone(), p.downcast_ref::<gst::Bin>().unwrap().by_name("end").unwrap().static_pad("sink").unwrap());
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let rate_ch = pad.current_caps().and_then(|c| c.structure(0).map(|st| (st.get::<i32>("rate").unwrap_or(1), st.get::<i32>("channels").unwrap_or(1))));
        if let (Some(b), Some((_, ch))) = (info.buffer(), rate_ch) {
            *s.lock().unwrap() += b.size() as u64 / 4 / ch as u64;
        }
        gst::PadProbeReturn::Ok
    });
    p.set_state(gst::State::Playing).unwrap();
    let _ = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Eos, gst::MessageType::Error]);
    let _ = p.set_state(gst::State::Null);
    let _ = std::fs::remove_file(&path);
    let n = *samples.lock().unwrap();
    n as f64 / 44_100.0
}

/// The core's side: 4 s of live H.264 and AAC in streamable Matroska on a FIFO.
#[cfg(unix)]
fn programme_into(fifo: &std::path::Path) -> std::process::Child {
    let line = format!(
        "videotestsrc is-live=true num-buffers=120 ! video/x-raw,width=320,height=240,framerate=30/1 ! x264enc tune=zerolatency ! h264parse ! queue ! mux. \
         audiotestsrc is-live=true num-buffers=172 ! audio/x-raw,rate=44100 ! avenc_aac ! aacparse ! queue ! mux. \
         matroskamux name=mux streamable=true ! filesink location=\"{}\"",
        fifo.display().to_string().replace('\\', "/")
    );
    Command::new("gst-launch-1.0").arg("-q").args(line.split_whitespace()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap()
}

// The programme arrives on a FIFO made with mkfifo, which Windows does not have.
#[cfg(unix)]
#[test]
fn the_programmes_sound_reaches_an_icecast_mount_as_mp3_behind_the_source_login() {
    gmx_netkit::init().unwrap();
    let (port, got, head) = icecast();
    let dir = std::env::temp_dir().join(format!("gmx-icecast-out-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let fifo = dir.join("programme");
    let _ = std::fs::remove_file(&fifo);
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let s = Settings::from_params(&json!({"uri": format!("icecast://source:hackme@127.0.0.1:{port}/church.mp3"), "name": "Sunday"})).unwrap();
    let fd = godwinmix_capture_common::fifo::open_read(&fifo).unwrap();
    let sender = Sender::start(&s, fd, None).expect("the sender starts");
    let mut core = programme_into(&fifo);
    let _ = core.wait();
    std::thread::sleep(Duration::from_millis(800));
    drop(sender);
    let _ = std::fs::remove_dir_all(&dir);
    let request = head.lock().unwrap().clone();
    assert!(request.contains("/church.mp3"), "the mount: {request}");
    assert!(request.to_lowercase().contains("audio/mpeg"), "the type: {request}");
    let secs = seconds_of(&got.lock().unwrap(), "mpegaudioparse ! mpg123audiodec");
    assert!(secs >= 3.0, "the mount got {secs:.2} s of MP3 from 4 s of programme");
}

/// A station: MP3 with an ICY title every 8 KB, for ever.
fn station() -> String {
    let mp3 = {
        let p = gst::parse::launch("audiotestsrc num-buffers=200 ! audio/x-raw,rate=44100,channels=2 ! lamemp3enc ! appsink name=out sync=false").unwrap();
        let sink = p.downcast_ref::<gst::Bin>().unwrap().by_name("out").unwrap().downcast::<gstreamer_app::AppSink>().unwrap();
        p.set_state(gst::State::Playing).unwrap();
        let mut out = Vec::new();
        while let Some(s) = sink.try_pull_sample(gst::ClockTime::from_seconds(5)) {
            out.extend_from_slice(&s.buffer().unwrap().map_readable().unwrap());
        }
        let _ = p.set_state(gst::State::Null);
        Arc::new(out)
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/live.mp3", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut conn in listener.incoming().flatten() {
            let mp3 = mp3.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let _ = conn.read(&mut buf);
                let _ = conn.write_all(b"HTTP/1.0 200 OK\r\nContent-Type: audio/mpeg\r\nicy-name: Test FM\r\nicy-metaint: 8192\r\n\r\n");
                let meta = b"StreamTitle='The Choir - Anthem';";
                let mut block = vec![meta.len().div_ceil(16) as u8];
                block.extend_from_slice(meta);
                block.resize(1 + 16 * block[0] as usize, 0);
                for chunk in mp3.chunks_exact(8192).cycle() {
                    if conn.write_all(chunk).and_then(|_| conn.write_all(&block)).is_err() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(40));
                }
            });
        }
    });
    url
}

#[test]
fn a_radio_station_arrives_as_live_sound_with_its_song_title() {
    gmx_netkit::init().unwrap();
    let url = station();
    let out = std::env::temp_dir().join(format!("gmx-icecast-radio-{}.mkv", std::process::id()));
    let radio = Radio::start(&url, Sink::File(out.clone()), None).expect("the radio starts");
    std::thread::sleep(Duration::from_secs(3));
    let title = radio.title.lock().unwrap().clone();
    drop(radio);
    let bytes = std::fs::read(&out).unwrap_or_default();
    let _ = std::fs::remove_file(&out);
    assert_eq!(title, "The Choir - Anthem");
    let secs = seconds_of(&bytes, "matroskademux ! mpegaudioparse ! mpg123audiodec");
    assert!(secs >= 2.0, "{secs:.2} s of the station's sound arrived in 3 s");
}

#[test]
fn the_shipped_manifest_passes_the_validator_the_harness_runs() {
    use godwinmix_sdk::prelude::*;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let m = Manifest::load(root.join("gmx-plugin.toml")).expect("the manifest must validate");
    assert_eq!(m.provides.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["output", "source"]);
}
