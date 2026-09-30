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
        let (msn, part) = match listed.last() {
            Some(s) if !s.complete => (s.msn, s.parts.len()),
            Some(s) => (s.msn + 1, 0),
            None => (0, 0),
        };
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

/// The multivariant playlist: one `EXT-X-STREAM-INF` per video rung, and the
/// audio as one `EXT-X-MEDIA` every rung shares.
pub fn master(video: &[Variant], audio: Option<&Variant>) -> String {
    let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-INDEPENDENT-SEGMENTS\n");
    if let (true, Some(a)) = (video.is_empty(), audio) {
        // Audio only: the audio is the one variant.
        let _ = write!(out, "#EXT-X-STREAM-INF:BANDWIDTH={}", a.bandwidth);
        if !a.info.codecs.is_empty() {
            let _ = write!(out, ",CODECS=\"{}\"", a.info.codecs);
        }
        let _ = writeln!(out, "\n{}/index.m3u8", a.id);
        return out;
    }
    if let Some(a) = audio {
        let _ = write!(
            out,
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"{}\",DEFAULT=YES,AUTOSELECT=YES",
            a.id
        );
        if a.info.channels > 0 {
            let _ = write!(out, ",CHANNELS=\"{}\"", a.info.channels);
        }
        let _ = writeln!(out, ",URI=\"{}/index.m3u8\"", a.id);
    }
    for v in video {
        let extra = audio.map(|a| a.bandwidth).unwrap_or(0);
        let _ = write!(out, "#EXT-X-STREAM-INF:BANDWIDTH={}", v.bandwidth + extra);
        if let Some(avg) = v.average {
            let _ = write!(out, ",AVERAGE-BANDWIDTH={}", avg + audio.and_then(|a| a.average).unwrap_or(0));
        }
        let codecs: Vec<&str> = [Some(v.info.codecs.as_str()), audio.map(|a| a.info.codecs.as_str())]
            .into_iter()
            .flatten()
            .filter(|c| !c.is_empty())
            .collect();
        if !codecs.is_empty() {
            let _ = write!(out, ",CODECS=\"{}\"", codecs.join(","));
        }
        if v.info.width > 0 && v.info.height > 0 {
            let _ = write!(out, ",RESOLUTION={}x{}", v.info.width, v.info.height);
        }
        if let Some((n, d)) = v.info.fps.filter(|(n, d)| *n > 0 && *d > 0) {
            let _ = write!(out, ",FRAME-RATE={:.3}", f64::from(n) / f64::from(d));
        }
        if audio.is_some() {
            out.push_str(",AUDIO=\"aud\"");
        }
        let _ = writeln!(out, "\n{}/index.m3u8", v.id);
    }
    out
}

/// The same playlist with `query` added to every URI in it, so a player that
/// was handed `?token=` keeps presenting it for every file it fetches.
pub fn with_query(text: &str, query: &str) -> String {
    if query.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 64 * query.len());
    for line in text.lines().filter(|l| !l.is_empty()) {
        if !line.starts_with('#') {
            out.push_str(line);
            out.push(if line.contains('?') { '&' } else { '?' });
            out.push_str(query);
        } else if let Some(at) = line.find("URI=\"") {
            let start = at + 5;
            let end = line[start..].find('"').map(|e| start + e).unwrap_or(line.len());
            out.push_str(&line[..end]);
            out.push(if line[start..end].contains('?') { '&' } else { '?' });
            out.push_str(query);
            out.push_str(&line[end..]);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "playlist_tests.rs"]
mod tests;
