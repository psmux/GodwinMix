//! The same segments as DASH: a dynamic MPD with a `SegmentTimeline` per
//! representation, pointing at the CMAF files the HLS playlists already
//! name. Nothing is packaged twice; this is one more way to list the ring.
//!
//! Times come from the segments themselves: each segment's `tfdt` is its `t`,
//! the gap to the next one its `d`, and `availabilityStartTime` is the wall
//! clock of the first segment less its media time. Only whole segments are
//! listed, and only those decoded by the newest init segment.

use super::playlist::init_uri;
use super::ring::{SegmentView, View};
use super::track::{TrackInfo, TrackKind};
use super::HlsParams;
use std::fmt::Write;

/// One track as the MPD sees it.
pub struct Rep<'a> {
    pub id: &'a str,
    pub kind: TrackKind,
    pub info: &'a TrackInfo,
    pub view: &'a View,
    pub bandwidth: u64,
}

fn iso(ms: i64) -> String {
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms.max(0) as u64);
    crate::observe::logs::rfc3339(&t)
}

fn secs(ms: u64) -> String {
    format!("PT{}.{:03}S", ms / 1000, ms % 1000)
}

/// The segments a representation lists: whole, stamped, on the newest init.
fn listed<'a>(view: &'a View, p: &HlsParams) -> Vec<&'a SegmentView> {
    let whole: Vec<&SegmentView> = view.segments.iter().filter(|s| s.complete && s.decode_time.is_some()).collect();
    let Some(init) = whole.last().map(|s| s.init) else { return Vec::new() };
    let same: Vec<&SegmentView> = whole.into_iter().filter(|s| s.init == init).collect();
    same[same.len().saturating_sub(p.listed())..].to_vec()
}

/// `<S t d r>` runs: `d` is the gap to the next segment's decode time, so
/// the timeline has no drift, and the last one's is its own length.
fn timeline(segs: &[&SegmentView], timescale: u32) -> String {
    let mut runs: Vec<(u64, u64, u32)> = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        let t = s.decode_time.unwrap_or(0);
        let d = match segs.get(i + 1).and_then(|n| n.decode_time) {
            Some(next) if next > t => next - t,
            _ => (s.duration_ns as u128 * u128::from(timescale) / 1_000_000_000) as u64,
        };
        match runs.last_mut() {
            Some((rt, rd, r)) if *rd == d && *rt + *rd * (u64::from(*r) + 1) == t => *r += 1,
            _ => runs.push((t, d, 0)),
        }
    }
    let mut out = String::new();
    for (t, d, r) in runs {
        let _ = write!(out, "<S t=\"{t}\" d=\"{d}\"");
        if r > 0 {
            let _ = write!(out, " r=\"{r}\"");
        }
        out.push_str("/>");
    }
    out
}

fn representation(out: &mut String, rep: &Rep<'_>, segs: &[&SegmentView], query: &str) {
    let i = rep.info;
    let _ = write!(out, "      <Representation id=\"{}\" bandwidth=\"{}\"", rep.id, rep.bandwidth.max(1));
    if !i.codecs.is_empty() {
        let _ = write!(out, " codecs=\"{}\"", i.codecs);
    }
    if rep.kind == TrackKind::Video && i.width > 0 {
        let _ = write!(out, " width=\"{}\" height=\"{}\"", i.width, i.height);
        if let Some((n, d)) = i.fps.filter(|(n, d)| *n > 0 && *d > 0) {
            let _ = write!(out, " frameRate=\"{n}/{d}\"");
        }
    }
    out.push_str(">\n");
    if rep.kind == TrackKind::Audio && i.channels > 0 {
        let _ = writeln!(
            out,
            "        <AudioChannelConfiguration schemeIdUri=\"urn:mpeg:dash:23003:3:audio_channel_configuration:2011\" value=\"{}\"/>",
            i.channels
        );
    }
    let q = if query.is_empty() { String::new() } else { format!("?{}", query.replace('&', "&amp;")) };
    let _ = writeln!(
        out,
        "        <SegmentTemplate timescale=\"{}\" initialization=\"{}/{}{q}\" media=\"{}/$Number${}\" startNumber=\"{}\">",
        i.timescale,
        rep.id,
        init_uri(segs[0].init),
        rep.id,
        format_args!(".m4s{q}"),
        segs[0].msn
    );
    let _ = writeln!(out, "          <SegmentTimeline>{}</SegmentTimeline>", timeline(segs, i.timescale));
    out.push_str("        </SegmentTemplate>\n      </Representation>\n");
}

/// The MPD for `reps`, or None until a video (or the audio, alone) has a
/// whole stamped segment and a timescale to put it on.
pub fn mpd(reps: &[Rep<'_>], p: &HlsParams, now_ms: i64, query: &str) -> Option<String> {
    let lists: Vec<(&Rep<'_>, Vec<&SegmentView>)> =
        reps.iter().filter(|r| r.info.timescale > 0).map(|r| (r, listed(r.view, p))).filter(|(_, s)| !s.is_empty()).collect();
    let (anchor, segs) = lists.iter().find(|(r, _)| r.kind == TrackKind::Video).or(lists.first())?;
    let first = segs[0];
    let media_ms = first.decode_time? as i128 * 1000 / i128::from(anchor.info.timescale);
    let ast = first.pdt_ms - media_ms as i64;
    let seg_ms = u64::from(p.segment_ms);
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<MPD xmlns=\"urn:mpeg:dash:schema:mpd:2011\" profiles=\"urn:mpeg:dash:profile:isoff-live:2011\" type=\"dynamic\" \
         availabilityStartTime=\"{}\" publishTime=\"{}\" minimumUpdatePeriod=\"{}\" minBufferTime=\"{}\" \
         timeShiftBufferDepth=\"{}\" suggestedPresentationDelay=\"{}\">",
        iso(ast),
        iso(now_ms),
        secs(seg_ms),
        secs(seg_ms),
        secs(u64::from(p.window_s) * 1000),
        secs(seg_ms * 3)
    );
    out.push_str("  <Period id=\"0\" start=\"PT0S\">\n");
    for (kind, mime) in [(TrackKind::Video, "video/mp4"), (TrackKind::Audio, "audio/mp4")] {
        let set: Vec<_> = lists.iter().filter(|(r, _)| r.kind == kind).collect();
        if set.is_empty() {
            continue;
        }
        let _ = writeln!(out, "    <AdaptationSet mimeType=\"{mime}\" segmentAlignment=\"true\" startWithSAP=\"1\">");
        for (rep, segs) in set {
            representation(&mut out, rep, segs, query);
        }
        out.push_str("    </AdaptationSet>\n");
    }
    out.push_str("  </Period>\n");
    let _ = writeln!(out, "  <UTCTiming schemeIdUri=\"urn:mpeg:dash:utc:direct:2014\" value=\"{}\"/>", iso(now_ms));
    out.push_str("</MPD>\n");
    Some(out)
}

#[cfg(test)]
#[path = "dash_tests.rs"]
mod tests;
