//! Collapsing a plugin's stderr when it says the same thing over and over.
//!
//! A GStreamer assertion that fails once a frame prints thirty lines a
//! second, each with its own timestamp, and buries everything else in the
//! log and the terminal. Lines are compared with their digits taken out, so
//! two lines that differ only by a time or a process id count as the same.
//! The first is logged; the rest are counted, and a line saying how many
//! went by is logged every [`EVERY`] while they keep coming and once when
//! something else is said.

use std::time::{Duration, Instant};

/// How often a running count is reported while the same line repeats.
pub const EVERY: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct Repeats {
    last: String,
    count: u64,
    since: Option<Instant>,
}

/// What to log for one line, in order: nothing, a count, the line, or both.
#[derive(Debug, PartialEq)]
pub enum Say {
    /// Log the line itself.
    Line,
    /// Log that the previous line was repeated this many more times, then the line.
    CountThenLine(u64),
    /// Log only that the line went by this many more times.
    Count(u64),
    /// Stay quiet; it is counted.
    Nothing,
}

impl Repeats {
    pub fn see(&mut self, line: &str, now: Instant) -> Say {
        let key = shape(line);
        if key == self.last {
            self.count += 1;
            let since = *self.since.get_or_insert(now);
            if now.duration_since(since) >= EVERY {
                let n = std::mem::take(&mut self.count);
                self.since = Some(now);
                return Say::Count(n);
            }
            return Say::Nothing;
        }
        let pending = std::mem::take(&mut self.count);
        self.last = key;
        self.since = Some(now);
        if pending > 0 {
            Say::CountThenLine(pending)
        } else {
            Say::Line
        }
    }
}

/// The line without its digits, so times and ids do not make it new.
fn shape(line: &str) -> String {
    line.chars().filter(|c| !c.is_ascii_digit()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "GStreamer-Video-CRITICAL **: 01:27:44.757: gst_video_frame_map_id: assertion failed";
    const B: &str = "GStreamer-Video-CRITICAL **: 01:27:44.791: gst_video_frame_map_id: assertion failed";

    #[test]
    fn a_line_that_differs_only_by_its_time_is_counted_not_logged() {
        let (mut r, t) = (Repeats::default(), Instant::now());
        assert_eq!(r.see(A, t), Say::Line);
        assert_eq!(r.see(B, t), Say::Nothing);
        assert_eq!(r.see(A, t), Say::Nothing);
        assert_eq!(r.see("something else", t), Say::CountThenLine(2));
        assert_eq!(r.see("something else", t), Say::Nothing);
    }

    #[test]
    fn a_line_that_keeps_coming_is_reported_every_ten_seconds() {
        let t = Instant::now();
        let mut counts = 0;
        let mut r = Repeats::default();
        r.see(A, t);
        for i in 1..=900u64 {
            if let Say::Count(_) = r.see(B, t + Duration::from_millis(33 * i)) {
                counts += 1;
            }
        }
        assert_eq!(counts, 2, "about 30 seconds of repeats is reported twice, not 900 times");
    }
}
