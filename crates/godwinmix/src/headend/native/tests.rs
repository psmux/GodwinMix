//! A channel set up here and a stream imported from Flussonic make the same
//! plan, so the same show; the export reads back as the same plans; and a
//! channel that cannot run is refused with what to send instead.

use super::compile::{all_profiles, channel_of, plan};
use super::export;
use super::store::Store;
use super::{check, compile};
use godwinmix_flussonic::import::import;
use godwinmix_flussonic::Document;
use godwinmix_protocol::headend::profile::builtins;
use godwinmix_protocol::headend::{ChannelHls, ChannelInput, ChannelOutput, HeadendChannel, TranscodeProfile};
use godwinmix_protocol::rendition::{AudioCodec, AudioWant, Deinterlace, Fit, Tracks, VideoCodec, VideoWant};
use serde_json::json;

const SAMPLE: &str = include_str!("../../../../godwinmix-flussonic/tests/fixtures/sample-headend.conf");

fn channel(name: &str) -> HeadendChannel {
    HeadendChannel {
        name: name.into(),
        title: None,
        enabled: true,
        input: ChannelInput { uri: "udp://@239.0.0.1:5000".into(), program: Some(2), interface: Some("eth1".into()), ..ChannelInput::default() },
        profile: "copy".into(),
        custom: None,
        hls: ChannelHls::default(),
        outputs: Vec::new(),
        origin: None,
    }
}

/// The transcoder line of the sample, as someone would set it up by hand.
fn by_hand() -> TranscodeProfile {
    TranscodeProfile {
        video: Some(VideoWant {
            codec: Some(VideoCodec::H264),
            width: Some(1280),
            height: Some(720),
            bitrate_kbps: Some(1500),
            keyframe_frames: Some(50),
            fit: Some(Fit::Pad),
            pad_color: Some("#000000".into()),
            deinterlace: Some(Deinterlace::Off),
            preset: Some("p1".into()),
            ..VideoWant::default()
        }),
        audio: Some(AudioWant { codec: Some(AudioCodec::Aac), bitrate_kbps: Some(128), tracks: Some(Tracks::All), ..AudioWant::default() }),
        ..TranscodeProfile::default()
    }
}

#[test]
fn a_channel_set_up_by_hand_is_the_plan_the_import_makes() {
    let imported = import(&Document::parse(SAMPLE).unwrap().config).shows;
    let ch117 = imported.iter().find(|p| p.name == "CH117").unwrap();
    let mut c = channel("CH117");
    c.input = ChannelInput { uri: "udp://@239.10.0.1:2001".into(), program: Some(7), interface: Some("eth1".into()), ..ChannelInput::default() };
    c.profile = "custom".into();
    c.custom = Some(by_hand());
    assert_eq!(&plan(&c, &all_profiles(&[])).unwrap(), ch117);
    let mut relay = channel("RELAY1");
    relay.input = ChannelInput { uri: "rtmp://127.0.0.1:1935/live/relay1".into(), ..ChannelInput::default() };
    assert_eq!(&plan(&relay, &all_profiles(&[])).unwrap(), imported.iter().find(|p| p.name == "RELAY1").unwrap());
}

#[test]
fn every_imported_stream_comes_back_as_the_same_plan() {
    let profiles = all_profiles(&[]);
    for p in import(&Document::parse(SAMPLE).unwrap().config).shows {
        let c = channel_of(&p, &profiles, "import of a test");
        assert_eq!(plan(&c, &profiles).unwrap(), p, "{}", p.name);
    }
    let text = "stream A {\n  title The A channel;\n  disabled true;\n  segment_duration 4;\n  segment_count 5;\n  input rtmp://h/app/a;\n  input udp://239.0.0.1:1;\n  push srt://192.0.2.9:9000;\n  transcoder vb=copy ab=96k;\n}\n";
    let p = import(&Document::parse(text).unwrap().config).shows.remove(0);
    let c = channel_of(&p, &profiles, "x");
    assert_eq!((c.enabled, c.input.backup.as_ref().map(|b| b.uri.as_str()), c.hls.window), (false, Some("udp://@239.0.0.1:1"), Some(20)));
    assert_eq!(plan(&c, &profiles).unwrap(), p);
}

#[test]
fn a_rendition_that_is_a_profile_is_named_by_it() {
    let profiles = all_profiles(&[]);
    let mut c = channel("NEWS");
    c.profile = "h264-720p-1500".into();
    let p = plan(&c, &profiles).unwrap();
    assert!(!p.copy && p.outputs[0]["rendition"]["video"]["width"] == 1280);
    let back = channel_of(&p, &profiles, "x");
    assert_eq!((back.profile.as_str(), back.custom.is_none()), ("h264-720p-1500", true));
}

#[test]
fn outputs_take_the_conversion_unless_they_copy() {
    let mut c = channel("SPORT");
    c.profile = "h264-480p-800".into();
    c.outputs = vec![ChannelOutput { id: String::new(), uri: "udp://239.0.1.1:5000".into(), copy: false }, ChannelOutput { id: "srt-out".into(), uri: "srt://192.0.2.30:9000".into(), copy: true }];
    let p = plan(&c, &all_profiles(&[])).unwrap();
    assert_eq!(p.outputs[1]["id"], "push-1");
    assert!(p.outputs[1].get("rendition").is_some() && p.outputs[2].get("rendition").is_none());
    assert_eq!(p.input, json!({"uri": "udp://@239.0.0.1:5000", "program": 2, "params": {"interface": "eth1"}}));
}

#[test]
fn what_cannot_run_is_refused_with_what_to_send() {
    let profiles = all_profiles(&[]);
    let mut bad = channel("has space");
    assert!(check::channel(&bad, &profiles).unwrap_err().contains("CH117"));
    bad.name = "ok".into();
    bad.input.uri = "239.0.0.1:5000".into();
    assert!(check::channel(&bad, &profiles).unwrap_err().contains("udp://@239.0.0.1:5000"));
    bad.input.uri = "udp://@239.0.0.1:5000".into();
    bad.profile = "nope".into();
    assert!(compile::profile_of(&bad, &profiles).unwrap_err().contains("copy"));
    bad.profile = "custom".into();
    assert!(check::channel(&bad, &profiles).is_err(), "custom with no settings");
    assert!(check::profile(&TranscodeProfile { id: "copy".into(), name: "x".into(), ..TranscodeProfile::default() }, &builtins()).is_err());
    assert!(check::profile(&TranscodeProfile { id: "sd-news".into(), name: "SD news".into(), ..by_hand() }, &builtins()).is_ok());
}

#[test]
fn the_export_reads_back_as_the_same_plans() {
    let profiles = all_profiles(&[]);
    let mut s = Store::default();
    let mut a = channel("CH117");
    a.input = ChannelInput { uri: "udp://@239.10.0.1:2001".into(), program: Some(7), interface: Some("eth1".into()), ..ChannelInput::default() };
    a.profile = "custom".into();
    a.custom = Some(by_hand());
    a.title = Some("News".into());
    let mut b = channel("RELAY1");
    b.input = ChannelInput { uri: "rtmp://127.0.0.1:1935/live/relay1".into(), ..ChannelInput::default() };
    b.enabled = false;
    s.channels = vec![a.clone(), b.clone()];
    let out = export::text(&s);
    let back = import(&Document::parse(&out.text).expect(&out.text).config).shows;
    let media = |p: &godwinmix_flussonic::import::ShowPlan| (p.input.clone(), p.outputs[0]["rendition"].clone(), p.disabled, p.title.clone());
    assert_eq!(media(&back[0]), media(&plan(&a, &profiles).unwrap()), "{}", out.text);
    assert_eq!(media(&back[1]), media(&plan(&b, &profiles).unwrap()), "{}", out.text);
}

#[test]
fn a_scan_takes_the_picture_from_the_numbers_when_the_event_has_only_sound() {
    use godwinmix_protocol::headend::ScannedProgram;
    let mut p = ScannedProgram { number: 2, streams: vec!["h264".into(), "aac".into()], ..ScannedProgram::default() };
    let stats = json!({"video_codec": "h264", "width": 640, "height": 360, "fps": 25.0, "audio_codec": "aac", "audio_channels": 1, "kbps": 900});
    let event = json!({"tracks": [{"track": 0, "kind": "audio", "codec": "aac", "channels": 1, "lang": "eng"}]});
    super::scan_read::fill(&mut p, Some(&stats), Some(&event));
    let kinds: Vec<(&str, Option<u32>)> = p.tracks.iter().map(|t| (t.kind.as_str(), t.width)).collect();
    assert_eq!(kinds, [("video", Some(640)), ("audio", None)]);
    assert_eq!(p.tracks[1].lang.as_deref(), Some("eng"), "the event's own track is kept");
    assert!(super::scan_read::heard(&p, Some(&stats)));
}
