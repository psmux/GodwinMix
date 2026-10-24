//! The TS cutter against a picture's ring begun by hand, as `cmafmux`
//! without parts fills it: segment n arrives whole once the keyframe after
//! it has gone by.

use super::*;
use crate::hls::ts::MAIN;
use crate::hls::ring::Part;
use crate::hls::track::{Track, TrackKind};
use crate::hls::HlsParams;
use bytes::Bytes;

const AVCC: [u8; 15] = [1, 0x64, 0, 0x1f, 0xff, 0xe1, 0, 2, 0x67, 0xaa, 1, 0, 2, 0x68, 0xbb];

fn unit(kind: UnitKind, ms: u32, key: bool, data: &'static [u8]) -> Unit {
    Unit { kind, ms, cts_ms: 0, key, data: Bytes::from_static(data) }
}

fn picture(v: &Track, start: u64, len: u64) {
    v.begin(start * MS, 1_790_856_000_000 + start as i64, None);
    v.push_part(Part { bytes: Bytes::from_static(b"v"), duration_ns: len * MS, independent: true });
    v.close();
}

/// Every packet's PID, start flag, continuity counter and random access flag.
fn packets(ts: &[u8]) -> Vec<(u16, bool, u8, bool)> {
    assert_eq!(ts.len() % 188, 0);
    ts.chunks(188)
        .map(|p| {
            assert_eq!(p[0], 0x47);
            let ra = p[3] & 0x20 != 0 && p[4] > 0 && p[5] & 0x40 != 0;
            ((u16::from(p[1] & 0x1f) << 8) | u16::from(p[2]), p[1] & 0x40 != 0, p[3] & 0x0f, ra)
        })
        .collect()
}

/// Ten seconds: a keyframe every second, segments of two, sound every 20 ms
/// on tracks 0 and 1, the hub's order. Answers the stream, main and audio1.
fn run() -> (Arc<Stream>, Arc<TsTrack>, Arc<TsTrack>) {
    let s = Arc::new(Stream::new("ch", HlsParams { window_s: 60, ..HlsParams::default() }, &"k".repeat(24)));
    let v = s.add_track("main", TrackKind::Video, 0);
    v.set_init(Bytes::from_static(b"i"));
    let (main, alone) = (s.add_ts(MAIN, None), s.add_ts("audio1", Some(1)));
    // The hub's times start at 5000 ms, the pipeline's at zero.
    let (mut m, mut a) = (TsCut::new(&s, main.clone(), 5000), TsCut::new(&s, alone.clone(), 5000));
    m.push(unit(UnitKind::AvcConfig, 5000, false, &AVCC));
    m.push(unit(UnitKind::AacConfig, 5000, false, &[0x11, 0x90]));
    a.push(unit(UnitKind::AacConfig, 5000, false, &[0x11, 0x90]));
    for t in (0..10_000u32).step_by(20) {
        if t % 40 == 0 {
            let key = t % 1000 == 0;
            m.push(unit(UnitKind::Video, 5000 + t, key, if key { &[0, 0, 0, 3, 0x65, 1, 2] } else { &[0, 0, 0, 3, 0x41, 1, 2] }));
            if key && t % 2000 == 0 && t > 0 {
                // The muxer has the keyframe that ends the segment before.
                picture(&v, u64::from(t) - 2000, 2000);
            }
        }
        m.push(unit(UnitKind::Audio, 5000 + t, false, &[0x21, 0x10, 0x04]));
        a.push(unit(UnitKind::Audio, 5000 + t, false, &[0x21, 0x10, 0x04]));
    }
    (s, main, alone)
}

#[test]
fn main_is_cut_at_the_cmaf_keyframes_with_the_same_numbers_and_starts_clean() {
    let (_s, main, _) = run();
    let (entries, _) = main.listing();
    assert_eq!(entries.iter().map(|e| e.msn).collect::<Vec<_>>(), vec![0, 1, 2, 3], "the open one is not listed");
    for e in &entries {
        assert_eq!(e.duration_ms, 2000);
        assert_eq!(e.start_ms, 1_790_856_000_000 + e.msn as i64 * 2000, "the picture's wall clock");
        let p = packets(&main.segment(e.msn).unwrap());
        assert_eq!((p[0].0, p[0].1), (0, true), "segment {} starts with the PAT", e.msn);
        assert_eq!(p[1].0, 0x1000, "then the PMT");
        assert!(p[2].0 == 0x100 && p[2].1 && p[2].3, "then a keyframe a decoder can start at");
    }
    // Two segments back to back are one stream: every PID's counter runs on.
    let both = [main.segment(1).unwrap(), main.segment(2).unwrap()].concat();
    let mut last = std::collections::HashMap::new();
    for (pid, _, cc, _) in packets(&both) {
        if let Some(prev) = last.insert(pid, cc) {
            assert_eq!(cc, (prev + 1) & 0x0f, "continuity on PID {pid:#x}");
        }
    }
}

#[test]
fn a_further_sound_alone_is_cut_beside_the_picture_and_each_segment_has_its_tables() {
    let (_s, main, alone) = run();
    let (a, _) = alone.listing();
    let (m, _) = main.listing();
    assert_eq!(a.iter().map(|e| e.msn).collect::<Vec<_>>(), m.iter().map(|e| e.msn).collect::<Vec<_>>());
    for e in &a {
        assert!(e.duration_ms.abs_diff(2000) <= 20, "{e:?}");
        let p = packets(&alone.segment(e.msn).unwrap());
        assert_eq!((p[0].0, p[1].0, p[2].0), (0, 0x1000, 0x101), "PAT, PMT, then the sound");
    }
}

/// A GPU encoder's timing: a keyframe every 50 frames at 29.97 fps
/// (1668 ms), and the sound reaching the hub 0.7 s behind the picture of
/// the same moment, so the frame queued last is often older than the
/// keyframe waiting at the front.
#[test]
fn sound_arriving_late_does_not_cost_a_waiting_keyframe_its_cut() {
    let s = Arc::new(Stream::new("ch", HlsParams { segment_ms: 1000, window_s: 60, ..HlsParams::default() }, &"k".repeat(24)));
    let v = s.add_track("main", TrackKind::Video, 0);
    v.set_init(Bytes::from_static(b"i"));
    let main = s.add_ts(MAIN, None);
    let mut m = TsCut::new(&s, main.clone(), 0);
    m.push(unit(UnitKind::AvcConfig, 0, false, &AVCC));
    m.push(unit(UnitKind::AacConfig, 0, false, &[0x11, 0x90]));
    // (arrival, time, video): picture as it is made, sound 700 ms later.
    // (arrival, time, frame number for the picture)
    let mut events: Vec<(u32, u32, Option<u32>)> = (0..600u32).map(|f| (f * 1001 / 30, f * 1001 / 30, Some(f))).collect();
    events.extend((0..960u32).map(|a| (a * 21 + 700, a * 21, None)));
    events.sort();
    // The muxer hands over a segment 60 ms after the keyframe that ends it
    // reached the cutter: the queue, the parser and the muxer between.
    let (mut last_key, mut due): (Option<u32>, Option<(u32, u32, u32)>) = (None, None);
    for (arrival, t, frame) in events {
        if let Some((_, k, end)) = due.filter(|d| d.0 <= arrival) {
            picture(&v, u64::from(k), u64::from(end - k));
            due = None;
        }
        if let Some(f) = frame {
            let key = f % 50 == 0;
            m.push(unit(UnitKind::Video, t, key, if key { &[0, 0, 0, 3, 0x65, 1, 2] } else { &[0, 0, 0, 3, 0x41, 1, 2] }));
            if key {
                due = last_key.map(|k| (arrival + 60, k, t));
                last_key = Some(t);
            }
        } else {
            m.push(unit(UnitKind::Audio, t, false, &[0x21, 0x10, 0x04]));
        }
    }
    let (entries, _) = main.listing();
    let leader: Vec<u64> = v.view().segments.iter().map(|s| s.msn).collect();
    assert!(entries.len() >= 9, "a segment per keyframe: {entries:?}");
    for e in &entries {
        assert!(e.duration_ms.abs_diff(1668) <= 2, "{e:?}");
        assert!(leader.contains(&e.msn), "the CMAF picture's numbers");
    }
}

#[test]
fn sound_behind_the_picture_does_not_make_the_queue_look_held_for_ever() {
    use super::span;
    let unit = |kind, ms| Unit { kind, ms, cts_ms: 0, key: kind == UnitKind::Video, data: bytes::Bytes::new() };
    let q: std::collections::VecDeque<Unit> = [unit(UnitKind::Video, 10_000), unit(UnitKind::Audio, 9_300), unit(UnitKind::Video, 10_033)].into_iter().collect();
    assert_eq!(span(&q), 33, "the newest time in the queue less the front's, never a wrap");
    assert_eq!(span(&std::collections::VecDeque::new()), 0);
}
