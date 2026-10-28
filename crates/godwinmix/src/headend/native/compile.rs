//! A channel as the plan a Flussonic import makes, and back. The plan is
//! what the keeper runs (`ShowPlan`: one direct show, its input, the HLS
//! output every player and the Flussonic URLs read, and any pushes), so a
//! channel set up here and a stream imported from a file run as the same
//! show, and everything that reads one reads the other.

use godwinmix_flussonic::import::{ShowPlan, HLS_OUTPUT};
use godwinmix_protocol::headend::profile::{builtins, CUSTOM};
use godwinmix_protocol::headend::{ChannelHls, ChannelInput, HeadendChannel, TranscodeProfile};
use godwinmix_protocol::rendition::{AudioWant, VideoWant};
use serde_json::{json, Map, Value};

/// Every profile a channel may name: the built in ones, then `saved`.
pub fn all_profiles(saved: &[TranscodeProfile]) -> Vec<TranscodeProfile> {
    builtins().into_iter().chain(saved.iter().cloned()).collect()
}

/// The conversion a channel asks for, or why it has none.
pub fn profile_of<'a>(c: &'a HeadendChannel, profiles: &'a [TranscodeProfile]) -> Result<&'a TranscodeProfile, String> {
    if c.profile == CUSTOM {
        return c.custom.as_ref().ok_or_else(|| format!("channel {} has profile custom and no custom settings. Send custom with its video and audio, or name a profile.", c.name));
    }
    profiles.iter().find(|p| p.id == c.profile).ok_or_else(|| {
        let ids: Vec<&str> = profiles.iter().map(|p| p.id.as_str()).collect();
        format!("there is no profile {}. Use one of {}, or custom.", c.profile, ids.join(", "))
    })
}

fn empty_video(v: &VideoWant) -> bool {
    v == &VideoWant::default()
}

fn empty_audio(a: &AudioWant) -> bool {
    a == &AudioWant::default()
}

/// A profile as an output's rendition, with a channel's `overrides` over
/// its picture, or None for a pass through. A profile with a ladder is
/// `{"ladder": [...]}`, its `video` the first rung and each further one
/// that rung over `video` (less `match_source`, which is the top's alone),
/// every one with the same sound; else `{video, audio}`.
pub fn rendition(p: &TranscodeProfile, overrides: Option<&VideoWant>) -> Option<Value> {
    let top = p.video.as_ref().map(|v| overrides.map_or_else(|| v.clone(), |o| o.over(v)));
    let single = one(top.as_ref(), p.audio.as_ref())?;
    let Some(top) = top.filter(|_| !p.ladder.is_empty()) else { return Some(single) };
    let mut rungs = vec![single];
    for rung in &p.ladder {
        let under = VideoWant { match_source: None, ..top.clone() };
        let v = overrides.map_or_else(|| rung.over(&under), |o| o.over(&rung.over(&under)));
        rungs.extend(one(Some(&v), p.audio.as_ref()));
    }
    Some(json!({"ladder": rungs}))
}

/// One rendition, `{video, audio}`, or None when it asks for nothing.
fn one(video: Option<&VideoWant>, audio: Option<&AudioWant>) -> Option<Value> {
    let mut r = Map::new();
    if let Some(v) = video.filter(|v| !empty_video(v)) {
        r.insert("video".into(), serde_json::to_value(v).ok()?);
    }
    if let Some(a) = audio.filter(|a| !empty_audio(a)) {
        r.insert("audio".into(), serde_json::to_value(a).ok()?);
    }
    (!r.is_empty()).then_some(Value::Object(r))
}

/// The one rendition an output that sends a single stream carries: a
/// ladder's first rung.
pub fn top(r: &Value) -> Value {
    r.get("ladder").and_then(|l| l.get(0)).cloned().unwrap_or_else(|| r.clone())
}

/// The input as a direct show takes it: the card goes into `params`.
pub fn input_spec(i: &ChannelInput) -> Value {
    let mut spec = Map::new();
    spec.insert("uri".into(), json!(i.uri.trim()));
    if let Some(p) = i.program {
        spec.insert("program".into(), json!(p));
    }
    let mut params = i.params.clone();
    if let Some(card) = i.interface.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        params.insert("interface".into(), json!(card));
    }
    if !params.is_empty() {
        spec.insert("params".into(), Value::Object(params));
    }
    if let Some(b) = &i.backup {
        spec.insert("backup".into(), input_spec(&b.as_input()));
    }
    Value::Object(spec)
}

fn hls_params(h: &ChannelHls) -> Value {
    let mut p = json!({"segments": h.segments, "access": h.access, "segment_ms": h.segment_ms});
    if let Some(w) = h.window {
        p["window"] = json!(w);
    }
    p
}

/// A rendition with the channel's GPU in it: the card it names, else the
/// profile's own, else `auto`, on every rung of a ladder, which runs on one
/// card. A pass through of the picture has none.
fn on_gpu(mut r: Value, gpu: Option<u32>) -> Value {
    if let Some(rungs) = r.get_mut("ladder").and_then(Value::as_array_mut) {
        for rung in rungs.iter_mut() {
            *rung = on_gpu(rung.take(), gpu);
        }
        return r;
    }
    if let Some(v) = r.get_mut("video").and_then(Value::as_object_mut) {
        match gpu {
            Some(n) => {
                v.insert("gpu".into(), json!(n));
            }
            None => {
                v.entry("gpu").or_insert(json!("auto"));
            }
        }
    }
    r
}

/// The plan the keeper runs for `c`.
pub fn plan(c: &HeadendChannel, profiles: &[TranscodeProfile]) -> Result<ShowPlan, String> {
    let r = rendition(profile_of(c, profiles)?, c.video_overrides.as_ref()).map(|r| on_gpu(r, c.gpu));
    let mut hls = json!({"id": HLS_OUTPUT, "uri": format!("hls://{HLS_OUTPUT}"), "params": hls_params(&c.hls)});
    if let Some(r) = &r {
        hls["rendition"] = r.clone();
    }
    let mut outputs = vec![hls];
    for (i, o) in c.outputs.iter().enumerate() {
        let id = if o.id.is_empty() { format!("push-{}", i + 1) } else { o.id.clone() };
        let mut out = json!({"id": id, "uri": o.uri.trim()});
        if let Some(r) = r.as_ref().filter(|_| !o.copy) {
            out["rendition"] = top(r);
        }
        outputs.push(out);
    }
    Ok(ShowPlan { name: c.name.clone(), title: c.title.clone(), disabled: !c.enabled, input: input_spec(&c.input), outputs, copy: r.is_none() })
}

pub use super::back::channel_of;
