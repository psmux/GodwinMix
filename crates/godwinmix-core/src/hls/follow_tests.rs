//! The sound cut where the picture is cut, and a break marked alike on
//! both, with plain bytes and no GStreamer: the picture's segments are
//! begun by hand at its keyframes, the sound's fragments pushed as a muxer
//! writes them.

use super::*;
use crate::hls::playlist::media;
use crate::hls::track::TrackKind;
use crate::hls::HlsParams;

const MS: u64 = 1_000_000;

fn stream(segment_ms: u32) -> Arc<Stream> {
    let p = HlsParams { segment_ms, part_ms: 0, window_s: 60, ..HlsParams::default() };
    Arc::new(Stream::new("watch", p, &"k".repeat(24)))
}

/// One segment of picture from `start` lasting `len` ms, begun and closed
/// as `cmafmux` without parts hands it over.
fn picture(v: &Track, start: u64, len: u64) {
    v.begin(start * MS, 1_790_856_000_000 + start as i64, None);
    v.push_part(Part { bytes: Bytes::from_static(b"v"), duration_ns: len * MS, independent: true });
    v.close();
}

/// The sound's fragments of 250 ms from `from` to `to` ms.
fn sound(f: &mut Follow, from: u64, to: u64) {
    for t in (from..to).step_by(250) {
        f.start(t * MS, 1_790_856_000_000 + t as i64, None);
        f.push(Bytes::from_static(b"a"), 250 * MS);
    }
}

/// Media sequence number, discontinuity and start of every whole segment.
fn cut(t: &Track) -> Vec<(u64, u32, u64)> {
    t.view().segments.iter().filter(|s| s.complete).map(|s| (s.msn, s.disc, s.start_ns / MS)).collect()
}

#[test]
fn sound_is_cut_where_eight_second_keyframes_cut_the_picture() {
    let s = stream(2000);
    let v = s.add_track("main", TrackKind::Video, 0);
    let a = s.add_track("audio", TrackKind::Audio, 0);
    v.set_init(Bytes::from_static(b"v"));
    a.set_init(Bytes::from_static(b"a"));
    let mut f = Follow::new(&s, a.clone());
    // The picture's keyframes come every 8 s, so its segments are 8 s long
    // although 2 s were asked for. The sound reaches the muxer as it plays,
    // a little ahead of the picture's segment that holds it.
    for k in 0..4u64 {
        sound(&mut f, k * 8000, k * 8000 + 2000);
        picture(&v, k * 8000, 8000);
        sound(&mut f, k * 8000 + 2000, k * 8000 + 8000);
    }
    sound(&mut f, 32_000, 32_500);
    let (vs, aus) = (cut(&v), cut(&a));
    assert_eq!(vs.iter().map(|s| s.0).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
    assert_eq!(aus.iter().map(|s| s.0).collect::<Vec<_>>(), vec![0, 1, 2, 3], "one segment of sound per segment of picture");
    for (pv, pa) in vs.iter().zip(&aus) {
        assert_eq!(pv.2, pa.2, "segment {} starts at the same time", pv.0);
    }
}

#[test]
fn a_restart_is_one_discontinuity_at_the_same_number_on_both() {
    let s = stream(2000);
    let v = s.add_track("main", TrackKind::Video, 0);
    let a = s.add_track("audio", TrackKind::Audio, 0);
    v.set_init(Bytes::from_static(b"v"));
    a.set_init(Bytes::from_static(b"a"));
    let mut f = Follow::new(&s, a.clone());
    for k in 0..3u64 {
        picture(&v, k * 2000, 2000);
        sound(&mut f, k * 2000, k * 2000 + 2000);
    }
    // The sound of a segment the picture never finished, then the feed goes.
    sound(&mut f, 6000, 7000);
    s.restart();
    // The new pipeline: new muxers, new headers (the sound's twice, as a
    // muxer that wrote its header again would), time from zero.
    v.set_init(Bytes::from_static(b"v2"));
    a.set_init(Bytes::from_static(b"a2"));
    a.set_init(Bytes::from_static(b"a3"));
    let mut f = Follow::new(&s, a.clone());
    for k in 0..3u64 {
        picture(&v, k * 2000, 2000);
        sound(&mut f, k * 2000, k * 2000 + 2000);
    }
    sound(&mut f, 6000, 6250);
    let (vs, aus) = (cut(&v), cut(&a));
    assert_eq!(vs, aus.iter().map(|s| (s.0, s.1, s.2)).collect::<Vec<_>>(), "the same numbers, discontinuities and times");
    assert_eq!(vs.iter().map(|s| (s.0, s.1)).collect::<Vec<_>>(), vec![(0, 0), (1, 0), (2, 0), (3, 1), (4, 1), (5, 1)]);
    let (pv, pa) = (media(&v.view(), &s.params, &[]), media(&a.view(), &s.params, &[]));
    assert_eq!(pv.matches("#EXT-X-DISCONTINUITY\n").count(), 1, "{pv}");
    assert_eq!(pa.matches("#EXT-X-DISCONTINUITY\n").count(), 1, "a header written twice is a new map, not a second break: {pa}");
    // A second restart before anything was cut is still one break.
    s.restart();
    s.restart();
    picture(&v, 0, 2000);
    let mut f = Follow::new(&s, a.clone());
    sound(&mut f, 0, 2000);
    assert_eq!(v.view().segments.last().map(|s| (s.msn, s.disc)), Some((6, 2)));
    assert_eq!(a.view().segments.last().map(|s| (s.msn, s.disc)), Some((6, 2)));
}

#[test]
fn sound_waits_for_a_picture_that_has_not_cut_yet_and_drops_what_came_before_it() {
    let s = stream(2000);
    let v = s.add_track("main", TrackKind::Video, 0);
    let a = s.add_track("audio", TrackKind::Audio, 0);
    v.set_init(Bytes::from_static(b"v"));
    a.set_init(Bytes::from_static(b"a"));
    let mut f = Follow::new(&s, a.clone());
    // Sound from before the first keyframe at 1 s, and while the picture
    // has said nothing.
    sound(&mut f, 0, 3000);
    assert!(a.view().segments.is_empty(), "nothing placed before the picture cut");
    picture(&v, 1000, 2000);
    sound(&mut f, 3000, 3250);
    let segs = a.view().segments;
    assert_eq!(segs.first().map(|s| (s.msn, s.start_ns / MS)), Some((1, 1000)), "the sound starts with the picture");
}

#[test]
fn sound_with_no_picture_is_cut_on_its_own_clock() {
    let s = stream(2000);
    let a = s.add_track("audio", TrackKind::Audio, 0);
    a.set_init(Bytes::from_static(b"a"));
    let mut f = Follow::new(&s, a.clone());
    sound(&mut f, 0, 6250);
    assert_eq!(cut(&a).iter().map(|s| (s.0, s.2)).collect::<Vec<_>>(), vec![(0, 0), (1, 2000), (2, 4000)]);
}
