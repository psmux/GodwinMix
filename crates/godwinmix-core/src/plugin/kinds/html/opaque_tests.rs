//! The keep alive pushes the frame it holds again. Whether that frame comes
//! out stamped now or stamped when it was first pushed is up to `appsrc`,
//! and a stale stamp would read as a source gone back in time.

use super::*;
use gstreamer_app::AppSink;

fn canvas() -> CanvasCaps {
    CanvasCaps { width: 64, height: 36, fps: gst::Fraction::new(30, 1), sample_rate: 48_000, channels: 2 }
}

#[test]
fn a_frame_pushed_again_is_stamped_again() {
    let _ = gst::init();
    let o = Opaque::build("page-test", &canvas(), (64, 36)).unwrap();
    let pipeline = gst::Pipeline::new();
    let sink = AppSink::builder().sync(false).build();
    pipeline.add_many([o.src.upcast_ref(), sink.upcast_ref()]).unwrap();
    o.src.link(&sink).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let frame = vec![16u8; 64 * 36 * 3 / 2];
    assert!(o.show(&frame, 64, 36));
    let first = sink.try_pull_sample(gst::ClockTime::from_seconds(5)).expect("the first frame");
    std::thread::sleep(Duration::from_millis(300));
    let again = o.last.lock().0.clone().expect("a frame held for the keep alive");
    o.push(again);
    let second = sink.try_pull_sample(gst::ClockTime::from_seconds(5)).expect("the frame again");
    let _ = pipeline.set_state(gst::State::Null);
    let (a, b) = (first.buffer().unwrap().pts().unwrap(), second.buffer().unwrap().pts().unwrap());
    assert!(b >= a + gst::ClockTime::from_mseconds(250), "pushed again at {a} then {b}");
}
