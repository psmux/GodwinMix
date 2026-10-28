//! An interlaced, anamorphic MPEG-2 programme, as a headend's SD channels
//! are: 704x480 stored, shown at 16:9, two fields a frame. A show's HLS
//! output asks for 640x360 with the picture laid in by its shape and the
//! fields deinterlaced where the stream marks them, and a ladder rung under
//! it. It goes live with both pictures in its master, and the first is the
//! frame asked for, not a narrower one between bars.

use super::direct_live::{free_udp, staged_ingest};
use super::hls_direct::{folder_with_relay, until};
use super::support::*;
use gstreamer::prelude::*;
use serde_json::json;
use std::time::Duration;

/// Interlaced 16:9 MPEG-2 at 704x480 and AC-3 sound in MPEG-TS to `port`.
fn sender(port: u16) -> gstreamer::Pipeline {
    gstreamer::init().unwrap();
    let text = format!(
        "mpegtsmux name=mux alignment=7 ! udpsink host=127.0.0.1 port={port} sync=false \
         videotestsrc is-live=true pattern=ball ! video/x-raw,format=I420,width=704,height=480,framerate=60000/1001,pixel-aspect-ratio=40/33 \
         ! interlace field-pattern=1:1 top-field-first=true ! avenc_mpeg2video gop-size=15 bitrate=2000000 ! mpegvideoparse ! mux. \
         audiotestsrc is-live=true ! audioconvert ! audioresample ! audio/x-raw,rate=48000,channels=2 ! avenc_ac3 ! ac3parse ! mux."
    );
    let p = gstreamer::parse::launch(&text).expect("interlace, avenc_mpeg2video, avenc_ac3 and mpegtsmux are installed").downcast::<gstreamer::Pipeline>().unwrap();
    p.set_state(gstreamer::State::Playing).unwrap();
    p
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_interlaced_anamorphic_programme_comes_out_deinterlaced_at_its_shape_with_a_ladder() {
    for need in ["cmafmux", "avenc_mpeg2video", "interlace", "deinterlace"] {
        if !godwinmix_core::probe::have_or_skip(need) {
            return;
        }
    }
    let (dir, port) = folder_with_relay("hls-picture");
    let config = dir.join("godwinmix.toml");
    let written = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, format!("{written}\n[governor]\nreserve_cores = 0\n")).unwrap();
    let source = staged_ingest(&dir);
    let st = start_alone(dir.clone(), port, &[]).await;
    let mut ws = rpc(&st, "").await;
    let _ = call(&mut ws, 1, "channel.remove", json!({"id": "live"})).await;
    let input = free_udp();
    let tx = sender(input);
    let picture = json!({"codec": "h264", "width": 640, "height": 360, "fit": "pad", "deinterlace": "auto"});
    let ladder = json!({"ladder": [
        {"video": picture, "audio": {"codec": "aac"}},
        {"video": {"codec": "h264", "width": 320, "height": 180, "fit": "pad", "deinterlace": "auto"}, "audio": {"codec": "aac"}}
    ]});
    let added = call(&mut ws, 2, "show.add", json!({"name": "Feed", "compositing": false, "input": {"uri": format!("udp://127.0.0.1:{input}")},
        "outputs": [{"uri": "hls://viewers", "params": {"segment_ms": 1000, "window": 6}, "rendition": ladder}]})).await;
    assert!(added.get("error").is_none(), "{added}");
    let asked = call(&mut ws, 3, "plugin.add", json!({"source": source.to_string_lossy()})).await;
    assert!(asked.get("error").is_none(), "{asked}");

    let out = until(&mut ws, "feed", "viewers", "live", Duration::from_secs(120)).await;
    let master = out["playback"]["master_url_path"].as_str().unwrap().to_string();
    // The second rung joins the master once it has a segment of its own.
    let mut text = String::new();
    for _ in 0..40 {
        text = super::hls_direct::text(&st, &master).await.1;
        if text.matches("#EXT-X-STREAM-INF").count() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(text.contains("RESOLUTION=640x360"), "the first picture is the frame asked for: {text}");
    assert!(text.contains("RESOLUTION=320x180"), "the ladder's second picture is in the master: {text}");
    tx.set_state(gstreamer::State::Null).unwrap();
}
