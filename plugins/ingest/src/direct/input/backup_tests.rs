use super::*;

/// Each tag as (timestamp, keyframe, sequence header, which side sent it).
type Seen = Vec<(u32, bool, bool, u8)>;

#[derive(Clone, Default)]
struct Record(Arc<Mutex<Seen>>);

impl TagSink for Record {
    fn tag(&mut self, t: MediaTag) {
        self.0.lock().unwrap().push((t.timestamp_ms, t.keyframe, t.sequence_header, t.payload[5]));
    }
    fn stats(&mut self, _: &InputStats) {}
}

/// A video tag whose sixth byte says which side sent it.
fn video(ms: u32, key: bool, header: bool, from: u8) -> MediaTag {
    let first = if key || header { 0x17 } else { 0x27 };
    let body = [first, u8::from(!header), 0, 0, 0, from];
    MediaTag { kind: TagKind::Video, timestamp_ms: ms, keyframe: key, sequence_header: header, payload: Arc::from(&body[..]) }
}

fn switch(out: Record) -> Switch {
    Switch { out: Box::new(out), legs: Default::default(), active: 0, want: 0, end_ms: 0, started: Instant::now() }
}

#[test]
fn a_stalled_main_hands_over_at_the_backups_keyframe_with_its_header_first() {
    let got = Record::default();
    let mut s = switch(got.clone());
    s.tag(0, video(0, true, true, 1));
    s.tag(0, video(0, true, false, 1));
    s.tag(0, video(40, false, false, 1));
    s.tag(1, video(9_000, true, true, 2));
    s.tag(1, video(9_000, true, false, 2));
    // The main goes quiet; the backup is fresh.
    s.legs[0].last = Some(Instant::now() - Duration::from_secs(3));
    s.decide(Duration::from_secs(2), Duration::from_secs(5));
    assert_eq!(s.want, 1);
    s.tag(1, video(9_040, false, false, 2));
    assert_eq!(s.active, 0, "no cut on a delta frame");
    s.tag(1, video(9_080, true, false, 2));
    assert_eq!(s.active, 1);
    let tags = got.0.lock().unwrap().clone();
    let from_backup: Vec<_> = tags.iter().filter(|t| t.3 == 2).collect();
    assert!(from_backup[0].2, "the backup's header goes first");
    assert_eq!(from_backup[1].0, 80, "laid 40 ms after the main's last tag");
    assert!(tags.windows(2).all(|w| w[0].0 <= w[1].0), "time never goes backwards: {tags:?}");
}

#[test]
fn the_main_comes_back_once_it_has_been_steady() {
    let got = Record::default();
    let mut s = switch(got.clone());
    s.active = 1;
    s.want = 1;
    s.tag(1, video(500, true, false, 2));
    s.tag(0, video(0, true, false, 1));
    s.decide(Duration::from_secs(2), Duration::from_secs(5));
    assert_eq!(s.want, 1, "not steady for long enough yet");
    s.legs[0].steady_since = Some(Instant::now() - Duration::from_secs(6));
    s.decide(Duration::from_secs(2), Duration::from_secs(5));
    assert_eq!(s.want, 0);
    s.tag(0, video(40, true, false, 1));
    assert_eq!(s.active, 0);
    let last = *got.0.lock().unwrap().last().unwrap();
    assert_eq!((last.0, last.3), (540, 1));
}
