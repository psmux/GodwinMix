//! Golden files for every playlist shape. `GMX_BLESS=1 cargo test -p
//! godwinmix-core hls::playlist` rewrites them after a deliberate change;
//! read the diff before committing it.

use super::*;
use crate::hls::ring::{Part, Ring};
use bytes::Bytes;

const SEG: u64 = 2_000_000_000;
const PART: u64 = 333_333_333;
/// 2026-09-30T12:00:00Z, so the dates in the golden files are fixed.
const T0: i64 = 1_790_856_000_000;

fn golden(name: &str, got: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/hls/golden").join(name);
    if std::env::var_os("GMX_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no golden file {}; run with GMX_BLESS=1 to write it", path.display()));
    assert_eq!(got, want, "{name} differs from its golden file");
}

/// A ring that has run for `whole` segments of six parts, plus `open` parts
/// of the next, starting at segment 100.
fn ring(whole: u64, open: usize) -> Ring {
    let mut r = Ring::new(40);
    r.set_init(Bytes::from_static(b"init"));
    for i in 0..=whole {
        let n = if i == whole { open } else { 6 };
        if n == 0 && i == whole {
            break;
        }
        r.begin((100 + i) * SEG, SEG, T0 + (i as i64) * 2000).unwrap();
        for p in 0..n {
            r.push_part(Part { bytes: Bytes::from_static(b"x"), duration_ns: PART, independent: p == 0 });
        }
    }
    if open == 0 {
        r.close();
    }
    r
}

fn reports() -> Vec<Report> {
    vec![
        Report { id: "720p".into(), last_msn: 104, last_part: Some(1) },
        Report { id: "480p".into(), last_msn: 104, last_part: Some(1) },
    ]
}

#[test]
fn low_latency_media_playlist() {
    let view = ring(4, 2).view();
    golden("ll-media.m3u8", &media(&view, &HlsParams::default(), &reports()));
}

#[test]
fn low_latency_playlist_at_a_segment_boundary_hints_the_next_one() {
    let view = ring(3, 0).view();
    let text = media(&view, &HlsParams::default(), &[]);
    assert!(text.ends_with("#EXT-X-PRELOAD-HINT:TYPE=PART,URI=\"103.0.m4s\"\n"), "{text}");
}

#[test]
fn plain_media_playlist_lists_only_whole_segments() {
    let p = HlsParams { part_ms: 0, window_s: 6, ..HlsParams::default() };
    let view = ring(5, 3).view();
    golden("media.m3u8", &media(&view, &p, &[]));
}

#[test]
fn a_new_init_is_a_discontinuity() {
    let mut r = ring(2, 0);
    r.set_init(Bytes::from_static(b"rebuilt"));
    r.begin(102 * SEG, SEG, T0 + 4000);
    r.push_part(Part { bytes: Bytes::from_static(b"x"), duration_ns: SEG, independent: true });
    r.close();
    let p = HlsParams { part_ms: 0, ..HlsParams::default() };
    golden("discontinuity.m3u8", &media(&r.view(), &p, &[]));
}

fn variant(id: &str, codecs: &str, w: u32, h: u32, kbps: u64) -> Variant {
    Variant {
        id: id.into(),
        info: TrackInfo { codecs: codecs.into(), width: w, height: h, fps: Some((30, 1)), ..TrackInfo::default() },
        bandwidth: kbps * 1000,
        average: Some(kbps * 900),
    }
}

#[test]
fn multivariant_playlist_for_a_four_rung_ladder() {
    let video = [
        variant("1080p", "avc1.640028", 1920, 1080, 6000),
        variant("720p", "avc1.64001f", 1280, 720, 3000),
        variant("480p", "avc1.4d401e", 854, 480, 1500),
        variant("360p", "avc1.42c01e", 640, 360, 800),
    ];
    let audio = Variant {
        id: "audio".into(),
        info: TrackInfo { codecs: "mp4a.40.2".into(), channels: 2, ..TrackInfo::default() },
        bandwidth: 128_000,
        average: Some(128_000),
    };
    golden("master.m3u8", &master(&video, Some(&audio)));
}

#[test]
fn audio_only_is_one_variant() {
    let audio = Variant {
        id: "audio".into(),
        info: TrackInfo { codecs: "mp4a.40.2".into(), channels: 2, ..TrackInfo::default() },
        bandwidth: 128_000,
        average: None,
    };
    golden("audio-only.m3u8", &master(&[], Some(&audio)));
}

#[test]
fn a_query_reaches_every_uri() {
    let view = ring(4, 2).view();
    let text = with_query(&media(&view, &HlsParams::default(), &reports()), "token=abc");
    golden("ll-media-token.m3u8", &text);
    for line in text.lines() {
        if !line.starts_with('#') || line.contains("URI=") {
            assert!(line.contains("token=abc"), "{line}");
        }
    }
}
