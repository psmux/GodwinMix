use super::*;

/// Count the buffers that reach the programme's raw video tee.
pub(super) fn count_programme(mix: &Mixer) -> Arc<std::sync::atomic::AtomicU64> {
    let frames = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counted = frames.clone();
    let tee = mix.program.by_name("vraw-tee").expect("the raw video tee");
    tee.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    frames
}

/// Count the buffers that reach a slot's queue with no segment in front of
/// them, which is what the compositor turns into an abort. Installed ahead of
/// the slot's own guard, so it sees what arrived rather than what was mended.
fn count_unsegmented(mix: &Mixer) -> Arc<std::sync::atomic::AtomicU64> {
    let bad = Arc::new(std::sync::atomic::AtomicU64::new(0));
    for slot in mix.pool.slots() {
        let gate = mix.program.by_name(&format!("slot-gate-{}", slot.index)).unwrap();
        let below = gate.static_pad("src").unwrap().peer().unwrap();
        let seen = bad.clone();
        gate.static_pad("src").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            if below.sticky_event::<gst::event::Segment>(0).is_none() {
                seen.fetch_add(1, Ordering::Relaxed);
            }
            gst::PadProbeReturn::Ok
        });
    }
    bad
}

/// A source that is added and never delivers, the way a phone takes a few
/// seconds to send its first picture.
fn silent(id: &str) -> SourceConfig {
    toml::from_str(&format!("id = \"{id}\"\nuri = \"rtmp://192.0.2.1/live/{id}\"\n")).unwrap()
}

fn on_air(mix: &Mixer) -> Option<SourceId> {
    mix.pool.slots().iter().find(|s| s.showing()).and_then(|s| s.source().cloned())
}

/// Nine sources and more on a pool of eight slots, the phone case from
/// 2026-10-05: adding a source to a full pool took the slot that was on air,
/// and the programme lost its picture until the next apply gave it back.
#[tokio::test(flavor = "multi_thread")]
async fn adding_a_source_to_a_full_pool_leaves_the_picture_on_air() {
    let ids = ["p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7"];
    let mut mix = with_sources(&ids).await;
    let live: SourceId = "p0".into();
    mix.take(Some(live.clone()), None).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let frames = count_programme(&mix);
    let mut after_add = Vec::new();
    for n in 0..3 {
        mix.add_source(&silent(&format!("late-{n}")), None).unwrap();
        after_add.push(on_air(&mix));
        mix.take(Some(live.clone()), None).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let before = frames.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let after = frames.load(Ordering::Relaxed);
    mix.shutdown();
    assert!(after_add.iter().all(|s| s.as_ref() == Some(&live)), "on air after each add: {after_add:?}");
    assert!(after > before + 10, "the programme stopped: {before} then {after}");
}

/// A source bound again to the slot it has just left, with nothing else
/// through that slot in between.
///
/// The slot's chain is flushed when it is unbound, and a flush takes the
/// segment off every pad it reaches. The valve at the head of the slot still
/// had the source's segment, the identical one came back with the source, and
/// a valve does not send a segment it believes it already has. So the
/// source's next frame reached the compositor with none, and
/// `gst_video_aggregator_fill_queues` aborted the process. This is the abort
/// the installed app hit with two phones live.
#[tokio::test(flavor = "multi_thread")]
async fn a_source_back_on_the_slot_it_left_brings_its_segment_with_it() {
    let mut mix = with_sources(&["back", "other"]).await;
    let id: SourceId = "back".into();
    mix.take(Some(id.clone()), None).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let frames = count_programme(&mix);
    let bad = count_unsegmented(&mix);
    for _ in 0..3 {
        mix.pool.drop_source(&id);
        mix.take(Some(id.clone()), None).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let before = frames.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let after = frames.load(Ordering::Relaxed);
    let showing = on_air(&mix);
    let unsegmented = bad.load(Ordering::Relaxed);
    mix.shutdown();
    assert_eq!(unsegmented, 0, "frames left a slot's valve with no segment below it");
    assert!(after > before + 10, "the programme stopped: {before} then {after}");
    assert_eq!(showing, Some(id), "the source did not come back on air");
}
