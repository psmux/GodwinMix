//! An HEVC rendition sent by `rtmp/output` to a real enhanced RTMP server:
//! ffmpeg listening, which keeps what it gets and is then asked what codec
//! it was and how many frames.

use super::*;
use crate::config::{OutputConfig, Params};
use crate::plugin::{Hello, Tier, API_LEVEL};
use godwinmix_protocol::rendition::{Fps, VideoShape};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn h264_keeps_flvmux_hevc_takes_eflvmux_and_av1_is_refused_with_the_way_out() {
    let _ = gst::init();
    assert_eq!(muxer_for_codec(VideoCodec::H264).unwrap(), "flvmux");
    if crate::probe::exists("eflvmux") {
        assert_eq!(muxer_for_codec(VideoCodec::H265).unwrap(), "eflvmux");
    }
    let av1 = muxer_for_codec(VideoCodec::Av1).unwrap_err().to_string();
    assert!(av1.contains("SRT, RIST or HLS"), "{av1}");
}

fn tool(name: &str) -> bool {
    Command::new(name).arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

#[test]
fn an_hevc_rendition_reaches_an_enhanced_rtmp_server_as_hevc() {
    let _ = gst::init();
    if !tool("ffmpeg") || !["x265enc", "eflvmux", "rtmp2sink"].iter().all(|e| crate::probe::exists(e)) {
        eprintln!("skipping: needs ffmpeg, x265enc, eflvmux and rtmp2sink");
        return;
    }
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let out = std::env::temp_dir().join(format!("gmx-hevc-rtmp-{}.flv", std::process::id()));
    let url = format!("rtmp://127.0.0.1:{port}/live/test");
    let mut server = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-listen", "1", "-i", &url, "-t", "3", "-c", "copy"])
        .arg(&out)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(700));
    let tx = gst::parse::launch(
        "videotestsrc is-live=true ! video/x-raw,width=320,height=240,framerate=30/1 ! x265enc tune=zerolatency speed-preset=ultrafast key-int-max=30 \
         ! h265parse ! queue name=v audiotestsrc is-live=true ! avenc_aac ! aacparse ! queue name=a",
    )
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let cfg = OutputConfig::bare("hevc", &url);
    let mut o = (super::super::rtmp::PROVIDE.make)(&cfg).unwrap();
    let mut params = Params::new();
    params.insert("uri".into(), toml::Value::String(url.clone()));
    let canvas = crate::caps::CanvasCaps::new(&crate::config::Canvas::default());
    o.initialize(Hello { instance: "hevc".into(), canvas, api_level: API_LEVEL, params: params.clone(), tier: Tier::Core }).unwrap();
    let shape = VideoShape { codec: VideoCodec::H265, width: 320, height: 240, fps: Fps::whole(30), bitrate_kbps: 1000, keyframe_ms: 1000 };
    let tap = crate::render::Tap { request: "hevc".into(), rung: 0, video: Some(shape), audio: None, keyframe_ms: 1000, video_tee: None, audio_tee: None, program: tx.clone() };
    let taps = [tap];
    let ctx = OutputCtx { id: "hevc", generation: 1, pipeline: &tx, params: &params, cfg: &cfg, taps: &taps };
    o.build(&ctx, &tx.by_name("v").unwrap(), &tx.by_name("a").unwrap()).unwrap();
    tx.set_state(gst::State::Playing).unwrap();
    let until = std::time::Instant::now() + Duration::from_secs(15);
    while server.try_wait().ok().flatten().is_none() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = server.kill();
    let _ = tx.set_state(gst::State::Null);
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v", "-count_frames", "-show_entries", "stream=codec_name,nb_read_frames", "-of", "csv=p=0"])
        .arg(&out)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&out);
    let answer = String::from_utf8_lossy(&probe.stdout).trim().to_string();
    let (codec, frames) = answer.split_once(',').unwrap_or(("", "0"));
    assert_eq!(codec, "hevc", "the server got: {answer}");
    let frames: u32 = frames.parse().unwrap_or(0);
    assert!(frames >= 60, "the server kept {frames} HEVC frames of 3 s");
}
