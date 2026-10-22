//! Flussonic's URL shapes under `/<name>/`, read into what to answer.
//!
//! Segments are `tracks-<sel>/YYYY/MM/DD/HH/MM/SS-<duration ms>.ts` in UTC,
//! so a name says when its segment began and is never handed out twice,
//! not even across a restart: the caches in front key on the path.

use super::access::Select;
use crate::clock::{self, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// `index.m3u8`, `playlist.m3u8`.
    Master,
    /// `video.m3u8`, `mono.m3u8` (`None`: the default selection, with the
    /// segment URIs under its `tracks-` folder), or `tracks-<sel>/mono.m3u8`.
    Media(Option<Select>),
    /// A segment of `sel` that began in unix second `start_s`.
    Segment { sel: Select, start_s: i64, ext: String },
    /// `tracks-<sel>/init-<id>.mp4`, for CMAF.
    Init { sel: Select, id: String },
    Preview,
    Embed,
    MpegTs,
}

/// `v1a1`, `v1`, `a2`.
pub fn select(s: &str) -> Option<Select> {
    let (video, rest) = match s.strip_prefix("v1") {
        Some(r) => (true, r),
        None => (false, s),
    };
    let audio = match rest {
        "" => None,
        a => Some(a.strip_prefix('a')?.parse::<u8>().ok().filter(|n| *n >= 1)?),
    };
    (video || audio.is_some()).then_some(Select { video, audio })
}

pub fn select_text(sel: Select) -> String {
    let mut s = String::new();
    if sel.video {
        s.push_str("v1");
    }
    if let Some(a) = sel.audio {
        s.push_str(&format!("a{a}"));
    }
    s
}

/// The name of a segment that began at `start_ms` and lasts `duration_ms`.
pub fn segment_name(start_ms: i64, duration_ms: u32, ext: &str) -> String {
    let t = clock::utc(start_ms);
    format!("{:04}/{:02}/{:02}/{:02}/{:02}/{:02}-{:05}.{ext}", t.year, t.month, t.day, t.hour, t.minute, t.second, duration_ms)
}

fn segment(rest: &str) -> Option<(i64, String)> {
    let parts: Vec<&str> = rest.split('/').collect();
    let [y, mo, d, h, mi, last] = parts.as_slice() else { return None };
    let (s, tail) = last.split_once('-')?;
    let (ms, ext) = tail.split_once('.')?;
    ms.parse::<u32>().ok()?;
    let n = |v: &str| v.parse::<u32>().ok();
    let t = Utc { year: i64::from(n(y)?), month: n(mo)?, day: n(d)?, hour: n(h)?, minute: n(mi)?, second: n(s)?, milli: 0 };
    if !(1..=12).contains(&t.month) || !(1..=31).contains(&t.day) || t.hour > 23 || t.minute > 59 || t.second > 60 {
        return None;
    }
    Some((clock::unix_ms(&t) / 1000, ext.to_string()))
}

/// What `rest`, the path after `/<name>/`, asks for.
pub fn route(rest: &str) -> Option<Route> {
    match rest {
        "index.m3u8" | "playlist.m3u8" => return Some(Route::Master),
        "video.m3u8" | "mono.m3u8" => return Some(Route::Media(None)),
        "preview.jpg" => return Some(Route::Preview),
        "embed.html" => return Some(Route::Embed),
        "mpegts" => return Some(Route::MpegTs),
        _ => {}
    }
    let (folder, file) = rest.strip_prefix("tracks-")?.split_once('/')?;
    let sel = select(folder)?;
    match file {
        "mono.m3u8" | "index.m3u8" | "video.m3u8" => Some(Route::Media(Some(sel))),
        f if f.starts_with("init-") && f.ends_with(".mp4") => {
            let id = &f[5..f.len() - 4];
            id.chars().all(|c| c.is_ascii_alphanumeric()).then(|| Route::Init { sel, id: id.to_string() })
        }
        f => segment(f).map(|(start_s, ext)| Route::Segment { sel, start_s, ext }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flussonic_path_reads() {
        assert_eq!(route("index.m3u8"), Some(Route::Master));
        assert_eq!(route("mono.m3u8"), Some(Route::Media(None)));
        let v1a1 = Select { video: true, audio: Some(1) };
        assert_eq!(route("tracks-v1a1/mono.m3u8"), Some(Route::Media(Some(v1a1))));
        assert_eq!(route("tracks-a2/mono.m3u8"), Some(Route::Media(Some(Select { video: false, audio: Some(2) }))));
        assert_eq!(route("tracks-v1/mono.m3u8"), Some(Route::Media(Some(Select { video: true, audio: None }))));
        let name = segment_name(1_792_756_804_123, 6000, "ts");
        assert_eq!(name, "2026/10/23/12/00/04-06000.ts");
        assert_eq!(route(&format!("tracks-v1a1/{name}")), Some(Route::Segment { sel: v1a1, start_s: 1_792_756_804, ext: "ts".into() }));
        assert_eq!(route("tracks-v1/init-ab12.mp4"), Some(Route::Init { sel: Select { video: true, audio: None }, id: "ab12".into() }));
        for bad in ["tracks-x/mono.m3u8", "tracks-a0/mono.m3u8", "tracks-v1/2026/13/01/00/00/00-1.ts", "../streamer", "tracks-v1/init-../x.mp4"] {
            assert_eq!(route(bad), None, "{bad}");
        }
        assert_eq!(select_text(v1a1), "v1a1");
    }
}
