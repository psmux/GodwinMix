//! An input whose sound comes seconds before its first picture, as an
//! MPEG-2 programme of a multiplex does when the show starts between two of
//! its keyframes: the host's first word on it is sound and no picture. The
//! show's HLS output asks for a rendition, which cannot be planned until the
//! picture is known. Once it is, the output goes live, and the show's health
//! says so; while it is not, the health does not read `ok`.

use super::direct_live::{free_udp, staged_ingest};
use super::hls_direct::{folder_with_relay, until};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::json;
use std::time::{Duration, Instant};

/// MPEG-2 video and AC-3 sound in MPEG-TS to `port`, the picture held back
/// by a closed valve until the caller opens it.
fn sender(port: u16) -> gstreamer::Pipeline {
    gstreamer::init().unwrap();
    let text = format!(
        "mpegtsmux name=mux alignment=7 ! udpsink host=127.0.0.1 port={port} sync=false \
         videotestsrc is-live=true horizontal-speed=4 ! video/x-raw,format=I420,width=320,height=180,framerate=30/1 ! valve name=picture drop=true \
         ! avenc_mpeg2video gop-size=30 bitrate=800000 ! mpegvideoparse ! mux. \
         audiotestsrc is-live=true ! audioconvert ! audioresample ! audio/x-raw,rate=48000,channels=2 ! avenc_ac3 ! ac3parse ! mux."
    );
    let p = gstreamer::parse::launch(&text).expect("avenc_mpeg2video, avenc_ac3 and mpegtsmux are installed").downcast::<gstreamer::Pipeline>().unwrap();
    p.set_state(gstreamer::State::Playing).unwrap();
    p
}

async fn show(ws: &mut Ws, id: u64) -> serde_json::Value {
    call(ws, id, "show.stats", json!({"ids": ["feed"]})).await["result"]["shows"][0].clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rendition_planned_once_the_picture_comes_after_the_sound_goes_live() {
    if !godwinmix_core::probe::have_or_skip("cmafmux") || !godwinmix_core::probe::have_or_skip("avenc_mpeg2video") {
        return;
    }
    let (dir, port) = folder_with_relay("hls-late-picture");
    let config = dir.join("godwinmix.toml");
    let written = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{written}\n[governor]\nreserve_cores = 0\n")).unwrap();
    let source = staged_ingest(&dir);
    let st = start_alone(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let _ = call(&mut ws, 1, "channel.remove", json!({"id": "live"})).await;
    let input = free_udp();
    let tx = sender(input);
    let added = call(&mut ws, 2, "show.add", json!({"name": "Feed", "compositing": false, "input": {"uri": format!("udp://127.0.0.1:{input}")},
        "outputs": [{"uri": "hls://viewers", "params": {"segment_ms": 1000, "window": 6},
                     "rendition": {"video": {"codec": "h264", "height": 120}, "audio": {"codec": "aac"}}}]})).await;
    assert!(added.get("error").is_none(), "{added}");
    let asked = call(&mut ws, 3, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");

    // Sound alone, long enough for the host to say so and the plan to be
    // refused for want of a picture.
    let heard = Instant::now() + Duration::from_secs(60);
    loop {
        let s = show(&mut ws, 4).await;
        let kbps = s["input"]["kbps"].as_u64().unwrap_or(0);
        if kbps > 0 || Instant::now() > heard {
            assert!(kbps > 0, "the sound alone never reached the show: {s}");
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    tokio::time::sleep(Duration::from_secs(6)).await;
    let s = show(&mut ws, 5).await;
    assert_ne!(s["health"]["state"], "ok", "an output with nothing to send is not ok: {s}");

    tx.by_name("picture").unwrap().set_property("drop", false);
    let out = until(&mut ws, "feed", "viewers", "live", Duration::from_secs(120)).await;
    let master = out["playback"]["master_url_path"].as_str().unwrap().to_string();
    let (_, text) = super::hls_direct::text(&st, &master).await;
    assert!(text.contains("avc1.") && text.contains("mp4a.40"), "{text}");
    tx.set_state(gstreamer::State::Null).unwrap();
}
