//! The multivariant playlist, and adding a query to every URI a playlist
//! hands out.

use super::playlist::Variant;
use std::fmt::Write;

/// The multivariant playlist: one `EXT-X-STREAM-INF` per video rung, and
/// every sound track as an `EXT-X-MEDIA` of the one group every rung
/// points at, the first the default.
pub fn master(video: &[Variant], audio: &[Variant]) -> String {
    let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-INDEPENDENT-SEGMENTS\n");
    if let (true, Some(a)) = (video.is_empty(), audio.first()) {
        // Audio only: the first sound is the one variant, the rest its group.
        audio_group(&mut out, &audio[1..], |a| Some(format!("{}/index.m3u8", a.id)));
        let _ = write!(out, "#EXT-X-STREAM-INF:BANDWIDTH={}", a.bandwidth);
        if !a.info.codecs.is_empty() {
            let _ = write!(out, ",CODECS=\"{}\"", a.info.codecs);
        }
        if audio.len() > 1 {
            out.push_str(",AUDIO=\"aud\"");
        }
        let _ = writeln!(out, "\n{}/index.m3u8", a.id);
        return out;
    }
    audio_group(&mut out, audio, |a| Some(format!("{}/index.m3u8", a.id)));
    for v in video {
        stream_inf(&mut out, v, audio, &format!("{}/index.m3u8", v.id));
    }
    out
}

/// One `EXT-X-STREAM-INF` and its URI. `BANDWIDTH` counts the largest
/// sound track, as Apple asks, and `CODECS` every codec a player may meet.
pub fn stream_inf(out: &mut String, v: &Variant, audio: &[Variant], uri: &str) {
    let peak = audio.iter().map(|a| a.bandwidth).max().unwrap_or(0);
    let _ = write!(out, "#EXT-X-STREAM-INF:BANDWIDTH={}", v.bandwidth + peak);
    if let Some(avg) = v.average {
        let _ = write!(out, ",AVERAGE-BANDWIDTH={}", avg + audio.iter().filter_map(|a| a.average).max().unwrap_or(0));
    }
    let mut codecs: Vec<&str> = vec![v.info.codecs.as_str()];
    for a in audio {
        if !codecs.contains(&a.info.codecs.as_str()) {
            codecs.push(&a.info.codecs);
        }
    }
    codecs.retain(|c| !c.is_empty());
    if !codecs.is_empty() {
        let _ = write!(out, ",CODECS=\"{}\"", codecs.join(","));
    }
    if v.info.width > 0 && v.info.height > 0 {
        let _ = write!(out, ",RESOLUTION={}x{}", v.info.width, v.info.height);
    }
    if let Some((n, d)) = v.info.fps.filter(|(n, d)| *n > 0 && *d > 0) {
        let _ = write!(out, ",FRAME-RATE={:.3}", f64::from(n) / f64::from(d));
    }
    if !audio.is_empty() {
        out.push_str(",AUDIO=\"aud\"");
    }
    let _ = writeln!(out, "\n{uri}");
}

/// The sound tracks as one group, `aud`, in track order: `NAME` and
/// `LANGUAGE` from the PMT, the first `DEFAULT=YES`. A track whose `uri` is
/// None is carried in the variant itself, as a muxed MPEG-TS main is.
pub fn audio_group(out: &mut String, audio: &[Variant], uri: impl Fn(&Variant) -> Option<String>) {
    let mut names: Vec<String> = Vec::new();
    for (i, a) in audio.iter().enumerate() {
        let mut name = if a.info.name.is_empty() { super::lang::name(&a.info.lang, a.info.sound) } else { a.info.name.clone() };
        // NAME must be unique in a group: two English tracks are English and English 2.
        let base = name.clone();
        let mut n = 2;
        while names.contains(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        let default = if i == 0 { "YES" } else { "NO" };
        let _ = write!(out, "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"{}\",DEFAULT={default},AUTOSELECT=YES", name.replace('"', "'"));
        if let Some(tag) = super::lang::tag(&a.info.lang) {
            let _ = write!(out, ",LANGUAGE=\"{tag}\"");
        }
        if a.info.channels > 0 {
            let _ = write!(out, ",CHANNELS=\"{}\"", a.info.channels);
        }
        if let Some(uri) = uri(a) {
            let _ = write!(out, ",URI=\"{uri}\"");
        }
        out.push('\n');
        names.push(name);
    }
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
