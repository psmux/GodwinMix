use super::full_pool::{count_pad, silent};
use super::*;

/// A source added to a full pool, then taken, is drawn: its frames reach the
/// compositor pad of the slot it was given.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_added_to_a_full_pool_is_drawn_when_taken() {
    let ids = ["q0", "q1", "q2", "q3", "q4", "q5", "q6", "q7"];
    let mut mix = with_sources(&ids).await;
    mix.take(Some("q0".into()), None).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let late: SourceId = "late".into();
    let cfg: SourceConfig = toml::from_str("id = \"late\"\nuri = \"test://red\"\n").unwrap();
    mix.add_source(&cfg, None).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    mix.take(Some(late.clone()), None).unwrap();
    let slot = mix.pool.slots().iter().find(|s| s.source() == Some(&late) && s.showing()).map(|s| s.pad().clone());
    let pad = slot.expect("the late source is on a slot that is showing");
    let frames = count_pad(&pad);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let drawn = frames.load(Ordering::Relaxed);
    let eos = pad.pad_flags().contains(gst::PadFlags::EOS);
    mix.shutdown();
    assert!(drawn > 10, "the taken source's slot had {drawn} frames in a second (EOS {eos})");
}

/// Ten sources on eight slots, every one taken in turn twice over: whichever
/// slot a take lands on, the source taken is drawn there.
#[tokio::test(flavor = "multi_thread")]
async fn every_source_is_drawn_after_takes_move_every_slot() {
    let ids = ["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7"];
    let mut mix = with_sources(&ids).await;
    // Two that take a slot each and send nothing, as phones do at first.
    mix.add_source(&silent("quiet-0"), None).unwrap();
    mix.add_source(&silent("quiet-1"), None).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut dark = Vec::new();
    for round in 0..2 {
        for id in ids {
            let id: SourceId = id.into();
            mix.take(Some(id.clone()), None).unwrap();
            let pad = mix.pool.slots().iter().find(|s| s.showing()).map(|s| s.pad().clone()).unwrap();
            let drawn = count_pad(&pad);
            tokio::time::sleep(Duration::from_millis(400)).await;
            let n = drawn.load(Ordering::Relaxed);
            if n < 4 {
                dark.push(format!("{id} in round {round}: {n} frames"));
            }
        }
    }
    mix.shutdown();
    assert!(dark.is_empty(), "taken and not drawn: {dark:?}");
}
