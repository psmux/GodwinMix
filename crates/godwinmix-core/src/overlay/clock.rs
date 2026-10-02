//! Where a crawl has got to, in pixels, from the programme's running time.

/// Kept per source, so a change of speed carries on from the same place
/// instead of jumping.
#[derive(Default)]
pub struct Clock {
    epoch: Option<u64>,
    since: u64,
    base: f64,
    speed: f64,
}

impl Clock {
    /// Pixels travelled at `now` (nanoseconds of running time). A new
    /// `epoch` starts again from nothing.
    pub fn travelled(&mut self, now: u64, epoch: u64, speed: f64) -> f64 {
        if self.epoch != Some(epoch) {
            *self = Clock { epoch: Some(epoch), since: now, base: 0.0, speed };
        }
        let elapsed = now.saturating_sub(self.since) as f64 / 1e9;
        if (speed - self.speed).abs() > f64::EPSILON {
            self.base += elapsed * self.speed;
            self.since = now;
            self.speed = speed;
            return self.base;
        }
        self.base + elapsed * self.speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crawl_carries_on_from_where_it_was_when_its_speed_changes() {
        let mut c = Clock::default();
        assert_eq!(c.travelled(1_000_000_000, 0, 100.0), 0.0, "starts at the edge");
        assert_eq!(c.travelled(2_000_000_000, 0, 100.0), 100.0);
        // Twice as fast from here: no jump, then twice the pace.
        assert_eq!(c.travelled(2_000_000_000, 0, 200.0), 100.0);
        assert_eq!(c.travelled(3_000_000_000, 0, 200.0), 300.0);
        // New words start again from the edge.
        assert_eq!(c.travelled(4_000_000_000, 1, 200.0), 0.0);
    }
}
