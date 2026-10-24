//! The channels kept here in Flussonic's config language: for
//! `headend.export`, and for the Flussonic API, which lists them beside the
//! followed file's streams. Importing the text again makes the same shows,
//! less what Flussonic has no word for, which the notes list.

use super::compile;
use super::store::Store;
use godwinmix_protocol::headend::{ChannelInput, HeadendChannel, HeadendExport, TranscodeProfile};
use godwinmix_protocol::rendition::{Deinterlace, Fit, VideoWant};
use std::fmt::Write;

fn input(i: &ChannelInput) -> String {
    let uri = i.uri.trim();
    let multicast = uri.strip_prefix("udp://@").filter(|_| i.interface.is_some() || i.program.is_some());
    let mut out = match (multicast, &i.interface) {
        (Some(rest), Some(card)) => format!("mpts-udp://{card}@{rest}"),
        (Some(rest), None) => format!("udp://{rest}"),
        _ => uri.to_string(),
    };
    if let Some(p) = i.program {
        out.push_str(if out.contains('?') { "&" } else { "?" });
        let _ = write!(out, "program={p}");
    }
    out
}

fn video(v: &VideoWant, line: &mut Vec<String>, notes: &mut Vec<String>, name: &str) {
    if let Some(c) = v.codec {
        line.push(format!("vcodec={}", format!("{c:?}").to_lowercase()));
    }
    if let Some(b) = v.bitrate_kbps {
        line.push(format!("vb={b}k"));
    }
    if v.width.is_some() || v.height.is_some() {
        let fit = match v.fit.unwrap_or_default() {
            Fit::Pad => format!(":fit:{}", v.pad_color.clone().unwrap_or_else(|| "#000000".into())),
            Fit::Crop => ":crop".into(),
            Fit::Stretch => String::new(),
        };
        line.push(format!("size={}x{}{fit}", v.width.unwrap_or(0), v.height.unwrap_or(0)));
    }
    match v.keyframe_frames {
        Some(g) => line.push(format!("gop={g}")),
        None if v.keyframe_ms.is_some() => notes.push(format!("stream {name}: a keyframe every {} ms has no Flussonic word; gop counts frames", v.keyframe_ms.unwrap_or(0))),
        None => {}
    }
    match v.deinterlace {
        Some(Deinterlace::On) => line.push("deinterlace=true".into()),
        Some(Deinterlace::Auto) => line.push("deinterlace=auto".into()),
        _ => {}
    }
    if let Some(p) = &v.preset {
        line.push(format!("preset={p}"));
    }
    if let Some(b) = v.bframes {
        line.push(format!("bframes={b}"));
    }
    if let Some(pick) = v.encoder {
        line.push(if pick == godwinmix_protocol::rendition::EncoderPick::Gpu { "hw=nvenc".into() } else { "hw=cpu".into() });
    }
}

fn transcoder(p: &TranscodeProfile, notes: &mut Vec<String>, name: &str) -> Option<String> {
    if p.is_copy() {
        return None;
    }
    let mut line = Vec::new();
    match &p.video {
        Some(v) => video(v, &mut line, notes, name),
        None => line.push("vb=copy".into()),
    }
    match &p.audio {
        Some(a) => {
            if let Some(c) = a.codec {
                line.push(format!("acodec={}", format!("{c:?}").to_lowercase()));
            }
            line.push(a.bitrate_kbps.map_or("ab=128k".into(), |b| format!("ab={b}k")));
        }
        None => line.push("ab=copy".into()),
    }
    Some(line.join(" "))
}

fn stream(c: &HeadendChannel, profiles: &[TranscodeProfile], notes: &mut Vec<String>) -> String {
    let mut s = format!("stream {} {{\n", c.name);
    if let Some(t) = &c.title {
        let _ = writeln!(s, "  title {t};");
    }
    if !c.enabled {
        s.push_str("  disabled true;\n");
    }
    let _ = writeln!(s, "  input {};", input(&c.input));
    if let Some(b) = &c.input.backup {
        let _ = writeln!(s, "  input {};", input(&b.as_input()));
    }
    if let Some(line) = compile::profile_of(c, profiles).ok().and_then(|p| transcoder(p, notes, &c.name)) {
        let _ = writeln!(s, "  transcoder {line};");
    }
    let _ = writeln!(s, "  segment_duration {};", f64::from(c.hls.segment_ms) / 1000.0);
    for o in &c.outputs {
        let _ = writeln!(s, "  push {};", o.uri);
        if o.copy {
            notes.push(format!("stream {}: push {} is a copy here; Flussonic pushes the transcoded stream", c.name, o.uri));
        }
    }
    s.push_str("}\n");
    s
}

pub fn text(s: &Store) -> HeadendExport {
    let profiles = compile::all_profiles(&s.profiles);
    let mut notes = Vec::new();
    let mut text = String::from("# The channels kept by a GodwinMix station, in Flussonic's config language.\n");
    for c in &s.channels {
        text.push_str(&stream(c, &profiles, &mut notes));
    }
    HeadendExport { text, notes }
}
