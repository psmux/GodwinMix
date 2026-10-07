//! `rist/output` sending to a real RIST receiver: GStreamer's `ristsrc`,
//! opened through `uridecodebin` the way `hls/source` now opens `rist://`,
//! decoding what arrives. The receiver's RTCP is what makes the output say it
//! is connected.

use super::*;
use crate::plugin::Tier;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn even_port() -> u16 {
    let p = std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    if p % 2 == 0 { p } else { p - 1 }
}

#[test]
fn an_address_is_host_and_an_even_port() {
    assert_eq!(address("rist://10.0.0.5:5004").unwrap(), ("10.0.0.5".to_string(), 5004));
    assert_eq!(address("rist://[::1]:6000").unwrap(), ("::1".to_string(), 6000));
    let odd = address("rist://10.0.0.5:5005").unwrap_err().to_string();
    assert!(odd.contains("5004 or 5006"), "{odd}");
    assert!(address("rist://nohost").is_err());
    assert_eq!(claims("rist://x:5004"), Some(240));
    assert_eq!(claims("srt://x:5004"), None);
    // And the other way: a rist:// address added as a source is a live stream.
    let kind = crate::plugin::source::resolve("rist://0.0.0.0:5004").map(|p| p.manifest.provide_id());
    assert_eq!(kind.as_deref(), Some("hls/source"));
}

/// Frames a `uridecodebin` on `rist://` decodes, counted as they come.
///
/// On the loopback address, not 0.0.0.0. The port is one the system handed
/// out a moment ago, and the sender binds its own sockets to ports the
/// system hands out, with `SO_REUSEADDR` as `udpsink` sets it. On Windows a
/// later socket on the same wildcard port takes the datagrams, and the
/// receiver decoded nothing in 45 s with no error on either bus; a socket on
/// the specific address is the one Windows delivers to, which is why the
/// direct carriage test, bound to 127.0.0.1, never failed this way.
fn receiver(port: u16) -> (gst::Pipeline, Arc<AtomicU64>) {
    // Every decoded stream to a sink of its own, the picture's counted. With
    // only the picture asked for, the sound had nowhere to go and its
    // unlinked pad stopped the receiver after one frame; with the launch
    // parser's delayed linking, one of the two pads was not linked at all.
    let p = gst::Pipeline::new();
    let d = gst::ElementFactory::make("uridecodebin").property("uri", format!("rist://127.0.0.1:{port}")).build().unwrap();
    p.add(&d).unwrap();
    let frames = Arc::new(AtomicU64::new(0));
    let (f, weak) = (frames.clone(), p.downgrade());
    d.connect_pad_added(move |_, pad| {
        let Some(p) = weak.upgrade() else { return };
        let sink = gst::ElementFactory::make("fakesink").property("sync", false).build().unwrap();
        p.add(&sink).unwrap();
        sink.sync_state_with_parent().unwrap();
        let video = pad.current_caps().and_then(|c| c.structure(0).map(|s| s.name().starts_with("video/"))).unwrap_or(false);
        if video {
            let f = f.clone();
            sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                f.fetch_add(1, Ordering::Relaxed);
                gst::PadProbeReturn::Ok
            });
        }
        let _ = pad.link(&sink.static_pad("sink").unwrap());
    });
    p.set_state(gst::State::Playing).unwrap();
    (p, frames)
}

/// The errors and warnings a pipeline posted, for a failure message.
fn said(p: &gst::Pipeline) -> Vec<String> {
    let Some(bus) = p.bus() else { return Vec::new() };
    let mut out = Vec::new();
    while let Some(m) = bus.pop_filtered(&[gst::MessageType::Error, gst::MessageType::Warning]) {
        let from = m.src().map(|s| s.name().to_string()).unwrap_or_default();
        match m.view() {
            gst::MessageView::Error(e) => out.push(format!("error from {from}: {} ({:?})", e.error(), e.debug())),
            gst::MessageView::Warning(w) => out.push(format!("warning from {from}: {} ({:?})", w.error(), w.debug())),
            _ => {}
        }
    }
    out
}

#[test]
fn a_rist_receiver_decodes_the_programme_and_the_output_says_it_is_connected() {
    let _ = gst::init();
    if !["ristsink", "ristsrc", "x264enc", "avenc_aac"].iter().all(|e| crate::probe::exists(e)) {
        eprintln!("skipping: needs ristsink, ristsrc, x264enc and avenc_aac");
        return;
    }
    let port = even_port();
    let (rx, frames) = receiver(port);
    let tx = gst::parse::launch(
        "videotestsrc is-live=true ! video/x-raw,width=320,height=240,framerate=30/1 ! x264enc tune=zerolatency key-int-max=30 \
         ! h264parse ! queue name=v audiotestsrc is-live=true ! avenc_aac ! aacparse ! queue name=a",
    )
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let cfg = OutputConfig::bare("contribution", &format!("rist://127.0.0.1:{port}"));
    let mut out = new(&cfg).unwrap();
    let mut params = Params::new();
    params.insert("uri".into(), toml::Value::String(cfg.uri.clone()));
    let hello = Hello {
        instance: "contribution".into(),
        canvas: crate::caps::CanvasCaps::new(&crate::config::Canvas::default()),
        api_level: API_LEVEL,
        params: params.clone(),
        tier: Tier::Core,
    };
    out.initialize(hello).unwrap();
    let ctx = OutputCtx { id: "contribution", generation: 1, pipeline: &tx, params: &params, cfg: &cfg, taps: &[] };
    out.build(&ctx, &tx.by_name("v").unwrap(), &tx.by_name("a").unwrap()).unwrap();
    tx.set_state(gst::State::Playing).unwrap();
    // Longer by GODWINMIX_TIMING_SLACK: on a three core macOS runner with
    // the suite beside it the receiver decoded 3 frames in fifteen seconds.
    let wait = Duration::from_secs(15).mul_f64(crate::plugin::harness::timing_slack());
    let until = Instant::now() + wait;
    while (frames.load(Ordering::Relaxed) < 60 || !out.connected()) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
    let got = frames.load(Ordering::Relaxed);
    let connected = out.connected();
    let (rx_said, tx_said) = (said(&rx), said(&tx));
    let _ = tx.set_state(gst::State::Null);
    let _ = rx.set_state(gst::State::Null);
    assert!(got >= 60, "the RIST receiver decoded {got} frames in {wait:?}; wanted 60. Port {port}; the receiver said {rx_said:?}, the sender {tx_said:?}");
    assert!(connected, "the receiver answered, so the output should say it is connected");
}
