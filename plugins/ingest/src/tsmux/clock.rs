//! Decode times that only go forward, per stream, as MPEG-TS requires.
//!
//! The tags carry the input's own times, and an input whose clock is
//! estimated from arrival can step back a little when its thread is
//! starved. A receiver given a decode time that goes backwards drops or
//! misorders frames, so a time at or before the last one is laid one tick
//! after it instead. It costs nothing while the times are good.

const WRAP: u64 = 1 << 33;

#[derive(Debug, Default, Clone, Copy)]
pub struct Forward(Option<u64>);

impl Forward {
    /// `dts`, or one tick after the last when it is not later, modulo the
    /// 33 bit wrap.
    pub fn next(&mut self, dts: u64) -> u64 {
        let out = match self.0 {
            Some(last) if (dts + WRAP - last) % WRAP == 0 || (dts + WRAP - last) % WRAP > WRAP / 2 => (last + 1) % WRAP,
            _ => dts,
        };
        self.0 = Some(out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_that_steps_back_is_laid_after_the_last_and_a_wrap_is_not_a_step_back() {
        let mut f = Forward::default();
        assert_eq!(f.next(9000), 9000);
        assert_eq!(f.next(12000), 12000);
        assert_eq!(f.next(11000), 12001, "back a little: one tick on");
        assert_eq!(f.next(12001), 12002, "the same time again: one tick on");
        assert_eq!(f.next(15000), 15000, "forward again: as it came");
        let mut w = Forward::default();
        w.next(WRAP - 10);
        assert_eq!(w.next(5), 5, "past the wrap is forward");
    }
}
