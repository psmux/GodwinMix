use super::*;
use crate::hls::ring::{Part, Ring};
use bytes::Bytes;

/// 2026-10-01T12:00:00Z.
const T0: i64 = 1_790_856_000_000;

fn golden(name: &str, got: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/hls/golden").join(name);
    if std::env::var_os("GMX_BLESS").is_some() {
        std::fs::write(&path, got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("no golden file {}; run with GMX_BLESS=1", path.display()));
    assert_eq!(got, want, "{name} differs from its golden file");
}

/// Segments of two seconds from segment 100, stamped at 90 kHz (video) or
/// 48 kHz (audio), the last one still open.
fn ring(timescale: u64, whole: u64) -> Ring {
    let mut r = Ring::new(40);
    r.set_init(Bytes::from_static(b"init"));
    for i in 0..=whole {
        r.begin((100 + i) * 2_000_000_000, 2_000_000_000, T0 + i as i64 * 2000);
        r.stamp(Some((100 + i) * 2 * timescale));
        r.push_part(Part { bytes: Bytes::from_static(b"x"), duration_ns: 2_000_000_000, independent: true });
    }
    r
}

fn info(codecs: &str, w: u32, h: u32, timescale: u32, channels: u32) -> TrackInfo {
    TrackInfo { codecs: codecs.into(), width: w, height: h, fps: (w > 0).then_some((30, 1)), channels, timescale, declared_kbps: 0 }
}

#[test]
fn a_ladder_and_its_audio_as_one_mpd() {
    let (v720, v360, aud) = (ring(90_000, 5).view(), ring(90_000, 5).view(), ring(48_000, 5).view());
    let (i720, i360, iaud) = (info("avc1.64001f", 1280, 720, 90_000, 0), info("avc1.42c01e", 640, 360, 90_000, 0), info("mp4a.40.2", 0, 0, 48_000, 2));
    let reps = [
        Rep { id: "720p", kind: TrackKind::Video, info: &i720, view: &v720, bandwidth: 3_128_000 },
        Rep { id: "360p", kind: TrackKind::Video, info: &i360, view: &v360, bandwidth: 928_000 },
        Rep { id: "audio", kind: TrackKind::Audio, info: &iaud, view: &aud, bandwidth: 128_000 },
    ];
    let text = mpd(&reps, &HlsParams::default(), T0 + 12_000, "key=abc&v=xyz").unwrap();
    golden("manifest.mpd", &text);
    // The first segment's wall clock less its media time: 200 s before T0.
    assert!(text.contains("availabilityStartTime=\"2026-10-01T11:56:40.000Z\""), "{text}");
    assert!(text.contains("<S t=\"18000000\" d=\"180000\" r=\"4\"/>"), "{text}");
    assert!(text.contains("media=\"720p/$Number$.m4s?key=abc&amp;v=xyz\" startNumber=\"100\""), "{text}");
}

#[test]
fn a_gap_in_decode_times_starts_a_new_run() {
    let mut r = ring(90_000, 2);
    r.begin(103 * 2_000_000_000, 2_000_000_000, T0 + 6000);
    r.stamp(Some(103 * 180_000 + 9_000));
    r.push_part(Part { bytes: Bytes::from_static(b"x"), duration_ns: 2_000_000_000, independent: true });
    r.close();
    let view = r.view();
    let segs = listed(&view, &HlsParams::default());
    // Segment 102 ran 100 ms long before 103 began, so it has a run of its own.
    assert_eq!(
        timeline(&segs, 90_000),
        "<S t=\"18000000\" d=\"180000\" r=\"1\"/><S t=\"18360000\" d=\"189000\"/><S t=\"18549000\" d=\"180000\"/>"
    );
}

#[test]
fn nothing_to_say_is_none() {
    let empty = View::default();
    let i = info("avc1.64001f", 1280, 720, 90_000, 0);
    let reps = [Rep { id: "720p", kind: TrackKind::Video, info: &i, view: &empty, bandwidth: 1 }];
    assert!(mpd(&reps, &HlsParams::default(), T0, "").is_none());
}
