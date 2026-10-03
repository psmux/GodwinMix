//! The source against a camera that behaves like one: an HTTP server in this
//! test serving real JPEGs, as an MJPEG stream and as a snapshot behind a
//! basic login. What the plugin writes is decoded by GStreamer, the way the
//! core would, and the pictures counted. Discovery finds a fake ONVIF camera
//! through the `Device` the core calls.

use crate::camera::{Camera, Sink};
use crate::discover::Discover;
use crate::onvif::soap::Login;
use crate::settings::Settings;
use base64::Engine;
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

/// Ten real JPEGs of a moving ball.
fn jpegs() -> Vec<Vec<u8>> {
    gmx_netkit::init().unwrap();
    let p = gst::parse::launch("videotestsrc num-buffers=10 pattern=ball ! video/x-raw,width=320,height=240 ! jpegenc ! appsink name=out sync=false").unwrap();
    let sink = p.downcast_ref::<gst::Bin>().unwrap().by_name("out").unwrap().downcast::<gstreamer_app::AppSink>().unwrap();
    p.set_state(gst::State::Playing).unwrap();
    let mut out = Vec::new();
    while let Some(s) = sink.try_pull_sample(gst::ClockTime::from_seconds(5)) {
        out.push(s.buffer().unwrap().map_readable().unwrap().to_vec());
    }
    let _ = p.set_state(gst::State::Null);
    out
}

/// A camera: `/video.mjpg` streams the pictures at 25 a second for ever,
/// `/snap.jpg` answers one picture to `admin:pw` and 401 to anyone else.
fn camera() -> String {
    let pictures = Arc::new(jpegs());
    let http = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", http.local_addr().unwrap());
    std::thread::spawn(move || {
        for conn in http.incoming().flatten() {
            let pictures = pictures.clone();
            std::thread::spawn(move || serve(conn, &pictures));
        }
    });
    base
}

fn serve(mut conn: std::net::TcpStream, pictures: &[Vec<u8>]) {
    let mut buf = vec![0u8; 8192];
    let n = conn.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]).to_string();
    if req.starts_with("GET /snap.jpg") {
        let good = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode("admin:pw"));
        if !req.contains(&good) {
            let _ = conn.write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"cam\"\r\nContent-Length: 0\r\n\r\n");
            return;
        }
        let pic = &pictures[0];
        let _ = write!(conn, "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", pic.len());
        let _ = conn.write_all(pic);
        return;
    }
    let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\n\r\n");
    for pic in pictures.iter().cycle() {
        let head = format!("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", pic.len());
        if conn.write_all(head.as_bytes()).and_then(|_| conn.write_all(pic)).and_then(|_| conn.write_all(b"\r\n")).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

/// Run the source for `secs` into a file, then count the pictures in it.
fn pictures_from(params: serde_json::Value, secs: u64) -> usize {
    let out = std::env::temp_dir().join(format!("gmx-ipcam-{}-{}.mkv", std::process::id(), params["uri"].as_str().unwrap().len()));
    let s = Settings::from_params(&params).unwrap();
    let cam = Camera::start(&s, Sink::File(out.clone()), None).expect("the camera starts");
    std::thread::sleep(Duration::from_secs(secs));
    drop(cam);
    let line = format!("filesrc location=\"{}\" ! matroskademux ! jpegdec ! fakesink name=end sync=false", out.display().to_string().replace('\\', "/"));
    let p = gst::parse::launch(&line).unwrap();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let c = count.clone();
    let pad = p.downcast_ref::<gst::Bin>().unwrap().by_name("end").unwrap().static_pad("sink").unwrap();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    p.set_state(gst::State::Playing).unwrap();
    let _ = p.bus().unwrap().timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Eos, gst::MessageType::Error]);
    let _ = p.set_state(gst::State::Null);
    let _ = std::fs::remove_file(&out);
    count.load(std::sync::atomic::Ordering::Relaxed)
}

#[test]
fn an_mjpeg_camera_arrives_as_pictures_the_core_can_decode() {
    let base = camera();
    let n = pictures_from(json!({"uri": format!("{base}/video.mjpg")}), 3);
    assert!(n >= 40, "decoded {n} pictures from 3 s of a 25 fps MJPEG camera; wanted 40");
}

#[test]
fn a_snapshot_camera_is_asked_behind_its_login() {
    let base = camera();
    let n = pictures_from(json!({"uri": format!("{base}/snap.jpg"), "fps": 5, "user": "admin", "password": "pw"}), 3);
    assert!(n >= 10, "decoded {n} snapshots from 3 s at 5 a second; wanted 10");
    let refused = crate::fetch::fetch(&format!("{base}/snap.jpg"), "admin", "wrong", Duration::from_secs(3));
    assert!(refused.is_err(), "a wrong password must not give a picture");
}

#[test]
fn the_shipped_manifest_passes_the_validator_the_harness_runs() {
    use godwinmix_sdk::prelude::*;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = Manifest::load(root.join("gmx-plugin.toml")).expect("the manifest must validate");
    let ids: Vec<&str> = manifest.provides.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["source", "discover"]);
}

#[test]
fn discovery_hands_the_core_ready_to_add_rtsp_streams() {
    let probe = crate::onvif::tests_camera();
    let mut d = Discover::aimed(probe, Login { user: "admin".into(), password: crate::onvif::TEST_PASSWORD.into() });
    let found = d.candidates(Duration::from_secs(3));
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].kind, "hls/source");
    assert!(found[0].params["uri"].as_str().unwrap().starts_with("rtsp://admin:"), "{:?}", found[0].params);
}
