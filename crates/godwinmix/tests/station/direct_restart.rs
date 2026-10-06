//! A direct show whose sender restarts, through the station: a UDP MPEG-TS
//! feed stopped and a new one started on the same port, first with new PIDs
//! and a new program number, then with the same layout again. The show's
//! UDP copy output and a UDP output with a smaller rendition both keep
//! receiving within a few seconds of each, and its health comes back to
//! `ok`. On a machine so busy that the governor refuses the rendition (the
//! whole suite at once can be), the rendition is left out and the test says
//! so; the copy is checked either way.

use super::direct_live::{free_udp, received, staged_ingest};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::json;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

/// A moving picture and a tone in MPEG-TS, the video on `vpid` and the
/// sound on `apid` in program `program`, sent live to `port`. The picture
/// moves and the tone sounds, so the show's own alarms stay quiet.
fn sender(port: u16, vpid: u16, apid: u16, program: u16) -> gstreamer::Pipeline {
    gstreamer::init().unwrap();
    let text = format!(
        "mpegtsmux name=mux alignment=7 prog-map=program_map,sink_{vpid}={program},sink_{apid}={program} ! udpsink host=127.0.0.1 port={port} sync=false \
         videotestsrc is-live=true horizontal-speed=4 ! video/x-raw,format=I420,width=320,height=180,framerate=30/1 ! x264enc tune=zerolatency key-int-max=30 bitrate=600 ! h264parse ! mux.sink_{vpid} \
         audiotestsrc is-live=true ! audioconvert ! audioresample ! audio/x-raw,rate=48000 ! avenc_aac ! aacparse ! mux.sink_{apid}"
    );
    let p = gstreamer::parse::launch(&text).expect("x264enc, avenc_aac and mpegtsmux are installed").downcast::<gstreamer::Pipeline>().unwrap();
    p.set_state(gstreamer::State::Playing).unwrap();
    p
}

async fn health(ws: &mut Ws, id: u64) -> serde_json::Value {
    let s = call(ws, id, "show.stats", json!({"ids": ["feed"]})).await;
    s["result"]["shows"][0].clone()
}

/// The governor turned the rendition away, and that is all that is wrong.
fn refused(show: &serde_json::Value) -> bool {
    let alarms = show["health"]["alarms"].as_array().cloned().unwrap_or_default();
    !alarms.is_empty() && alarms.iter().all(|a| a["kind"] == "governor-refused")
}

/// Wait for the show's health to read `ok`, up to `secs`, or with
/// `transcoded` false, to hold nothing but the governor's refusal.
async fn comes_back(ws: &mut Ws, what: &str, secs: u64, transcoded: bool) {
    let until = Instant::now() + Duration::from_secs(secs);
    loop {
        let show = health(ws, 50).await;
        if show["health"]["state"] == "ok" || (!transcoded && refused(&show)) {
            return;
        }
        assert!(Instant::now() < until, "{what}: the show's health is not back to ok after {secs} s: {show}");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_direct_show_follows_a_sender_restarted_with_new_pids_or_the_same_layout() {
    let (dir, port) = folder("direct-restart");
    let source = staged_ingest(&dir);
    let st = start(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let (input, out, small) = (free_udp(), UdpSocket::bind("127.0.0.1:0").unwrap(), UdpSocket::bind("127.0.0.1:0").unwrap());
    let (to, to_small) = (out.local_addr().unwrap().port(), small.local_addr().unwrap().port());
    let mut tx = sender(input, 65, 66, 1);
    let outputs = json!([{"id": "out", "uri": format!("udp://127.0.0.1:{to}")},
        {"id": "small", "uri": format!("udp://127.0.0.1:{to_small}"), "rendition": {"video": {"height": 120}}}]);
    let added = call(&mut ws, 1, "show.add", json!({"name": "Feed", "compositing": false,
        "input": {"uri": format!("udp://127.0.0.1:{input}")}, "outputs": outputs})).await;
    assert!(added.get("error").is_none(), "{added}");
    let asked = call(&mut ws, 2, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");
    assert!(received(&out, Duration::from_secs(60), 100_000) >= 100_000, "the first sender never reached the output; see {}", dir.join("log.jsonl").display());
    let mut transcoded = received(&small, Duration::from_secs(60), 20_000) >= 20_000;
    let show = health(&mut ws, 3).await;
    if !transcoded {
        assert!(refused(&show), "the first sender was never transcoded: {show}");
        eprintln!("skipping the rendition: the governor refused it on this machine as loaded now: {show}");
    }
    comes_back(&mut ws, "the first sender", 30, transcoded).await;
    // Give the measured rate time to settle the plan, then keep it.
    tokio::time::sleep(Duration::from_secs(4)).await;
    let planned = health(&mut ws, 4).await["outputs"][1]["rendition_text"].clone();

    for (what, vpid, apid, program) in [("new PIDs and a new program", 300, 301, 7), ("the same layout again", 300, 301, 7)] {
        tx.set_state(gstreamer::State::Null).unwrap();
        // Long enough for the show to see the input go quiet, and for what
        // the old sender left in flight to drain.
        received(&out, Duration::from_secs(2), usize::MAX);
        received(&small, Duration::from_millis(200), usize::MAX);
        tx = sender(input, vpid, apid, program);
        let started = Instant::now();
        let got = received(&out, Duration::from_secs(10), 50_000);
        assert!(got >= 50_000, "{what}: only {got} bytes reached the output in 10 s: {}", health(&mut ws, 60).await);
        eprintln!("{what}: the copy had 50 kB again {:?} after the new sender started", started.elapsed());
        if transcoded {
            let got = received(&small, Duration::from_secs(10), 20_000);
            // As at the start: on a loaded macOS runner the governor turned
            // the rendition away when the new sender came ("0.0 cores is
            // free"), which is the governor doing its job. Anything else is
            // still a failure.
            let show = health(&mut ws, 62).await;
            if got < 20_000 && refused(&show) {
                eprintln!("{what}: the governor refused the rendition on this machine as loaded now: {show}");
                transcoded = false;
            } else {
                assert!(got >= 20_000, "{what}: only {got} bytes of the rendition in 10 s: {show}");
                eprintln!("{what}: the rendition had 20 kB again {:?} after the new sender started", started.elapsed());
            }
        }
        comes_back(&mut ws, what, 20, transcoded).await;
        let show = health(&mut ws, 61).await;
        assert_eq!(show["outputs"][0]["state"], "live", "{what}: {show}");
        if transcoded {
            assert_eq!(show["outputs"][1]["state"], "live", "{what}: {show}");
            assert_eq!(show["outputs"][1]["rendition_text"], planned, "{what}: the rendition was planned again: {show}");
        }
    }
    tx.set_state(gstreamer::State::Null).unwrap();
}
