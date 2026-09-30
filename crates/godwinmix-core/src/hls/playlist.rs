//! Writing the playlists: the multivariant one for a ladder, and one media
//! playlist per rung, with the LL-HLS tags when parts are on.
//!
//! Pure functions of a [`View`] and the params, so the tests compare them
//! against golden files. RFC 8216 and its second edition (8216bis) are the
//! references; the tag order follows Apple's LL-HLS examples, which is what
//! hls.js, Safari and ffmpeg are tested against.

use super::ring::{SegmentView, View};
use super::track::TrackInfo;
use super::HlsParams;
pub use super::master::{master, with_query};
use std::fmt::Write;

/// Another rung's position, for `EXT-X-RENDITION-REPORT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub id: String,
    pub last_msn: u64,
    pub last_part: Option<u32>,
}

/// One entry of the multivariant playlist.
#[derive(Debug, Clone)]
pub struct Variant {
    pub id: String,
    pub info: TrackInfo,
    /// Peak bit/s, what `BANDWIDTH` means.
    pub bandwidth: u64,
    pub average: Option<u64>,
}

pub fn init_uri(gen: u32) -> String {
    if gen == 0 { "init.mp4".into() } else { format!("init{gen}.mp4") }
}

fn secs(ns: u64) -> f64 {
    ns as f64 / 1e9
}

/// `EXT-X-TARGETDURATION`: the configured length rounded up, or the longest
/// segment there has been rounded to the nearest second if one ran over
/// (RFC 8216 4.3.3.1). A source whose keyframes wander makes it grow once
/// and then stay.
fn target_duration(view: &View, p: &HlsParams) -> u64 {
    let configured = u64::from(p.segment_ms).div_ceil(1000);
    (secs(view.longest_ns).round() as u64).max(configured)
}

/// `PART-TARGET` in seconds: the configured part, or the longest part seen
/// if the frame rate made them a little longer.
fn part_target(view: &View, p: &HlsParams) -> f64 {
    let longest = view.segments.iter().flat_map(|s| s.parts.iter().map(|(d, _)| *d)).max().unwrap_or(0);
    let ms = (longest.div_ceil(1_000_000)).max(u64::from(p.part_ms));
    ms as f64 / 1000.0
}

/// The media playlist for one rung.
pub fn media(view: &View, p: &HlsParams, reports: &[Report]) -> String {
    let mut out = String::with_capacity(2048);
    let target = target_duration(view, p);
    let ll = p.low_latency();
    let listed = listed(view, p);
    let first = listed.first();
    out.push_str("#EXTM3U\n");
    let _ = writeln!(out, "#EXT-X-VERSION:{}", if ll { 9 } else { 7 });
    let _ = writeln!(out, "#EXT-X-TARGETDURATION:{target}");
    if ll {
        let pt = part_target(view, p);
        let _ = writeln!(out, "#EXT-X-SERVER-CONTROL:CAN-BLOCK-RELOAD=YES,PART-HOLD-BACK={:.3}", pt * 3.0);
        let _ = writeln!(out, "#EXT-X-PART-INF:PART-TARGET={pt:.3}");
    }
    let _ = writeln!(out, "#EXT-X-MEDIA-SEQUENCE:{}", first.map(|s| s.msn).unwrap_or(0));
    if let Some(s) = first.filter(|s| s.init > 0) {
        let _ = writeln!(out, "#EXT-X-DISCONTINUITY-SEQUENCE:{}", s.init);
    }
    // Parts are listed for the last three target durations, which is what
    // 8216bis asks for and what a player joining at the live edge needs.
    let keep = (3 * target * 1000).div_ceil(u64::from(p.segment_ms)) as usize;
    let parts_from = listed.len().saturating_sub(keep);
    let mut init = None;
    for (i, s) in listed.iter().enumerate() {
        if init != Some(s.init) {
            if init.is_some() {
                out.push_str("#EXT-X-DISCONTINUITY\n");
            }
            let _ = writeln!(out, "#EXT-X-MAP:URI=\"{}\"", init_uri(s.init));
            init = Some(s.init);
        }
        segment(&mut out, s, ll && i >= parts_from);
    }
    if ll {
        let (msn, part) = next_part(listed.last().copied(), p);
        let _ = writeln!(out, "#EXT-X-PRELOAD-HINT:TYPE=PART,URI=\"{msn}.{part}.m4s\"");
        for r in reports {
            let _ = write!(out, "#EXT-X-RENDITION-REPORT:URI=\"../{}/index.m3u8\",LAST-MSN={}", r.id, r.last_msn);
            if let Some(part) = r.last_part {
                let _ = write!(out, ",LAST-PART={part}");
            }
            out.push('\n');
        }
    }
    out
}

/// The part a player should ask for next, for `EXT-X-PRELOAD-HINT`. An open
/// segment whose parts already fill a fragment ends at the next keyframe, so
/// the next part starts the next segment; hinting one more part of this one
/// names a file that never comes, and a player that preloads it gets a 404
/// and a blocking reload that answers without it.
fn next_part(last: Option<&SegmentView>, p: &HlsParams) -> (u64, usize) {
    let (fragment, chunk) = p.mux_durations();
    let full = |s: &SegmentView| {
        let held: u64 = s.parts.iter().map(|(d, _)| *d).sum();
        held + chunk.unwrap_or(0) / 2 >= fragment
    };
    match last {
        Some(s) if !s.complete && !full(s) => (s.msn, s.parts.len()),
        Some(s) => (s.msn + 1, 0),
        None => (0, 0),
    }
}

/// The segments a playlist lists: the window of whole ones, then the open
/// one when parts are on (its parts are all a player can have of it yet).
fn listed<'a>(view: &'a View, p: &HlsParams) -> Vec<&'a SegmentView> {
    let whole: Vec<&SegmentView> = view.segments.iter().filter(|s| s.complete).collect();
    let mut out: Vec<&SegmentView> = whole[whole.len().saturating_sub(p.listed())..].to_vec();
    if p.low_latency() {
        out.extend(view.segments.iter().filter(|s| !s.complete && !s.parts.is_empty()).take(1));
    }
    out
}

fn segment(out: &mut String, s: &SegmentView, with_parts: bool) {
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_millis(s.pdt_ms.max(0) as u64);
    let _ = writeln!(out, "#EXT-X-PROGRAM-DATE-TIME:{}", crate::observe::logs::rfc3339(&when));
    if with_parts {
        for (i, (d, independent)) in s.parts.iter().enumerate() {
            let _ = write!(out, "#EXT-X-PART:DURATION={:.5},URI=\"{}.{}.m4s\"", secs(*d), s.msn, i);
            out.push_str(if *independent { ",INDEPENDENT=YES\n" } else { "\n" });
        }
    }
    if s.complete {
        let _ = writeln!(out, "#EXTINF:{:.5},\n{}.m4s", secs(s.duration_ns), s.msn);
    }
}

#[cfg(test)]
#[path = "playlist_tests.rs"]
mod tests;
