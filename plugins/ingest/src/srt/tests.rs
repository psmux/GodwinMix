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
///
/// A second of SRT latency here and in the player below, not the default
/// 120 ms: libsrt drops a packet that arrives later than the latency, and on
/// a runner with every core busy that was most of them, so a player saw its
/// first keyframe and nothing after it. The tests are about who is let in
/// and what is sent, not about how little delay a busy machine manages.
fn caller(port: u16, streamid: &str, passphrase: &str) -> Option<gst::Element> {
    gmx_netkit::init().ok()?;
    let pass = if passphrase.is_empty() { String::new() } else { format!("passphrase={passphrase}") };
    let line = format!(
        "videotestsrc is-live=true ! video/x-raw,width=320,height=240,framerate=30/1 ! \
         x264enc tune=zerolatency speed-preset=ultrafast key-int-max=15 ! h264parse ! mux. \
         audiotestsrc is-live=true ! audioconvert ! avenc_aac ! aacparse ! mux. \
         mpegtsmux name=mux ! srtsink uri=srt://127.0.0.1:{port} mode=caller latency=1000 streamid={streamid} {pass}"
    );
    let pipeline = gst::parse::launch(&line).ok()?;
    pipeline.set_state(gst::State::Playing).ok()?;
    Some(pipeline)
}

fn stop(pipeline: gst::Element) {
    let _ = pipeline.set_state(gst::State::Null);
}

/// Ten seconds, times `GODWINMIX_TIMING_SLACK` on a runner that declares
/// itself slow: with every core busy, as the rest of the suite keeps them, a
/// player decoded one frame in ten seconds and thirty alone in three.
fn wait_for(what: impl Fn() -> bool) -> bool {
    let slack = std::env::var("GODWINMIX_TIMING_SLACK").ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0).max(1.0);
    let until = Instant::now() + Duration::from_secs(10).mul_f64(slack);
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
    // Both kinds: on a busy runner the picture's size was read and the
    // sound's codec had not been yet.
    let coded = wait_for(|| gate.hub.stream("church", "cam2").is_some_and(|s| s["video"]["width"] == 320 && !s["audio"].is_null()));
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

/// A player on the same port as the encoder: GStreamer's `srtsrc` as a
/// caller with `m=request`, which is what vMix, OBS and `srt-live-transmit`
/// send, decodes the channel's stream; one asking for a stream that is not on
/// air is turned away.
#[test]
fn a_player_on_the_publishers_port_is_sent_the_stream() {
    let Some((server, gate)) = server() else { return };
    let Some(encoder) = caller(server.port(), "church/main", KEY_ONE) else {
        eprintln!("skipping: GStreamer has no srtsink or x264enc");
        return;
    };
    assert!(wait_for(|| gate.hub.is_live("church", "main")), "the encoder is on air");
    let line = format!(
        "srtsrc name=src uri=\"srt://127.0.0.1:{}?mode=caller\" latency=1000 streamid=\"#!::r=church/main,m=request\" passphrase={KEY_ONE} \
         ! tsdemux ! h264parse ! avdec_h264 ! fakesink name=end",
        server.port()
    );
    let player = gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap();
    let frames = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let f = frames.clone();
    player.by_name("end").unwrap().static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        f.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    player.set_state(gst::State::Playing).unwrap();
    let played = wait_for(|| frames.load(Ordering::Relaxed) >= 30);
    // What the player's own pipeline said, for the message: a tsdemux that
    // found no picture and an srtsrc cut off read differently.
    let mut said = Vec::new();
    while let Some(m) = player.bus().and_then(|b| b.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Warning])) {
        let src = m.src().map(|s| s.name().to_string()).unwrap_or_default();
        match m.view() {
            gst::MessageView::Error(e) => said.push(format!("{src}: {}", e.error())),
            gst::MessageView::Warning(w) => said.push(format!("{src}: {}", w.error())),
            _ => {}
        }
    }
    // And what libsrt counted on the player's socket: nothing received reads
    // differently from packets that came and were dropped as late.
    let counted = player.by_name("src").map(|s| s.property::<gst::Structure>("stats").to_string());
    let _ = player.set_state(gst::State::Null);
    let refused = decide::decide(
        &gate.table.read().unwrap(),
        &gate.hub,
        &streamid::parse("#!::r=church/nothere,m=request").unwrap(),
    );
    stop(encoder);
    assert!(played, "the player decoded {} frames; its pipeline said {said:?}; srtsrc counted {counted:?}", frames.load(Ordering::Relaxed));
    assert!(matches!(refused, decide::Decision::Refuse { code: decide::NOT_FOUND, .. }), "{refused:?}");
}
