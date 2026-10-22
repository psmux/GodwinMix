//! Reading the playlists a direct show's HLS output serves today (CMAF,
//! `godwinmix_core::hls`), for the bridge.

use godwinmix_flussonic::clock::parse_rfc3339;
use godwinmix_flussonic::facade::access::{Kind, Track};

/// One attribute of an `#EXT-X-...:A=B,C="D"` line.
pub fn attr(line: &str, key: &str) -> Option<String> {
    let body = line.split_once(':')?.1;
    let mut rest = body;
    while !rest.is_empty() {
        let (k, after) = rest.split_once('=')?;
        let (value, next) = match after.strip_prefix('"') {
            Some(q) => {
                let end = q.find('"')?;
                (&q[..end], q[end + 1..].trim_start_matches(','))
            }
            None => match after.split_once(',') {
                Some((v, n)) => (v, n),
                None => (after, ""),
            },
        };
        if k.trim() == key {
            return Some(value.to_string());
        }
        rest = next;
    }
    None
}

/// The master: the video rung and the sound, each with the URI of its own
/// playlist, relative to the master.
pub fn master(text: &str) -> Vec<(Track, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    let mut sounds = 0u8;
    while let Some(line) = lines.next() {
        if line.starts_with("#EXT-X-MEDIA:") && attr(line, "TYPE").as_deref() == Some("AUDIO") {
            let Some(uri) = attr(line, "URI") else { continue };
            sounds += 1;
            let track = Track {
                kind: Kind::Audio,
                number: sounds,
                codecs: String::new(),
                bandwidth: 0,
                width: 0,
                height: 0,
                fps: None,
                channels: attr(line, "CHANNELS").and_then(|c| c.parse().ok()),
                lang: attr(line, "LANGUAGE"),
                name: None,
            };
            out.push((track, uri));
        } else if line.starts_with("#EXT-X-STREAM-INF:") && !out.iter().any(|(t, _): &(Track, String)| t.kind == Kind::Video) {
            let Some(uri) = lines.next().map(str::trim).filter(|u| !u.is_empty() && !u.starts_with('#')) else { continue };
            let codecs = attr(line, "CODECS").unwrap_or_default();
            let (w, h) = attr(line, "RESOLUTION").and_then(|r| r.split_once('x').map(|(w, h)| (w.parse().unwrap_or(0), h.parse().unwrap_or(0)))).unwrap_or((0, 0));
            let video_codec = codecs.split(',').find(|c| !c.starts_with("mp4a") && !c.starts_with("ac-3") && !c.starts_with("ec-3")).unwrap_or_default();
            let audio_codec = codecs.split(',').find(|c| c.starts_with("mp4a") || c.starts_with("ac-3") || c.starts_with("ec-3")).unwrap_or("mp4a.40.2");
            for (t, _) in out.iter_mut().filter(|(t, _)| t.kind == Kind::Audio) {
                t.codecs = audio_codec.to_string();
            }
            let track = Track {
                kind: Kind::Video,
                number: 1,
                codecs: video_codec.to_string(),
                bandwidth: attr(line, "BANDWIDTH").and_then(|b| b.parse().ok()).unwrap_or(0),
                width: w,
                height: h,
                fps: attr(line, "FRAME-RATE").and_then(|f| f.parse().ok()),
                channels: None,
                lang: None,
                name: None,
            };
            out.insert(0, (track, uri.to_string()));
        }
    }
    out
}

/// One whole segment of a media playlist, with its URI and its init's.
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    pub start_ms: i64,
    pub duration_ms: u32,
    pub uri: String,
    pub init: Option<String>,
    pub discontinuity: bool,
}

/// A media playlist's sequence and whole segments. Parts and preload
/// hints are LL-HLS's and left out: a Flussonic player wants segments.
pub fn media(text: &str) -> (u64, Vec<Seg>) {
    let (mut sequence, mut segs) = (0, Vec::new());
    let (mut init, mut when, mut extinf, mut disc) = (None, None, None, false);
    for line in text.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            sequence = v.parse().unwrap_or(0);
        } else if line.starts_with("#EXT-X-MAP:") {
            init = attr(line, "URI");
        } else if let Some(v) = line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
            when = parse_rfc3339(v);
        } else if let Some(v) = line.strip_prefix("#EXTINF:") {
            extinf = v.split(',').next().and_then(|d| d.parse::<f64>().ok());
        } else if line == "#EXT-X-DISCONTINUITY" {
            disc = true;
        } else if !line.is_empty() && !line.starts_with('#') {
            if let (Some(start_ms), Some(d)) = (when, extinf) {
                segs.push(Seg { start_ms, duration_ms: (d * 1000.0).round() as u32, uri: line.to_string(), init: init.clone(), discontinuity: disc });
                when = Some(start_ms + (d * 1000.0).round() as i64);
            }
            extinf = None;
            disc = false;
        }
    }
    (sequence, segs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_cmaf_playlists_the_packager_writes() {
        let master = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"audio\",DEFAULT=YES,AUTOSELECT=YES,CHANNELS=\"2\",URI=\"audio/index.m3u8\"\n\
            #EXT-X-STREAM-INF:BANDWIDTH=1628000,CODECS=\"avc1.64001f,mp4a.40.2\",RESOLUTION=1280x720,FRAME-RATE=29.970,AUDIO=\"aud\"\nmain/index.m3u8\n";
        let t = super::master(master);
        assert_eq!(t[0].0.kind, Kind::Video);
        assert_eq!((t[0].0.width, t[0].0.codecs.as_str(), t[0].1.as_str()), (1280, "avc1.64001f", "main/index.m3u8"));
        assert_eq!((t[1].0.channels, t[1].0.codecs.as_str(), t[1].1.as_str()), (Some(2), "mp4a.40.2", "audio/index.m3u8"));
        let text = include_str!("../../../godwinmix-core/src/hls/golden/media.m3u8");
        let (_, segs) = media(text);
        assert!(segs.len() >= 2 && segs[0].init.as_deref() == Some("init.mp4"));
        assert_eq!(segs[1].start_ms - segs[0].start_ms, i64::from(segs[0].duration_ms));
    }
}
