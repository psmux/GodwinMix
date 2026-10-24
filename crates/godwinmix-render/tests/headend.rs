//! A headend's conversion: an interlaced MPEG-2 broadcast with three AC-3
//! tracks into H.264 720p with bars and every track as AAC.

mod common;

use common::*;
use godwinmix_protocol::rendition::{Deinterlace, Fit, SoundTrack, Tracks};
use godwinmix_render::*;

fn ac3(channels: u8) -> AudioShape {
    AudioShape { codec: AudioCodec::Ac3, channels, sample_rate: 48_000, bitrate_kbps: 384 }
}

fn broadcast() -> StreamInfo {
    let video = VideoShape {
        codec: VideoCodec::Mpeg2,
        width: 1920,
        height: 1080,
        fps: Fps { num: 30000, den: 1001 },
        bitrate_kbps: 12_000,
        keyframe_ms: 0,
    };
    let tracks = vec![
        SoundTrack { track: 1, shape: ac3(2), lang: Some("spa".into()) },
        SoundTrack { track: 2, shape: ac3(6), lang: None },
    ];
    StreamInfo { video: Some(video), audio: Some(ac3(6)), encoded: true, tracks, interlaced: true }
}

fn channel(fit: Option<Fit>, tracks: Option<Tracks>) -> RenditionRequest {
    let mut r = with_video(
        request("ch", Container::Hls),
        VideoWant {
            codec: Some(VideoCodec::H264),
            width: Some(1280),
            height: Some(720),
            bitrate_kbps: Some(1500),
            keyframe_frames: Some(50),
            fit,
            deinterlace: Some(Deinterlace::Off),
            ..VideoWant::default()
        },
    );
    r.audio = Some(AudioWant { codec: Some(AudioCodec::Aac), bitrate_kbps: Some(128), tracks, ..AudioWant::default() });
    r
}

fn gpu() -> StaticCostModel {
    StaticCostModel::software().with_hardware("h264-nvidia", VideoCodec::H264, "nvidia")
}

#[test]
fn every_sound_track_gets_its_own_decode_and_aac_at_its_own_channel_count() {
    let all = plan(&sources(&[("main", broadcast())]), &on("main", vec![channel(Some(Fit::Pad), Some(Tracks::All))]), &gpu()).unwrap();
    let ids: Vec<&str> = all.nodes.iter().map(|n| n.id.as_str()).collect();
    for id in [
        "decode:main:video",
        "scale:main:1280x720p30000/1001",
        "encode:main:h264:1280x720p30000/1001:1500k:g1668",
        "decode:main:audio",
        "aencode:main:aac:6ch48000:128k",
        "decode:main:audio1",
        "aencode:main:a1:aac:2ch48000:128k",
        "decode:main:audio2",
        "aencode:main:a2:aac:6ch48000:128k",
    ] {
        assert!(ids.contains(&id), "{id} missing from {ids:?}");
    }
    let mux = all.output("ch").unwrap();
    assert_eq!(mux.inputs.len(), 4, "{:?}", mux.inputs);
    assert!(all.encodes().all(|n| encoder_of(n).hardware));
    let one = plan(&sources(&[("main", broadcast())]), &on("main", vec![channel(None, None)]), &gpu()).unwrap();
    assert_eq!(one.output("ch").unwrap().inputs.len(), 2, "the first track alone unless all are asked for");
}

#[test]
fn a_fit_other_than_black_bars_is_other_work_with_an_id_of_its_own() {
    let crop = plan(&sources(&[("main", broadcast())]), &on("main", vec![channel(Some(Fit::Crop), None)]), &gpu()).unwrap();
    assert!(crop.node("scale:main:1280x720p30000/1001:crop").is_some());
    let mut on_di = channel(None, None);
    on_di.video.as_mut().unwrap().deinterlace = Some(Deinterlace::On);
    let di = plan(&sources(&[("main", broadcast())]), &on("main", vec![on_di]), &gpu()).unwrap();
    assert!(di.node("scale:main:1280x720p30000/1001:di").is_some());
}
