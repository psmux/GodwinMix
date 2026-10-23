//! The TS playlists from a listing, and Flussonic's names read back.

use super::*;
use crate::hls::track::TrackInfo;

fn entry(msn: u64, disc: u32, start_ms: i64) -> Entry {
    Entry { msn, disc, start_ms, duration_ms: 2000, bytes: 100 }
}

#[test]
fn a_clock_name_is_the_utc_second_and_the_length_and_reads_back() {
    // 2026-10-23T08:04:12.345Z
    let e = Entry { duration_ms: 2002, ..entry(0, 0, 1_792_742_652_345) };
    assert_eq!(clock_name(&e), "2026/10/23/08/04/12-2002.ts");
    assert_eq!(parse_clock("2026/10/23/08/04/12-2002.ts"), Some((1_792_742_652, 2002)));
    for bad in ["2026/10/23/08/04-2002.ts", "2026/13/23/08/04/12-2.ts", "x.ts", "2026/10/23/08/04/12-2002.m4s"] {
        assert_eq!(parse_clock(bad), None, "{bad}");
    }
}

#[test]
fn the_media_playlist_lists_the_window_with_its_breaks() {
    let p = crate::hls::HlsParams { window_s: 6, ..Default::default() };
    let all: Vec<Entry> = (0..6).map(|n| entry(10 + n, u32::from(n >= 4), 1_790_856_000_000 + n as i64 * 2000)).collect();
    let m = media(&all, 2_400_000_000, &p);
    assert_eq!(m.segments.len(), 3, "a window of six seconds is three segments");
    let text = m.render(|e| format!("{}.ts", e.start_ms));
    assert!(text.contains("#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:13\n"), "{text}");
    assert_eq!(text.matches("#EXT-X-DISCONTINUITY\n").count(), 1, "{text}");
    assert!(text.contains("#EXTINF:2.000,\n1790856008000.ts"), "{text}");
}

#[test]
fn the_master_carries_the_first_sound_in_main_and_points_at_the_others() {
    let sound = |id: &str, sound: u8, lang: &str| Variant {
        id: id.into(),
        info: TrackInfo { codecs: "mp4a.40.2".into(), channels: 2, sound, lang: lang.into(), ..TrackInfo::default() },
        bandwidth: 130_000,
        average: Some(128_000),
    };
    let main = Variant {
        id: "main".into(),
        info: TrackInfo { codecs: "avc1.64001f".into(), width: 1280, height: 720, fps: Some((30, 1)), ..TrackInfo::default() },
        bandwidth: 1_700_000,
        average: Some(1_600_000),
    };
    let m = Master { main: Some(main), sounds: vec![sound("audio", 0, "eng"), sound("audio1", 1, "spa"), sound("audio2", 2, "fre")] };
    let text = m.render(|id| match id {
        None => "main/ts.m3u8".into(),
        Some(id) => format!("{id}/ts.m3u8"),
    });
    assert!(text.contains("NAME=\"English\",DEFAULT=YES,AUTOSELECT=YES,LANGUAGE=\"en\",CHANNELS=\"2\"\n"), "track 0 has no URI: {text}");
    assert!(text.contains("LANGUAGE=\"es\",CHANNELS=\"2\",URI=\"audio1/ts.m3u8\""), "{text}");
    assert!(text.contains("BANDWIDTH=1830000,AVERAGE-BANDWIDTH=1728000,CODECS=\"avc1.64001f,mp4a.40.2\",RESOLUTION=1280x720"), "{text}");
    assert!(text.ends_with("AUDIO=\"aud\"\nmain/ts.m3u8\n"), "{text}");
}
