//! Times in nanoseconds, as GStreamer counts them, so a time made here
//! rounds to the same millisecond a time made by its parsers does.

/// Nanoseconds.
pub type Ns = u64;

/// A 90 kHz MPEG time in nanoseconds, rounded down as tsdemux does.
pub fn ns_of_90k(t: u64) -> Ns {
    (u128::from(t) * 100_000 / 9) as Ns
}

/// How long `samples` last at `rate`, rounded down as `baseparse` does.
pub fn samples_ns(samples: u32, rate: u32) -> Ns {
    if rate == 0 {
        return 0;
    }
    u64::from(samples) * 1_000_000_000 / u64::from(rate)
}

/// Frame times across one PES packet: the first frame at the packet's
/// time, each later one a frame's length after the one before, as a parser
/// fills in the frames its input buffer did not stamp.
#[derive(Debug, Default)]
pub struct Clock {
    next: Option<Ns>,
}

impl Clock {
    /// The time of the next frame, given the packet's time if this frame
    /// starts it, and how long the frame lasts.
    pub fn frame(&mut self, stamped: Option<Ns>, lasts: Ns) -> Option<Ns> {
        let at = stamped.or(self.next)?;
        self.next = Some(at + lasts);
        Some(at)
    }
}
