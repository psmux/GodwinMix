//! What an output's MPEG-TS playlists say, as data a caller names files
//! from, and the playlists written from it.
//!
//! The listing crosses from the HLS packager to the station as JSON, so
//! the station's own routes and the Flussonic facade can each write the
//! playlists with their own file names: `{start_ms}.ts` here, and
//! `YYYY/MM/DD/HH/MM/SS-<ms>.ts` there ([`clock_name`]). Both are made from
//! the wall clock a segment starts at, so a name is never used twice for an
//! output, not across a restart either.

use super::super::playlist::{self, Variant};
use super::super::HlsParams;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

/// One whole segment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The same number as the CMAF segment of the same moment.
    pub msn: u64,
    /// Discontinuity sequence: breaks in the feed before it.
    pub disc: u32,
    /// Wall clock UTC at its first frame, ms since 1970.
    pub start_ms: i64,
    pub duration_ms: u32,
    pub bytes: usize,
}

/// One rendition's media playlist, as data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    pub target_s: u64,
    pub segments: Vec<Entry>,
}

/// The multivariant playlist, as data: the picture with the first sound
/// muxed in, and every sound track (the first carried by `main`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Master {
    /// `main`, with its measured TS bandwidth. None for sound alone.
    pub main: Option<Variant>,
    /// Every sound track in track order. The first is in `main` when there
    /// is one; each other is its own rendition, its id the rung to ask for.
    pub sounds: Vec<Variant>,
}

/// The listing for `entries`: the last window of them.
pub fn media(entries: &[Entry], longest_ns: u64, p: &HlsParams) -> Media {
    let from = entries.len().saturating_sub(p.listed());
    let configured = u64::from(p.segment_ms).div_ceil(1000);
    let target_s = ((longest_ns as f64 / 1e9).round() as u64).max(configured);
    Media { target_s, segments: entries[from..].to_vec() }
}

impl Media {
    /// The media playlist, each segment's URI from `uri`.
    pub fn render(&self, uri: impl Fn(&Entry) -> String) -> String {
        let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");
        let _ = writeln!(out, "#EXT-X-TARGETDURATION:{}", self.target_s);
        let first = self.segments.first();
        let _ = writeln!(out, "#EXT-X-MEDIA-SEQUENCE:{}", first.map_or(0, |s| s.msn));
        if let Some(s) = first.filter(|s| s.disc > 0) {
            let _ = writeln!(out, "#EXT-X-DISCONTINUITY-SEQUENCE:{}", s.disc);
        }
        let mut disc = first.map(|s| s.disc);
        for s in &self.segments {
            if disc != Some(s.disc) {
                out.push_str("#EXT-X-DISCONTINUITY\n");
                disc = Some(s.disc);
            }
            let when = std::time::UNIX_EPOCH + std::time::Duration::from_millis(s.start_ms.max(0) as u64);
            let _ = writeln!(out, "#EXT-X-PROGRAM-DATE-TIME:{}", crate::observe::logs::rfc3339(&when));
            let _ = writeln!(out, "#EXTINF:{:.3},\n{}", f64::from(s.duration_ms) / 1000.0, uri(s));
        }
        out
    }
}

impl Master {
    /// The multivariant playlist. `uri(None)` is `main`'s media playlist,
    /// `uri(Some(id))` a further sound's.
    pub fn render(&self, uri: impl Fn(Option<&str>) -> String) -> String {
        let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:4\n#EXT-X-INDEPENDENT-SEGMENTS\n");
        let Some(main) = &self.main else {
            let Some(first) = self.sounds.first() else { return out };
            let rest = &self.sounds[1..];
            playlist::audio_group(&mut out, rest, |a| Some(uri(Some(&a.id))));
            let _ = write!(out, "#EXT-X-STREAM-INF:BANDWIDTH={},CODECS=\"{}\"", first.bandwidth.max(1), first.info.codecs);
            if !rest.is_empty() {
                out.push_str(",AUDIO=\"aud\"");
            }
            let _ = writeln!(out, "\n{}", uri(Some(&first.id)));
            return out;
        };
        // The first sound is in `main` itself: no URI, and no bandwidth of
        // its own on top of `main`'s, which already counts it.
        playlist::audio_group(&mut out, &self.sounds, |a| (a.info.sound > 0).then(|| uri(Some(&a.id))));
        let mut alone = self.sounds.clone();
        if let Some(first) = alone.first_mut() {
            first.bandwidth = 0;
            first.average = None;
        }
        playlist::stream_inf(&mut out, main, &alone, &uri(None));
        out
    }
}

/// Flussonic's name for a segment: its UTC start to the second, and its
/// length in ms. `2026/10/23/08/04/12-2000.ts`.
pub fn clock_name(e: &Entry) -> String {
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_millis(e.start_ms.max(0) as u64);
    let t = crate::observe::logs::rfc3339(&when);
    format!("{}/{}/{}/{}/{}/{}-{}.ts", &t[0..4], &t[5..7], &t[8..10], &t[11..13], &t[14..16], &t[17..19], e.duration_ms)
}

/// A clock name read back: its start's second since 1970 and its length.
pub fn parse_clock(name: &str) -> Option<(i64, u32)> {
    let stem = name.strip_suffix(".ts")?;
    let (when, ms) = stem.rsplit_once('-')?;
    let f: Vec<i64> = when.split('/').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let [y, mo, d, h, mi, s] = f[..] else { return None };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    Some((days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s, ms.parse().ok()?))
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
