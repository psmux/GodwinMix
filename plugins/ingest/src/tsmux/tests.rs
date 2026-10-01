//! The muxer against real decoders: x264 and AAC in, GStreamer's own
//! `tsdemux` and libav out.

use super::*;
use crate::testfeed;

fn mux(tags: &[MediaTag]) -> Vec<u8> {
    let mut m = Muxer::new();
    let mut out = Vec::new();
    for t in tags {
        m.tag(t, t.timestamp_ms, &mut out);
    }
    out
}

#[test]
fn ninety_frames_of_h264_and_aac_decode_out_of_the_muxed_stream() {
    let tags = testfeed::tags(90);
    let ts = mux(&tags);
    assert_eq!(ts.len() % PACKET, 0);
    let path = std::env::temp_dir().join(format!("gmx-tsmux-{}.ts", std::process::id()));
    std::fs::write(&path, &ts).unwrap();
    let (pictures, sound) = testfeed::decode_ts(&path);
    let _ = std::fs::remove_file(&path);
    assert!(pictures >= 88, "decoded {pictures} of 90 pictures");
    assert!(sound >= 80, "decoded {sound} sound frames of about 140");
}

#[test]
fn the_tables_come_before_every_keyframe_and_the_clock_rides_on_video() {
    let tags = testfeed::tags(60);
    let ts = mux(&tags);
    let packets: Vec<&[u8]> = ts.chunks(PACKET).collect();
    let pid = |p: &[u8]| (u16::from(p[1] & 0x1f) << 8) | u16::from(p[2]);
    assert_eq!(pid(packets[0]), 0, "a receiver joining at the start finds the PAT first");
    let mut keyframes = 0;
    for (i, p) in packets.iter().enumerate() {
        let random_access = p[3] & 0x20 != 0 && p[4] > 0 && p[5] & 0x40 != 0;
        if pid(p) == psi::VIDEO_PID && random_access {
            keyframes += 1;
            assert_eq!(pid(packets[i - 1]), psi::PMT_PID, "packet {i}: a keyframe follows its PMT");
            assert!(p[5] & 0x10 != 0, "and carries a PCR");
        }
    }
    assert_eq!(keyframes, 2, "60 frames at a keyframe every 30");
}

/// Three seconds of sound through `encoder ! parser`, each frame an enhanced
/// RTMP v2 body with `cc`, as the inputs put AC-3 and MPEG audio on the hub.
fn enhanced_sound(encoder: &str, parser: &str, cc: &[u8; 4]) -> Option<Vec<MediaTag>> {
    use gstreamer as gst;
    use gstreamer::prelude::*;
    gmx_netkit::init().unwrap();
    gst::ElementFactory::find(encoder)?;
    let line = format!(
        "audiotestsrc num-buffers=140 samplesperbuffer=1024 ! audio/x-raw,rate=48000,channels=2 ! {encoder} ! {parser} ! appsink name=out sync=false"
    );
    let p = gst::parse::launch(&line).unwrap().downcast::<gst::Pipeline>().unwrap();
    let sink = p.by_name("out").unwrap().downcast::<gstreamer_app::AppSink>().unwrap();
    p.set_state(gst::State::Playing).unwrap();
    let mut tags = Vec::new();
    while let Ok(sample) = sink.pull_sample() {
        let buffer = sample.buffer().unwrap();
        let ms = buffer.pts().map_or(0, |t| t.mseconds() as u32);
        let map = buffer.map_readable().unwrap();
        let payload = [&crate::exaudio::prefix(cc)[..], map.as_slice()].concat();
        tags.push(MediaTag { kind: TagKind::Audio, timestamp_ms: ms, keyframe: false, sequence_header: false, payload: payload.into() });
    }
    let _ = p.set_state(gst::State::Null);
    Some(tags)
}

#[test]
fn ac3_and_mpeg_audio_cross_as_they_came_and_decode() {
    let kinds = [("avenc_ac3", "ac3parse", crate::exaudio::AC3, "ac3"), ("avenc_mp2", "mpegaudioparse", crate::exaudio::MPEG, "mp2")];
    for (encoder, parser, cc, name) in kinds {
        let Some(tags) = enhanced_sound(encoder, parser, cc) else {
            eprintln!("skipping {name}: no {encoder}");
            continue;
        };
        assert!(tags.len() > 30, "{name}: {} frames encoded", tags.len());
        let path = std::env::temp_dir().join(format!("gmx-tsmux-{name}-{}.ts", std::process::id()));
        std::fs::write(&path, mux(&tags)).unwrap();
        let (_, sound) = testfeed::decode_ts(&path);
        let _ = std::fs::remove_file(&path);
        assert!(sound as usize + 3 >= tags.len(), "{name}: decoded {sound} of {} frames", tags.len());
    }
}

#[test]
fn a_frame_before_its_sequence_header_is_dropped_not_sent_broken() {
    let tags = testfeed::tags(30);
    let frames: Vec<MediaTag> = tags.into_iter().filter(|t| !t.sequence_header).collect();
    assert!(mux(&frames).is_empty());
}
