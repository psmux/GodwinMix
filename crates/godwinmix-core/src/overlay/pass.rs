//! Something drawn over the whole programme frame for a while: an effect
//! clip, the clip of a stinger, a luma matte wipe, a shader transition.
//!
//! A pass is not a source. It has no slot, no scene item and no tile; it is
//! put on the board when something asks for it, painted by the board's probe
//! after the transparent sources, and dropped when it says it has finished.
//! With no pass and no transparent source the probe is not there at all, so
//! a show that never fires an effect runs the graph it always ran.
//!
//! `paint` runs on the compositor's streaming thread, once a frame. It may
//! read and write memory it already has and do arithmetic. It may not wait:
//! a pass whose next picture is not ready draws the last one, or nothing.

use super::blend::Planes;
use std::sync::Arc;

pub trait Pass: Send + Sync {
    /// Paint onto the frame for running time `now`, in nanoseconds.
    fn paint(&self, frame: &mut Planes<'_>, now: u64);

    /// True once there is nothing more to draw. The board drops it on the
    /// next frame.
    fn finished(&self) -> bool {
        false
    }

    /// What a log line calls it.
    fn name(&self) -> &str;
}

/// How many frames in a row a pass may take longer than a frame to paint
/// before it is taken off. A third of a second at 30 fps: long enough not to
/// punish one slow frame, short enough that a machine that cannot afford an
/// effect at this canvas loses the effect and not the programme.
pub const SLOW_FRAMES: u32 = 10;

/// The board's list: each pass, the number it was put on under, and how
/// many frames in a row it has been too slow.
#[derive(Default)]
pub struct Passes {
    next: u64,
    pub(super) list: Vec<(u64, Arc<dyn Pass>, u32)>,
}

impl Passes {
    pub(super) fn add(&mut self, pass: Arc<dyn Pass>) -> u64 {
        self.next += 1;
        self.list.push((self.next, pass, 0));
        self.next
    }

    pub(super) fn remove(&mut self, id: u64) {
        self.list.retain(|(i, ..)| *i != id);
    }

    /// The passes to paint this frame, with the finished ones gone.
    pub(super) fn live(&mut self) -> Vec<(u64, Arc<dyn Pass>)> {
        self.list.retain(|(_, p, _)| !p.finished());
        self.list.iter().map(|(i, p, _)| (*i, p.clone())).collect()
    }

    /// Note what each pass cost this frame against `budget`, and take off
    /// any that has been over it `SLOW_FRAMES` times running. Answers the
    /// names of those taken off.
    pub(super) fn spent(&mut self, costs: &[(u64, std::time::Duration)], budget: std::time::Duration) -> Vec<String> {
        let mut dropped = Vec::new();
        for (id, cost) in costs {
            if let Some(e) = self.list.iter_mut().find(|e| e.0 == *id) {
                e.2 = if *cost > budget { e.2 + 1 } else { 0 };
                if e.2 >= SLOW_FRAMES {
                    dropped.push(e.1.name().to_string());
                }
            }
        }
        self.list.retain(|e| e.2 < SLOW_FRAMES);
        dropped
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct Still;
    impl Pass for Still {
        fn paint(&self, _: &mut Planes<'_>, _: u64) {}
        fn name(&self) -> &str {
            "still"
        }
    }

    #[test]
    fn a_pass_slower_than_a_frame_ten_times_running_is_taken_off() {
        let mut p = Passes::default();
        let id = p.add(Arc::new(Still));
        let frame = Duration::from_millis(33);
        for _ in 0..SLOW_FRAMES - 1 {
            assert!(p.spent(&[(id, Duration::from_millis(50))], frame).is_empty());
        }
        // One quick frame starts the count again.
        assert!(p.spent(&[(id, Duration::from_millis(5))], frame).is_empty());
        for _ in 0..SLOW_FRAMES - 1 {
            p.spent(&[(id, Duration::from_millis(50))], frame);
        }
        assert_eq!(p.spent(&[(id, Duration::from_millis(50))], frame), vec!["still".to_string()]);
        assert!(p.is_empty(), "the slow pass is gone and the programme keeps its time");
    }
}
