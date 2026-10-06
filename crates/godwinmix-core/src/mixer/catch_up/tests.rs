use super::*;
use std::sync::Arc;
use std::time::Duration;

const MS: i64 = 1_000_000;

fn run(judge: &mut Judge, leads: &[Option<i64>], at: Instant) -> Vec<Verdict> {
    leads.iter().map(|l| judge.tick(*l, at)).collect()
}

#[test]
fn a_source_on_time_is_never_moved() {
    let mut judge = Judge::default();
    let t = Instant::now();
    let verdicts = run(&mut judge, &[Some(30 * MS); 40], t);
    assert!(verdicts.iter().all(|v| *v == Verdict::Hold), "{verdicts:?}");
}

/// The camera of 2026-10-06: every frame due 1.2 s after it arrived. Two
/// seconds of that and it is moved back by the least lead seen, keeping a
/// little so no frame turns up after its time.
#[test]
fn a_source_behind_for_two_seconds_is_caught_up_by_its_least_lead() {
    let mut judge = Judge::default();
    let t = Instant::now();
    let leads = [Some(1250 * MS), Some(1210 * MS), Some(1300 * MS), Some(1220 * MS)];
    let verdicts = run(&mut judge, &leads, t);
    assert_eq!(&verdicts[..3], &[Verdict::Hold; 3]);
    assert_eq!(verdicts[3], Verdict::CatchUp(1210 * MS - KEEP));
}

/// One tick with a frame on time, or with no frame, means the run was not a
/// backlog, and the count starts again.
#[test]
fn one_frame_on_time_starts_the_count_again() {
    let mut judge = Judge::default();
    let t = Instant::now();
    let leads = [Some(900 * MS), Some(900 * MS), Some(20 * MS), Some(900 * MS), None, Some(900 * MS)];
    assert!(run(&mut judge, &leads, t).iter().all(|v| *v == Verdict::Hold));
}

/// A stream that is not live fills its queues again to the same lead. The
/// second catch up within the relapse window is refused, and the guard is off.
#[test]
fn a_source_that_fills_up_again_is_left_alone() {
    let mut judge = Judge::default();
    let t = Instant::now();
    assert_eq!(run(&mut judge, &[Some(800 * MS); 4], t)[3], Verdict::CatchUp(800 * MS - KEEP));
    let soon = t + Duration::from_secs(3);
    assert_eq!(run(&mut judge, &[Some(790 * MS); 4], soon)[3], Verdict::GiveUp);
    let later = t + Duration::from_secs(60);
    assert!(run(&mut judge, &[Some(800 * MS); 8], later).iter().all(|v| *v == Verdict::Hold));
}

/// A backlog bigger than the programme's queues is measured short the first
/// time and caught up in two steps: the second finds less, and is allowed.
#[test]
fn a_backlog_bigger_than_the_queues_is_caught_up_in_two_steps() {
    let mut judge = Judge::default();
    let t = Instant::now();
    assert_eq!(run(&mut judge, &[Some(2000 * MS); 4], t)[3], Verdict::CatchUp(2000 * MS - KEEP));
    let soon = t + Duration::from_secs(3);
    assert_eq!(run(&mut judge, &[Some(900 * MS); 4], soon)[3], Verdict::CatchUp(900 * MS - KEEP));
}

#[test]
fn a_new_backlog_long_after_is_caught_up_again() {
    let mut judge = Judge::default();
    let t = Instant::now();
    run(&mut judge, &[Some(800 * MS); 4], t);
    let later = t + RELAPSE + Duration::from_secs(1);
    assert_eq!(run(&mut judge, &[Some(800 * MS); 4], later)[3], Verdict::CatchUp(800 * MS - KEEP));
}

/// The move itself: a src pad offset, which the pad applies to its segment
/// and sends again with the next buffer. What reaches the pad below places the
/// same buffer earlier by exactly the move, with nobody pushing an event.
#[test]
fn a_catch_up_reaches_the_pad_below_with_the_next_buffer() {
    let _ = gst::init();
    let segments: Arc<Mutex<Vec<gst::FormattedSegment<gst::ClockTime>>>> = Arc::default();
    let seen = segments.clone();
    let sink = gst::Pad::builder(gst::PadDirection::Sink)
        .name("below")
        .event_function(move |_, _, event| {
            if let gst::EventView::Segment(s) = event.view() {
                if let Some(s) = s.segment().downcast_ref::<gst::ClockTime>() {
                    seen.lock().push(s.clone());
                }
            }
            true
        })
        .chain_function(|_, _, _| Ok(gst::FlowSuccess::Ok))
        .build();
    let src = gst::Pad::builder(gst::PadDirection::Src).name("queue-src").build();
    src.link(&sink).unwrap();
    src.set_active(true).unwrap();
    sink.set_active(true).unwrap();

    let catch = CatchUp::default();
    catch.carry(&src);
    let _ = src.push_event(gst::event::StreamStart::new("cam"));
    let _ = src.push_event(gst::event::Segment::new(&gst::FormattedSegment::<gst::ClockTime>::new()));
    let pts = gst::ClockTime::from_seconds(10);
    let buffer = || {
        let mut b = gst::Buffer::new();
        b.get_mut().unwrap().set_pts(pts);
        b
    };
    src.push(buffer()).unwrap();
    assert_eq!(catch.apply(1200 * MS), 1200 * MS);
    src.push(buffer()).unwrap();

    let segments = segments.lock();
    assert_eq!(segments.len(), 2, "the segment is sent again with the next buffer");
    assert_eq!(segments[0].to_running_time(pts), Some(pts));
    assert_eq!(segments[1].to_running_time(pts), Some(pts - gst::ClockTime::from_mseconds(1200)));

    catch.reset();
    assert_eq!(src.offset(), 0, "a restarted source starts from no move at all");
}
