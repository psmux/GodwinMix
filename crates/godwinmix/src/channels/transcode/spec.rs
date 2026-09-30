//! A plan as the listener builds it: one list of nodes per stream, each
//! with the GStreamer element and the properties the catalogue gives it.
//!
//! The listener (the ingest plugin) has no catalogue and no planner. It is
//! handed exactly what to run, with every catalogue property already worked
//! out into the unit that element wants, and applies it defensively. A node
//! is identified by its plan id, so the listener keeps a running node whose
//! description has not changed and rebuilds only the ones that have.
//!
//! Every value is a string, a number or a boolean, never null: the table
//! travels to the plugin as TOML, which has no null.

use std::collections::BTreeMap;

use godwinmix_core::catalogue::apply::{cpu_count, Vars};
use godwinmix_core::catalogue::model::PropValue;
use godwinmix_protocol::rendition::{AudioShape, Fps, StreamInfo, VideoShape};
use godwinmix_render::container::{audio_slug, video_slug};
use godwinmix_render::{Node, NodeKind, Plan, Track};
use serde_json::{json, Map, Value};

use super::machine::Machine;

/// The nodes of `plan` that read `stream`, in start order, as the listener
/// builds them. Sources, copies and muxes are not built: a copy is the
/// stream's own tags and a mux is the restreamer's.
pub fn stream_nodes(plan: &Plan, stream: &str, info: &StreamInfo, machine: &Machine) -> Vec<Value> {
    plan.nodes.iter().filter_map(|n| node(n, stream, info, plan, machine)).collect()
}

fn node(n: &Node, stream: &str, info: &StreamInfo, plan: &Plan, machine: &Machine) -> Option<Value> {
    let input = n.inputs.first().cloned().unwrap_or_default();
    let mut v = match &n.kind {
        NodeKind::Decode { source, track } if source == stream => decode(*track, info, machine)?,
        NodeKind::Scale { source, width, height, fps } if source == stream => {
            json!({"kind": "scale", "input": input, "width": width, "height": height, "fps": fps_pair(*fps)})
        }
        NodeKind::Encode { source, shape, encoder } if source == stream => {
            let keyframe_ms = plan.keyframe_ms.get(source).copied().unwrap_or(shape.keyframe_ms);
            let mut v = video_encoder(&encoder.id, shape, keyframe_ms, machine)?;
            v["kind"] = json!("encode");
            v["input"] = json!(input);
            v
        }
        NodeKind::AudioConvert { source, channels, sample_rate } if source == stream => {
            json!({"kind": "aconvert", "input": input, "channels": channels, "sample_rate": sample_rate})
        }
        NodeKind::AudioEncode { source, shape } if source == stream => {
            let mut v = audio_encoder(shape, machine)?;
            v["kind"] = json!("aencode");
            v["input"] = json!(input);
            v
        }
        _ => return None,
    };
    v["id"] = json!(n.id);
    Some(v)
}

fn decode(track: Track, info: &StreamInfo, machine: &Machine) -> Option<Value> {
    match track {
        Track::Video => {
            let codec = info.video?.codec;
            let e = machine.decoder(codec)?;
            let mut v = json!({"kind": "decode", "track": "video", "codec": video_slug(codec), "element": e.decoder, "hardware": e.accel != "software"});
            if let Some(p) = &e.parser {
                v["parser"] = json!(p);
            }
            if let Some(d) = &e.download {
                v["download"] = json!(d);
            }
            Some(v)
        }
        Track::Audio => {
            let codec = info.audio?.codec;
            let e = machine.audio_decoder(codec)?;
            let mut v = json!({"kind": "decode", "track": "audio", "codec": audio_slug(codec), "element": e.decoder});
            if let Some(p) = &e.parser {
                v["parser"] = json!(p);
            }
            Some(v)
        }
    }
}

/// `[30, 1]`, `[30000, 1001]`.
pub fn fps_pair(fps: Fps) -> Value {
    json!([fps.num, fps.den.max(1)])
}

fn video_encoder(id: &str, shape: &VideoShape, keyframe_ms: u32, machine: &Machine) -> Option<Value> {
    let e = machine.encoder(id)?;
    let fps = shape.fps.as_f64().max(1.0);
    let frames = ((f64::from(keyframe_ms) * fps / 1000.0).round() as i64).max(1);
    let vars = Vars {
        video_bitrate_kbps: i64::from(shape.bitrate_kbps),
        keyframe_frames: frames,
        keyframe_secs: (i64::from(keyframe_ms) / 1000).max(1),
        fps: fps.round() as i64,
        cpu_count: cpu_count(),
        ..Vars::default()
    };
    let mut props = resolve(&e.properties, &vars);
    if let Some(kf) = &e.keyframe {
        let value = if kf.unit == "seconds" { vars.keyframe_secs } else { frames };
        props.insert(kf.property.clone(), json!(value));
    }
    let mut v = json!({
        "encoder": id, "element": e.encoder, "props": props, "hardware": e.accel != "software",
        "codec": video_slug(shape.codec), "width": shape.width, "height": shape.height,
        "fps": fps_pair(shape.fps), "bitrate_kbps": shape.bitrate_kbps, "keyframe_ms": keyframe_ms,
    });
    if let Some(p) = &e.parser {
        v["parser"] = json!(p);
    }
    Some(v)
}

fn audio_encoder(shape: &AudioShape, machine: &Machine) -> Option<Value> {
    let e = machine.audio_encoder(shape.codec)?;
    let vars = Vars { audio_bitrate_kbps: i64::from(shape.bitrate_kbps), ..Vars::default() };
    let mut v = json!({
        "encoder": e.id(), "element": e.encoder, "props": resolve(&e.properties, &vars),
        "codec": audio_slug(shape.codec), "channels": shape.channels,
        "sample_rate": shape.sample_rate, "bitrate_kbps": shape.bitrate_kbps,
    });
    if let Some(p) = &e.parser {
        v["parser"] = json!(p);
    }
    Some(v)
}

/// A catalogue property table with every derived value worked out.
fn resolve(props: &BTreeMap<String, PropValue>, vars: &Vars) -> Map<String, Value> {
    let mut out = Map::new();
    for (name, value) in props {
        let v = match value {
            PropValue::Derived(d) => match vars.resolve(d) {
                Some(n) => json!(n),
                None => continue,
            },
            PropValue::Bool(b) => json!(b),
            PropValue::Int(i) => json!(i),
            PropValue::Float(f) => json!(f),
            PropValue::Text(s) => json!(s),
        };
        out.insert(name.clone(), v);
    }
    out
}
