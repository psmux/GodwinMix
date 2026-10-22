use super::*;
use crate::facade::access::Segment;

fn track(kind: Kind, number: u8, lang: Option<&str>) -> Track {
    let (codecs, bandwidth) = match kind {
        Kind::Video => ("avc1.64001f", 1_500_000),
        Kind::Audio => ("mp4a.40.2", 128_000),
    };
    Track { kind, number, codecs: codecs.into(), bandwidth, width: 1280, height: 720, fps: Some(29.97), channels: Some(2), lang: lang.map(String::from), name: None }
}

fn three(muxed: bool) -> Tracks {
    Tracks { tracks: vec![track(Kind::Video, 1, None), track(Kind::Audio, 1, Some("eng")), track(Kind::Audio, 2, Some("spa"))], muxed }
}

#[test]
fn every_sound_is_a_rendition_and_the_first_rides_in_a_muxed_variant() {
    let m = master(&three(true));
    assert!(m.contains("#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"ENG\",LANGUAGE=\"eng\",DEFAULT=YES,AUTOSELECT=YES,CHANNELS=\"2\"\n"), "{m}");
    assert!(m.contains("NAME=\"SPA\",LANGUAGE=\"spa\",DEFAULT=NO,AUTOSELECT=YES,CHANNELS=\"2\",URI=\"tracks-a2/mono.m3u8\""), "{m}");
    assert!(m.contains("BANDWIDTH=1628000,CODECS=\"avc1.64001f,mp4a.40.2\",RESOLUTION=1280x720,FRAME-RATE=29.970,AUDIO=\"audio\"\ntracks-v1a1/mono.m3u8\n"), "{m}");
    // CMAF cannot mux: every sound has its own playlist, the variant is video alone.
    let m = master(&three(false));
    assert!(m.contains("URI=\"tracks-a1/mono.m3u8\"") && m.contains("\ntracks-v1/mono.m3u8\n"), "{m}");
    assert_eq!(m.matches("EXT-X-MEDIA:").count(), 2);
}

#[test]
fn a_feed_with_one_kind_has_no_group() {
    let sound_only = Tracks { tracks: vec![track(Kind::Audio, 1, None)], muxed: true };
    let m = master(&sound_only);
    assert!(!m.contains("EXT-X-MEDIA") && m.ends_with("tracks-a1/mono.m3u8\n"), "{m}");
    let same = Tracks { tracks: vec![track(Kind::Video, 1, None), track(Kind::Audio, 1, None), track(Kind::Audio, 2, None)], muxed: false };
    let m = master(&same);
    assert!(m.contains("NAME=\"Sound 1\"") && m.contains("NAME=\"Sound 2\""), "{m}");
}

#[test]
fn a_media_playlist_names_segments_by_their_utc_start() {
    let start = 1_792_756_804_000;
    let seg = |i: i64, init: Option<&str>| Segment { start_ms: start + i * 6006, duration_ms: 6006, init: init.map(String::from), discontinuity: false };
    let w = Window { container: Container::Ts, sequence: 41, segments: vec![seg(0, None), seg(1, None)] };
    let text = media(&w, "tracks-v1a1/");
    let want = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:7\n#EXT-X-MEDIA-SEQUENCE:41\n#EXT-X-INDEPENDENT-SEGMENTS\n\
                #EXT-X-PROGRAM-DATE-TIME:2026-10-23T12:00:04.000Z\n#EXTINF:6.006,\ntracks-v1a1/2026/10/23/12/00/04-06006.ts\n\
                #EXT-X-PROGRAM-DATE-TIME:2026-10-23T12:00:10.006Z\n#EXTINF:6.006,\ntracks-v1a1/2026/10/23/12/00/10-06006.ts\n";
    assert_eq!(text, want);
    let w = Window { container: Container::Cmaf, sequence: 0, segments: vec![seg(0, Some("a1")), seg(1, Some("a1")), seg(2, Some("b2"))] };
    let text = media(&w, "");
    assert_eq!(text.matches("#EXT-X-MAP:URI=\"init-a1.mp4\"").count(), 1, "{text}");
    assert!(text.contains("#EXT-X-MAP:URI=\"init-b2.mp4\"") && text.contains("04-06006.m4s"), "{text}");
}

#[test]
fn two_segments_in_one_second_never_share_a_name() {
    let s = |ms: i64| Segment { start_ms: ms, duration_ms: 400, init: None, discontinuity: false };
    let w = Window { container: Container::Ts, sequence: 0, segments: vec![s(1_000), s(1_400), s(2_000)] };
    let text = media(&w, "");
    assert_eq!(text.matches(".ts").count(), 2, "{text}");
}
