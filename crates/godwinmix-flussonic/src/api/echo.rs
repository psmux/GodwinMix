//! A stream's config as Flussonic's API echoes it: `name`, `title`,
//! `inputs[].url`, and `transcoder` with `global` and `tracks`, which is
//! what the BTN36 channel wall's `transcoder_settings` reads.

use crate::model::{Bitrate, Deinterlace, Hw, SizeMode, Stream, Transcoder};
use serde_json::{json, Map, Value};

fn bitrate(b: Bitrate) -> Value {
    match b {
        Bitrate::Copy => json!("copy"),
        Bitrate::Bps(bps) => json!(bps),
    }
}

fn hw(h: Hw) -> &'static str {
    match h {
        Hw::Cpu => "cpu",
        Hw::Nvenc => "nvenc",
        Hw::Qsv => "qsv",
        Hw::Other => "other",
    }
}

pub fn transcoder(t: &Transcoder) -> Value {
    let mut global = Map::new();
    global.insert("hw".into(), json!(hw(t.hw)));
    global.insert("deviceid".into(), json!(t.device_id));
    if let Some(g) = t.gop {
        global.insert("gop".into(), json!(g));
    }
    global.insert("deinterlace".into(), json!(!matches!(t.deinterlace, Deinterlace::Off)));
    let mut tracks = Vec::new();
    for v in &t.video {
        let mut track = json!({"content": "video", "codec": v.codec, "bitrate": bitrate(v.bitrate)});
        if let Some(s) = v.size {
            let strategy = match s.mode {
                SizeMode::Fit => "fit",
                SizeMode::Crop => "crop",
                SizeMode::Scale => "scale",
            };
            track["size"] = json!({"width": s.width, "height": s.height, "strategy": strategy});
            if let Some(rgb) = s.pad {
                track["size"]["background"] = json!(format!("#{rgb:06X}"));
            }
        }
        for (k, val) in [("preset", v.preset.clone()), ("profile", v.profile.clone()), ("level", v.level.clone())] {
            if let Some(val) = val {
                track[k] = json!(val);
            }
        }
        if let Some(b) = v.bframes {
            track["bframes"] = json!(b);
        }
        tracks.push(track);
    }
    for a in &t.audio {
        let mut track = json!({"content": "audio", "codec": a.codec, "bitrate": bitrate(a.bitrate)});
        if let Some(ch) = a.channels {
            track["channels"] = json!(ch);
        }
        if let Some(r) = a.sample_rate {
            track["sample_rate"] = json!(r);
        }
        tracks.push(track);
    }
    json!({"global": global, "tracks": tracks})
}

/// The config half of one stream's answer.
pub fn stream(s: &Stream) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("name".into(), json!(s.name));
    if let Some(t) = &s.title {
        m.insert("title".into(), json!(t));
    }
    m.insert("disabled".into(), json!(s.disabled));
    m.insert("static".into(), json!(s.is_static));
    m.insert("inputs".into(), Value::Array(s.inputs.iter().map(|i| json!({"url": i.url})).collect()));
    if let Some(t) = &s.transcoder {
        m.insert("transcoder".into(), transcoder(t));
    }
    if !s.pushes.is_empty() {
        m.insert("pushes".into(), Value::Array(s.pushes.iter().map(|p| json!({"url": p})).collect()));
    }
    m
}
