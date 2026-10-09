//! The deadline on a real slot: an RTMP output built and never live is
//! rebuilt once it has been down for `DOWN_FOR`, and not while a reconnect is
//! armed for it or once it has been taken away.

use super::deadline::DOWN_FOR;
use super::*;
use std::time::{Duration, Instant};

fn slot(id: &str) -> (gst::Pipeline, Arc<OutputSlot>) {
    let _ = gst::init();
    let program = gst::Pipeline::with_name(&format!("test-program-{id}"));
    let vtee = make("tee", &format!("{id}-vtee")).unwrap();
    vtee.set_property("allow-not-linked", true);
    let atee = make("tee", &format!("{id}-atee")).unwrap();
    atee.set_property("allow-not-linked", true);
    program.add_many([&vtee, &atee]).unwrap();
    let (tx, rx) = mpsc::channel(crate::mixer::BUS_QUEUE);
    std::mem::forget(rx);
    // Port 1 on loopback: nothing answers, and nothing here reads the bus.
    let cfg = OutputConfig::bare(id, &format!("rtmp://127.0.0.1:1/live/{id}"));
    let slot = OutputSlot::attach(&program, &vtee, &atee, &cfg, tx).unwrap();
    (program, slot)
}

#[test]
fn an_rtmp_output_down_past_the_deadline_with_nothing_armed_is_rebuilt() {
    let (program, slot) = slot("stuck");
    slot.refresh_connected();
    let later = Instant::now() + DOWN_FOR + Duration::from_secs(1);
    assert!(!slot.stuck_down(Instant::now()), "not straight after it was built");
    assert!(slot.stuck_down(later), "down past the deadline with no reconnect armed");

    // An error armed a reconnect: that is the way back, not this.
    assert!(slot.try_arm_reconnect());
    assert!(!slot.stuck_down(later));

    // The reconnect ran: the clock starts again from its build.
    slot.reconnect().unwrap();
    assert!(!slot.stuck_down(Instant::now() + Duration::from_secs(1)));

    // Taken away: nothing to rebuild.
    slot.shutdown();
    assert!(!slot.stuck_down(later + DOWN_FOR));
    let _ = program.set_state(gst::State::Null);
}
