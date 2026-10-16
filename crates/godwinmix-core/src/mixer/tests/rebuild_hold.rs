//! A source rebuilt while its last frame is held on air.
//!
//! The held branch stays in the programme pipeline while the replacement is
//! built, so the two must not share element names: GStreamer refuses a second
//! element with a name already in the bin, and the rebuild failed until the
//! hold ran out.

use super::supervision::Rig;
use super::*;

const ID: &str = "cam";

/// A mixer with one test pattern source live, on programme and having drawn.
async fn on_programme() -> Rig {
    let mut cfg = programme_config(crate::config::Accel::Software);
    cfg.stall.hold_last_frame = true;
    let mut rig = Rig::new(cfg);
    rig.add(&format!("id = \"{ID}\"\nuri = \"test://ball\"\n"));
    rig.until(ID, SourceState::Live, Duration::from_secs(30)).await.expect("the source goes live");
    rig.mix.take(Some(ID.into()), None).expect("the source goes to programme");
    rig.run(Duration::from_millis(500)).await;
    assert!(rig.slot(ID).expect("the slot").input.last_video.seen() > 0, "no picture yet");
    rig
}

/// Rebuild the source and run until it is back in the mixer. The add waits for
/// the old pipeline to stop (`park_while_stopping`), so it arrives on a later
/// turn of the loop and not from the call.
async fn rebuild(rig: &mut Rig) {
    rig.mix.rebuild_source(&ID.to_string());
    for _ in 0..1000 {
        if rig.slot(ID).is_some() {
            return;
        }
        rig.run(Duration::from_millis(5)).await;
    }
    panic!("the source was not added again while its old branch was held");
}

/// Run until the rebuilt source has delivered its first picture and the held
/// branch has been let go.
async fn until_settled(rig: &mut Rig) {
    for _ in 0..300 {
        let delivered = rig.slot(ID).is_some_and(|s| s.input.last_video.seen() > 0);
        if delivered && rig.mix.retired.is_empty() {
            return;
        }
        rig.run(Duration::from_millis(50)).await;
    }
    panic!("the rebuilt source never settled: present {}, held {}", rig.slot(ID).is_some(), rig.mix.retired.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_rebuild_comes_back_and_the_frame_is_held_until_it_has_media() {
    let mut rig = on_programme().await;
    let before = rig.mix.program.children().len();
    rebuild(&mut rig).await;

    // Built beside the held branch, on the first attempt.
    assert_eq!(rig.mix.rebuild_failures.get(ID).copied().unwrap_or(0), 1, "one attempt, not retries");
    assert_eq!(rig.mix.retired.len(), 1, "the old branch should hold the last frame");
    assert_eq!(rig.mix.program_source.as_deref(), Some(ID), "the source is back on programme");
    assert!(
        rig.mix.program.children().len() > before,
        "two branches should be in the programme while the frame is held"
    );
    // Not let go before the new source has a picture of its own.
    assert_eq!(rig.slot(ID).unwrap().input.last_video.seen(), 0, "this test needs to look before the first frame");

    until_settled(&mut rig).await;
    assert_eq!(rig.mix.program.children().len(), before, "the held branch was not removed");
    rig.mix.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn fifty_rebuilds_leave_nothing_behind() {
    let mut rig = on_programme().await;
    let before = rig.mix.program.children().len();
    for round in 0..50 {
        rebuild(&mut rig).await;
        // Every fifth round the next rebuild comes before the first picture,
        // which is the held branch being replaced by one that never drew.
        if round % 5 != 4 {
            until_settled(&mut rig).await;
        }
    }
    until_settled(&mut rig).await;
    let after = rig.mix.program.children().len();
    rig.mix.shutdown();
    assert_eq!(after, before, "elements leaked into the programme over 50 rebuilds");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_rebuild_does_not_move_the_programmes_frame_timing() {
    let mut rig = on_programme().await;
    let frame = rig.mix.canvas.frame_duration().nseconds();
    let gaps = Arc::new(Gaps::default());
    gaps.watch(&rig.mix.venc_tee.static_pad("sink").expect("the encoder tee has a sink pad"));
    gaps.wait_for(15).await;
    gaps.largest.store(0, Ordering::Relaxed);

    for _ in 0..5 {
        rebuild(&mut rig).await;
        until_settled(&mut rig).await;
    }
    let largest = gaps.largest.load(Ordering::Relaxed);
    rig.mix.shutdown();
    println!("rebuild timing: largest programme interval {:.1} ms, one frame is {:.1} ms", largest as f64 / 1e6, frame as f64 / 1e6);
    assert!(largest <= frame * 2, "five rebuilds cost the programme {largest} ns, over two frames ({} ns)", frame * 2);
}

/// Two branches of one source differ in every element name, and a name still
/// says which source it belongs to, hyphens in the id and all.
#[test]
fn branch_names_differ_by_generation_and_still_name_their_source() {
    use crate::plugin::branch::{id_in_element_name, meter_name};
    assert_ne!(meter_name("cam-1", 3), meter_name("cam-1", 4));
    assert_eq!(id_in_element_name("pgm-vq-cam-1@4", &["vq"]), Some("cam-1"));
    assert_eq!(id_in_element_name(&meter_name("cam-1", 4), &["alevel"]), Some("cam-1"));
    assert_eq!(id_in_element_name("pgm-vq-cam-1@4", &["aq"]), None);
    assert_eq!(id_in_element_name("pgm-vq-@4", &["vq"]), None);
    assert_eq!(id_in_element_name("pgm-v-q", &["vq"]), None);
}
