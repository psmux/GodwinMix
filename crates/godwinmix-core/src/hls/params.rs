//! `params` on an `hls/output`: how long a segment is, how long a part is,
//! and how much of the past is kept.

use crate::config::Params;
use anyhow::{bail, Result};

/// What one HLS output was asked for, with the defaults filled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HlsParams {
    /// Target segment length. Segments are cut at the first keyframe at or
    /// after it, so an encoder with a keyframe every `segment_ms` gets
    /// segments of exactly this length.
    pub segment_ms: u32,
    /// LL-HLS part length. 0 is plain HLS: segments only.
    pub part_ms: u32,
    /// Seconds of the past each rung keeps in memory and lists.
    pub window_s: u32,
}

impl Default for HlsParams {
    fn default() -> Self {
        HlsParams { segment_ms: 2000, part_ms: 0, window_s: 30 }
    }
}

impl HlsParams {
    /// The part length LL-HLS gets when `low_latency = true` names no other.
    pub const PART_MS: u32 = 333;

    /// Read `segment_ms`, `part_ms`, `low_latency` and `window` from an
    /// output's params. Low latency is on when `part_ms` is given or
    /// `low_latency = true`, and plain HLS otherwise. Anything else in the
    /// table is left for the output's other readers.
    pub fn from_params(params: &Params) -> Result<HlsParams> {
        let mut out = HlsParams::default();
        match params.get("low_latency") {
            None => {}
            Some(toml::Value::Boolean(on)) => out.part_ms = if *on { Self::PART_MS } else { 0 },
            Some(v) => bail!("hls/output params.low_latency must be true or false, not `{v}`"),
        }
        if let Some(v) = number(params, "segment_ms")? {
            out.segment_ms = v;
        }
        if let Some(v) = number(params, "part_ms")? {
            out.part_ms = v;
        }
        if let Some(v) = number(params, "window")? {
            out.window_s = v;
        }
        out.check()?;
        Ok(out)
    }

    fn check(&self) -> Result<()> {
        if !(500..=10_000).contains(&self.segment_ms) {
            bail!(
                "hls/output params.segment_ms must be 500 to 10000, not {}. 2000 is the usual.",
                self.segment_ms
            );
        }
        if self.part_ms != 0 && !(100..=self.segment_ms / 2).contains(&self.part_ms) {
            bail!(
                "hls/output params.part_ms must be 0 (no low latency) or 100 to {} (half a \
                 segment), not {}. 333 is the usual.",
                self.segment_ms / 2,
                self.part_ms
            );
        }
        let least = (self.segment_ms * 3).div_ceil(1000);
        if self.window_s < least || self.window_s > 600 {
            bail!(
                "hls/output params.window must be {least} to 600 seconds (at least three \
                 segments), not {}",
                self.window_s
            );
        }
        Ok(())
    }

    pub fn low_latency(&self) -> bool {
        self.part_ms > 0
    }

    /// Segments one rung keeps: the window, and two more so a player that
    /// read the playlist just before a segment left can still fetch it.
    pub fn ring_capacity(&self) -> usize {
        self.listed() + 2
    }

    /// Segments listed in a media playlist.
    pub fn listed(&self) -> usize {
        (self.window_s as usize * 1000).div_ceil(self.segment_ms as usize)
    }

    pub fn segment_ns(&self) -> u64 {
        u64::from(self.segment_ms) * 1_000_000
    }

    /// `fragment-duration` and `chunk-duration` for `cmafmux`.
    ///
    /// The muxer ends a fragment only on a chunk boundary, and only once the
    /// boundary has reached the fragment's length. Six chunks of 333333333 ns
    /// are 1999999998 ns, two short of two seconds, so a fragment asked for as
    /// 2 s with 333 ms parts misses the keyframe at 2 s and runs on to the
    /// next boundary that has one: 2.67 s at 30 fps, measured. So the chunk is
    /// the segment divided by a whole number of parts, rounded down, and the
    /// fragment is exactly that many chunks.
    pub fn mux_durations(&self) -> (u64, Option<u64>) {
        if !self.low_latency() {
            return (self.segment_ns(), None);
        }
        let parts = u64::from((self.segment_ms + self.part_ms / 2) / self.part_ms).max(1);
        let chunk = self.segment_ns() / parts;
        (chunk * parts, Some(chunk))
    }
}

fn number(params: &Params, key: &str) -> Result<Option<u32>> {
    let Some(v) = params.get(key) else { return Ok(None) };
    let n = v
        .as_integer()
        .or_else(|| v.as_float().map(|f| f.round() as i64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()));
    match n {
        Some(n) if (0..=i64::from(u32::MAX)).contains(&n) => Ok(Some(n as u32)),
        _ => bail!("hls/output params.{key} must be a whole number, not `{v}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(text: &str) -> Params {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn defaults_are_two_second_segments_of_plain_hls() {
        let p = HlsParams::from_params(&Params::new()).unwrap();
        assert_eq!(p, HlsParams::default());
        assert!(!p.low_latency());
        assert_eq!(p.ring_capacity(), 17);
        assert_eq!(p.listed(), 15);
    }

    #[test]
    fn refusals_name_the_range() {
        let e = HlsParams::from_params(&params("segment_ms = 100")).unwrap_err().to_string();
        assert!(e.contains("500 to 10000"), "{e}");
        let e = HlsParams::from_params(&params("part_ms = 1500")).unwrap_err().to_string();
        assert!(e.contains("100 to 1000"), "{e}");
        let e = HlsParams::from_params(&params("window = 2")).unwrap_err().to_string();
        assert!(e.contains("6 to 600"), "{e}");
        let e = HlsParams::from_params(&params("window = \"lots\"")).unwrap_err().to_string();
        assert!(e.contains("whole number"), "{e}");
    }

    #[test]
    fn a_fragment_is_a_whole_number_of_chunks() {
        let p = HlsParams { part_ms: 333, ..HlsParams::default() };
        assert_eq!(p.mux_durations(), (1_999_999_998, Some(333_333_333)));
        let p = HlsParams { segment_ms: 1000, part_ms: 250, window_s: 6 };
        assert_eq!(p.mux_durations(), (1_000_000_000, Some(250_000_000)));
        let p = HlsParams { segment_ms: 1000, part_ms: 200, window_s: 6 };
        assert_eq!(p.mux_durations(), (1_000_000_000, Some(200_000_000)));
        assert_eq!(HlsParams::default().mux_durations(), (2_000_000_000, None));
    }

    #[test]
    fn low_latency_is_a_part_length_or_a_switch() {
        let p = HlsParams::from_params(&params("low_latency = true")).unwrap();
        assert_eq!(p.part_ms, 333);
        let p = HlsParams::from_params(&params("segment_ms = 1000\npart_ms = 200")).unwrap();
        assert!(p.low_latency());
        let p = HlsParams::from_params(&params("low_latency = true\npart_ms = 0\nwindow = 60")).unwrap();
        assert!(!p.low_latency(), "an explicit 0 wins");
        assert_eq!(p.window_s, 60);
        assert!(HlsParams::from_params(&params("low_latency = 1")).is_err());
    }
}
