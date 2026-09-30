//! The multivariant playlist, and adding a query to every URI a playlist
//! hands out.

use super::playlist::Variant;
use std::fmt::Write;

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
