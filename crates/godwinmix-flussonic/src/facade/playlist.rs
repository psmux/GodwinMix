//! The playlists in Flussonic's shape. Every URI in them is relative, so a
//! signature in the path (`/s/<expires>/<sig>/<name>/index.m3u8`) covers
//! everything a player fetches after it.

use super::access::{Container, Kind, Select, Track, Tracks, Window};
use super::paths::{segment_name, select_text};
use crate::clock;
use std::fmt::Write;

fn sound_name(t: &Track, all: &[&Track]) -> String {
    let base = t.name.clone().or_else(|| t.lang.as_ref().map(|l| l.to_uppercase())).unwrap_or_else(|| format!("Sound {}", t.number));
    let twice = all.iter().filter(|o| o.number != t.number).any(|o| o.name.clone().or_else(|| o.lang.as_ref().map(|l| l.to_uppercase())).as_deref() == Some(base.as_str()));
    if twice {
        format!("{base} {}", t.number)
    } else {
        base
    }
}

/// The selection `video.m3u8` and `mono.m3u8` mean, and the master's variant.
pub fn default_select(t: &Tracks) -> Select {
    let has_video = t.tracks.iter().any(|x| x.kind == Kind::Video);
    let has_sound = t.tracks.iter().any(|x| x.kind == Kind::Audio);
    match (has_video, has_sound) {
        (true, true) if t.muxed => Select { video: true, audio: Some(1) },
        (true, _) => Select { video: true, audio: None },
        (false, _) => Select { video: false, audio: Some(1) },
    }
}

/// `index.m3u8`: every sound track an `EXT-X-MEDIA`, the first the default.
pub fn master(t: &Tracks) -> String {
    let video = t.tracks.iter().find(|x| x.kind == Kind::Video);
    let sounds: Vec<&Track> = t.tracks.iter().filter(|x| x.kind == Kind::Audio).collect();
    let main = default_select(t);
    let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-INDEPENDENT-SEGMENTS\n");
    let grouped = video.is_some() && !sounds.is_empty();
    if grouped {
        for s in &sounds {
            let first = s.number == 1;
            let name = sound_name(s, &sounds);
            let _ = write!(out, "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",NAME=\"{name}\"");
            if let Some(l) = &s.lang {
                let _ = write!(out, ",LANGUAGE=\"{l}\"");
            }
            let _ = write!(out, ",DEFAULT={},AUTOSELECT=YES", if first { "YES" } else { "NO" });
            if let Some(c) = s.channels.filter(|c| *c > 0) {
                let _ = write!(out, ",CHANNELS=\"{c}\"");
            }
            // The first sound rides in the variant when the segments mux it.
            if !(first && main.audio == Some(1)) {
                let _ = write!(out, ",URI=\"tracks-a{}/mono.m3u8\"", s.number);
            }
            out.push('\n');
        }
    }
    let first_sound = sounds.first();
    let bandwidth = video.map(|v| v.bandwidth).unwrap_or(0) + first_sound.map(|s| s.bandwidth).unwrap_or(0);
    let _ = write!(out, "#EXT-X-STREAM-INF:BANDWIDTH={}", bandwidth.max(1));
    let codecs: Vec<&str> = [video, first_sound.copied()].into_iter().flatten().map(|x| x.codecs.as_str()).filter(|c| !c.is_empty()).collect();
    if !codecs.is_empty() {
        let _ = write!(out, ",CODECS=\"{}\"", codecs.join(","));
    }
    if let Some(v) = video.filter(|v| v.width > 0 && v.height > 0) {
        let _ = write!(out, ",RESOLUTION={}x{}", v.width, v.height);
        if let Some(f) = v.fps.filter(|f| *f > 0.0) {
            let _ = write!(out, ",FRAME-RATE={f:.3}");
        }
    }
    if grouped {
        out.push_str(",AUDIO=\"audio\"");
    }
    let _ = writeln!(out, "\ntracks-{}/mono.m3u8", select_text(main));
    out
}

/// A media playlist. `prefix` is put before every URI: empty for a
/// playlist inside its `tracks-` folder, `tracks-v1a1/` for `mono.m3u8`.
pub fn media(w: &Window, prefix: &str) -> String {
    let (version, ext) = match w.container {
        Container::Ts => (3, "ts"),
        Container::Cmaf => (7, "m4s"),
    };
    let target = w.segments.iter().map(|s| s.duration_ms.div_ceil(1000)).max().unwrap_or(2).max(1);
    let mut out = format!("#EXTM3U\n#EXT-X-VERSION:{version}\n#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:{}\n#EXT-X-INDEPENDENT-SEGMENTS\n", w.sequence);
    let (mut init, mut last_second) = (None, None);
    for s in &w.segments {
        let second = s.start_ms.div_euclid(1000);
        // Two segments in one second would share a name; the later waits.
        if last_second.is_some_and(|l| second <= l) {
            continue;
        }
        last_second = Some(second);
        if s.discontinuity {
            out.push_str("#EXT-X-DISCONTINUITY\n");
        }
        if s.init.is_some() && s.init != init {
            init = s.init.clone();
            let _ = writeln!(out, "#EXT-X-MAP:URI=\"{prefix}init-{}.mp4\"", init.as_deref().unwrap_or_default());
        }
        let _ = writeln!(out, "#EXT-X-PROGRAM-DATE-TIME:{}", clock::rfc3339(s.start_ms));
        let _ = writeln!(out, "#EXTINF:{:.3},\n{prefix}{}", f64::from(s.duration_ms) / 1000.0, segment_name(s.start_ms, s.duration_ms, ext));
    }
    out
}

#[cfg(test)]
mod tests;
