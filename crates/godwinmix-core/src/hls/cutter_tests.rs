use super::*;
use crate::hls::track::TrackKind;
use crate::hls::HlsParams;

fn boxed(kind: &[u8; 4], len: usize) -> Vec<u8> {
    let mut v = (len as u32).to_be_bytes().to_vec();
    v.extend_from_slice(kind);
    v.resize(len, 0);
    v
}

fn moof(delta: bool, at_ms: u64) -> Piece {
    Piece {
        header: true,
        delta,
        running_ns: Some(at_ms * 1_000_000),
        duration_ns: Some(333_333_333),
        ..Piece::default()
    }
}

const SAMPLE: Piece = Piece { header: false, delta: true, marker: false, running_ns: None, duration_ns: None, wall_ms: 0 };
const LAST: Piece = Piece { marker: true, ..SAMPLE };

fn track() -> Arc<Track> {
    Arc::new(Track::new("720p", TrackKind::Video, HlsParams::default(), 3000))
}

#[test]
fn chunks_become_parts_and_fragments_become_segments() {
    let t = track();
    let mut c = Cutter::new(t.clone(), false, false);
    c.push(&boxed(b"ftyp", 40), Piece { header: true, ..Piece::default() });
    for f in 0..2u64 {
        for chunk in 0..6u64 {
            c.push(&boxed(b"moof", 100), moof(chunk > 0, 2000 * (f + 50) + chunk * 333));
            c.push(&[1; 500], SAMPLE);
            c.push(&[2; 500], LAST);
        }
    }
    let pos = t.position();
    assert_eq!(pos.complete, Some(50), "the first fragment closed when the second began");
    assert_eq!(pos.open, Some((51, 6)), "the second is still open: nothing said it ended");
    assert_eq!(t.segment(50).unwrap().iter().map(Bytes::len).sum::<usize>(), 6 * 1100);
    assert_eq!(t.part(51, 5).unwrap().len(), 1100);
    let view = t.view();
    assert_eq!(view.segments[0].parts.iter().filter(|(_, i)| *i).count(), 1, "only the first part holds a keyframe");
    c.eos();
    assert_eq!(t.position().complete, Some(51));
}

#[test]
fn whole_fragments_close_at_their_marker() {
    let t = track();
    let mut c = Cutter::new(t.clone(), false, true);
    c.push(&boxed(b"ftyp", 40), Piece { header: true, ..Piece::default() });
    c.push(&boxed(b"moof", 100), moof(false, 0));
    c.push(&[1; 10], LAST);
    assert_eq!(t.position().complete, Some(0));
}

#[test]
fn samples_before_the_first_moof_are_dropped() {
    let t = track();
    let mut c = Cutter::new(t.clone(), false, false);
    c.push(&[1; 10], LAST);
    c.push(&boxed(b"ftyp", 40), Piece { header: true, ..Piece::default() });
    assert_eq!(t.position(), Default::default());
}

#[test]
fn audio_parts_are_all_independent() {
    let t = track();
    let mut c = Cutter::new(t.clone(), true, false);
    c.push(&boxed(b"ftyp", 40), Piece { header: true, ..Piece::default() });
    c.push(&boxed(b"moof", 100), moof(false, 0));
    c.push(&[1; 10], LAST);
    c.push(&boxed(b"moof", 100), moof(true, 333));
    c.push(&[1; 10], LAST);
    assert!(t.view().segments[0].parts.iter().all(|(_, i)| *i));
}
