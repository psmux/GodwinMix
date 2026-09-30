//! A real libsrt listener, and real GStreamer SRT callers publishing to it.

use super::*;
use crate::channels::Table;
use crate::hub::Hub;
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;
use std::sync::{OnceLock, RwLock};

const KEY_ONE: &str = "first-key-0123456789";
const KEY_TWO: &str = "second-key-0123456789";

fn gate() -> Arc<ChannelGate> {
    let table = Table::from_params(&json!({"channels": [
        {"id": "church", "app": "church", "protocols": ["srt"],
         "keys": [{"id": "obs", "secret": KEY_ONE}, {"id": "phone", "secret": KEY_TWO}]},
    ]}));
    Arc::new(ChannelGate {
        hub: Hub::new(),
        table: Arc::new(RwLock::new(table)),
        open_app: String::new(),
        relay: OnceLock::new(),
        reporter: None,
        on_air: Default::default(),
    })
}

fn free_udp_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// An SRT caller sending a small H.264 and AAC picture in MPEG-TS.
fn caller(port: u16, streamid: &str, passphrase: &str) -> Option<gst::Element> {
    gmx_netkit::init().ok()?;
    let pass = if passphrase.is_empty() { String::new() } else { format!("passphrase={passphrase}") };
    let line = format!(
        "videotestsrc is-live=true ! video/x-raw,width=320,height=240,framerate=30/1 ! \
         x264enc tune=zerolatency speed-preset=ultrafast key-int-max=15 ! h264parse ! mux. \
         audiotestsrc is-live=true ! audioconvert ! avenc_aac ! aacparse ! mux. \
         mpegtsmux name=mux ! srtsink uri=srt://127.0.0.1:{port} mode=caller streamid={streamid} {pass}"
    );
    let pipeline = gst::parse::launch(&line).ok()?;
    pipeline.set_state(gst::State::Playing).ok()?;
    Some(pipeline)
}

fn stop(pipeline: gst::Element) {
    let _ = pipeline.set_state(gst::State::Null);
}

fn wait_for(what: impl Fn() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn server() -> Option<(SrtServer, Arc<ChannelGate>)> {
    if ffi::lib().is_err() {
        eprintln!("skipping: no libsrt on this machine");
        return None;
    }
    let gate = gate();
    let server = SrtServer::bind("127.0.0.1", free_udp_port(), gate.clone()).expect("a free UDP port");
    Some((server, gate))
}

#[test]
fn two_callers_on_one_port_are_two_streams_of_one_channel() {
    let Some((server, gate)) = server() else { return };
    let Some(one) = caller(server.port(), "church/main", KEY_ONE) else {
        eprintln!("skipping: GStreamer has no srtsink or x264enc");
        return;
    };
    let two = caller(server.port(), "#!::r=church/cam2,m=publish,u=phone", KEY_TWO).unwrap();
    let both = wait_for(|| gate.hub.is_live("church", "main") && gate.hub.is_live("church", "cam2"));
    let coded = wait_for(|| gate.hub.stream("church", "cam2").is_some_and(|s| s["video"]["width"] == 320));
    let described = gate.hub.stream("church", "cam2");
    stop(one);
    stop(two);
    assert!(both, "both callers were live together");
    assert!(coded, "the stream's codec was read from its tags: {described:?}");
    let described = described.unwrap();
    assert_eq!(described["protocol"], "srt");
    assert_eq!(described["key"], "phone");
    assert_eq!(described["audio"]["codec"], "aac");
}

#[test]
fn a_wrong_passphrase_is_refused_by_libsrt_and_a_key_in_the_id_needs_none() {
    let Some((server, gate)) = server() else { return };
    let Some(wrong) = caller(server.port(), "church/main", "not-the-key-at-all") else { return };
    let keyed = caller(server.port(), &format!("church/cam2?psk={KEY_ONE}"), "").unwrap();
    let in_by_id = wait_for(|| gate.hub.is_live("church", "cam2"));
    let wrong_in = gate.hub.is_live("church", "main");
    stop(wrong);
    stop(keyed);
    assert!(in_by_id, "a key in the stream id let the caller in");
    assert!(!wrong_in, "a caller with the wrong passphrase got in");
}

#[test]
fn a_caller_leaves_and_the_stream_ends_and_closing_the_port_ends_the_listener() {
    let Some((server, gate)) = server() else { return };
    let Some(one) = caller(server.port(), "church/main", KEY_ONE) else { return };
    assert!(wait_for(|| gate.hub.is_live("church", "main")));
    stop(one);
    assert!(wait_for(|| !gate.hub.is_live("church", "main")), "the stream ended with its caller");
    let port = server.port();
    drop(server);
    // libsrt lets go of the UDP socket from its own collector thread, within
    // a second or two of the last SRT socket on it closing.
    assert!(wait_for(|| std::net::UdpSocket::bind(("127.0.0.1", port)).is_ok()), "the port is free again");
}

/// An HEVC encoder over SRT: its stream goes on the hub as enhanced RTMP
/// HEVC, with its size read, rather than being turned away.
#[test]
fn an_hevc_caller_is_carried_as_enhanced_rtmp_hevc() {
    let Some((server, gate)) = server() else { return };
    gmx_netkit::init().unwrap();
    if gst::ElementFactory::find("x265enc").is_none() {
        eprintln!("skipping: no x265enc");
        return;
    }
    let line = format!(
        "videotestsrc is-live=true ! video/x-raw,format=I420,width=320,height=240,framerate=30/1 ! \
         x265enc tune=zerolatency speed-preset=ultrafast key-int-max=15 ! h265parse config-interval=-1 ! mux. \
         audiotestsrc is-live=true ! audioconvert ! avenc_aac ! aacparse ! mux. \
         mpegtsmux name=mux ! srtsink uri=srt://127.0.0.1:{} mode=caller streamid=church/hevc passphrase={KEY_ONE}",
        server.port()
    );
    let pipeline = gst::parse::launch(&line).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let coded = wait_for(|| gate.hub.stream("church", "hevc").is_some_and(|s| s["video"]["width"] == 320));
    let described = gate.hub.stream("church", "hevc");
    stop(pipeline);
    assert!(coded, "the HEVC stream's size was read: {described:?}");
    assert_eq!(described.unwrap()["video"]["codec"], "h265");
}
