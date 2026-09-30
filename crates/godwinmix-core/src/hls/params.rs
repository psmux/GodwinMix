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
    /// LL-HLS part length. 0 turns low latency off: segments only.
    pub part_ms: u32,
    /// Seconds of the past each rung keeps in memory and lists.
    pub window_s: u32,
}

impl Default for HlsParams {
    fn default() -> Self {
        HlsParams { segment_ms: 2000, part_ms: 333, window_s: 30 }
    }
}

impl HlsParams {
    /// Read `segment_ms`, `part_ms` and `window` from an output's params.
    /// Anything else in the table is left for the output's other readers.
    pub fn from_params(params: &Params) -> Result<HlsParams> {
        let mut out = HlsParams::default();
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
    fn defaults_are_two_second_segments_and_third_second_parts() {
        let p = HlsParams::from_params(&Params::new()).unwrap();
        assert_eq!(p, HlsParams::default());
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
    fn zero_part_turns_low_latency_off() {
        let p = HlsParams::from_params(&params("part_ms = 0\nwindow = 60")).unwrap();
        assert!(!p.low_latency());
        assert_eq!(p.window_s, 60);
    }
}
