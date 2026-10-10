//! One publisher at a time on a source's own port, and what happens when
//! it goes: driven through the gate, as a connection thread would, with a
//! real remuxer writing to a file.

use super::*;
use crate::media_tag::{MediaTag, TagKind};
use crate::remux::Out;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

struct Rig {
    gate: Arc<OneAtATime>,
    state: Arc<State>,
    ended: Arc<AtomicUsize>,
    path: std::path::PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn rig(name: &str) -> Option<Rig> {
    let path = std::env::temp_dir().join(format!("gmx-listen-{name}-{}.mkv", std::process::id()));
    let remux = match Remux::open(Out::File(path.clone()), None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping: {e}");
            return None;
        }
    };
    let state = Arc::new(State::new());
    let ended = Arc::new(AtomicUsize::new(0));
    let count = ended.clone();
    let end: End = Arc::new(move |_| {
        count.fetch_add(1, Ordering::Relaxed);
    });
    let gate = gate(&Settings::default(), None, remux, state.clone(), end);
    Some(Rig { gate, state, ended, path })
}

/// A kick that says it was called.
fn kick() -> (Kick, Arc<AtomicBool>) {
    let kicked = Arc::new(AtomicBool::new(false));
    let flag = kicked.clone();
    (Arc::new(move || flag.store(true, Ordering::Relaxed)), kicked)
}

fn tag(kind: TagKind, keyframe: bool) -> MediaTag {
    let payload: Arc<[u8]> = Arc::from(&[0x17u8, 1, 0, 0, 0, 0, 0, 0, 1][..]);
    MediaTag { kind, timestamp_ms: 0, keyframe, sequence_header: false, payload }
}

#[test]
fn a_publisher_that_left_before_its_first_keyframe_leaves_the_port_to_the_next() {
    let Some(r) = rig("early") else { return };
    let first = r.gate.admit("live", "cam", "a:1", kick().0).expect("the first is let in");
    drop(first);
    assert_eq!(r.state.bytes.load(Ordering::Relaxed), 0, "no FLV header before a keyframe");
    let second = r.gate.admit("live", "cam", "a:2", kick().0);
    assert!(second.is_ok(), "the next publisher is let in on the same stream");
    assert_eq!(r.ended.load(Ordering::Relaxed), 0, "nothing was written, so nothing needs a restart");
}

#[test]
fn a_second_publisher_is_refused_while_the_first_is_still_sending() {
    let Some(r) = rig("busy") else { return };
    let (k, kicked) = kick();
    let mut first = r.gate.admit("live", "cam", "a:1", k).expect("let in");
    first.tag(tag(TagKind::Audio, false));
    let why = r.gate.admit("live", "cam", "b:1", kick().0).err().expect("a second is refused");
    assert!(why.contains("already has a publisher"), "{why}");
    assert!(!kicked.load(Ordering::Relaxed), "a live publisher is never cut off");
}

#[test]
fn a_silent_publisher_is_cut_off_at_once_and_its_stream_ended_for_a_clean_restart() {
    let Some(r) = rig("silent") else { return };
    let (k, kicked) = kick();
    let mut first = r.gate.admit("live", "cam", "a:1", k).expect("let in");
    first.tag(tag(TagKind::Video, true));
    assert!(r.state.bytes.load(Ordering::Relaxed) > 13, "the header and the keyframe were written");
    // A pulled cable: nothing more comes, and the connection is still open.
    std::thread::sleep(STALE + Duration::from_millis(150));
    let why = r.gate.admit("live", "cam", "a:2", kick().0).err().expect("asked to come back");
    assert!(kicked.load(Ordering::Relaxed), "the silent one was cut off");
    assert!(why.contains("Publish again"), "{why}");
    // Its connection thread ends, as a kick makes it: the stream ends with it.
    drop(first);
    assert_eq!(r.ended.load(Ordering::Relaxed), 1, "the source ends its stream so the core restarts it clean");
    let after = r.gate.admit("live", "cam", "a:3", kick().0).err().expect("never spliced");
    assert!(after.contains("clean stream"), "{after}");
}

#[test]
fn a_silent_publisher_that_wrote_nothing_gives_the_port_straight_to_the_newcomer() {
    let Some(r) = rig("handover") else { return };
    let (k, kicked) = kick();
    let first = r.gate.admit("live", "cam", "a:1", k).expect("let in");
    std::thread::sleep(STALE + Duration::from_millis(150));
    let second = r.gate.admit("live", "cam", "a:2", kick().0);
    assert!(kicked.load(Ordering::Relaxed));
    assert!(second.is_ok(), "nothing reached the remuxer, so the newcomer starts it");
    drop(first);
    assert!(r.state.publisher.lock().unwrap().is_some(), "the old one leaving does not clear the new one");
    assert_eq!(r.ended.load(Ordering::Relaxed), 0);
}
