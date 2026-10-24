//! A channel as the plan a Flussonic import makes, and back. The plan is
//! what the keeper runs (`ShowPlan`: one direct show, its input, the HLS
//! output every player and the Flussonic URLs read, and any pushes), so a
//! channel set up here and a stream imported from a file run as the same
//! show, and everything that reads one reads the other.

use godwinmix_flussonic::import::{ShowPlan, HLS_OUTPUT};
use godwinmix_protocol::headend::profile::{builtins, COPY, CUSTOM};
use godwinmix_protocol::headend::{ChannelBackup, ChannelHls, ChannelInput, ChannelOutput, HeadendChannel, TranscodeProfile};
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

/// A profile as an output's rendition, or None for a pass through.
pub fn rendition(p: &TranscodeProfile) -> Option<Value> {
    let mut r = Map::new();
    if let Some(v) = p.video.as_ref().filter(|v| !empty_video(v)) {
        r.insert("video".into(), serde_json::to_value(v).ok()?);
    }
    if let Some(a) = p.audio.as_ref().filter(|a| !empty_audio(a)) {
        r.insert("audio".into(), serde_json::to_value(a).ok()?);
    }
    (!r.is_empty()).then_some(Value::Object(r))
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

/// The plan the keeper runs for `c`.
pub fn plan(c: &HeadendChannel, profiles: &[TranscodeProfile]) -> Result<ShowPlan, String> {
    let r = rendition(profile_of(c, profiles)?);
    let mut hls = json!({"id": HLS_OUTPUT, "uri": format!("hls://{HLS_OUTPUT}"), "params": hls_params(&c.hls)});
    if let Some(r) = &r {
        hls["rendition"] = r.clone();
    }
    let mut outputs = vec![hls];
    for (i, o) in c.outputs.iter().enumerate() {
        let id = if o.id.is_empty() { format!("push-{}", i + 1) } else { o.id.clone() };
        let mut out = json!({"id": id, "uri": o.uri.trim()});
        if let Some(r) = r.as_ref().filter(|_| !o.copy) {
            out["rendition"] = r.clone();
        }
        outputs.push(out);
    }
    Ok(ShowPlan { name: c.name.clone(), title: c.title.clone(), disabled: !c.enabled, input: input_spec(&c.input), outputs, copy: r.is_none() })
}

fn input_of(v: &Value) -> ChannelInput {
    let mut params = v["params"].as_object().cloned().unwrap_or_default();
    let interface = params.remove("interface").and_then(|i| i.as_str().map(String::from));
    ChannelInput {
        uri: v["uri"].as_str().unwrap_or_default().to_string(),
        program: v["program"].as_u64().and_then(|p| u16::try_from(p).ok()),
        interface,
        params,
        backup: v.get("backup").filter(|b| b.is_object()).map(|b| {
            let i = input_of(b);
            ChannelBackup { uri: i.uri, program: i.program, interface: i.interface, params: i.params }
        }),
    }
}

fn hls_of(v: &Value) -> ChannelHls {
    let d = ChannelHls::default();
    ChannelHls {
        segments: v["segments"].as_str().map_or(d.segments, String::from),
        access: v["access"].as_str().map_or(d.access, String::from),
        segment_ms: v["segment_ms"].as_u64().map_or(d.segment_ms, |m| m as u32),
        window: v["window"].as_u64().map(|w| w as u32),
    }
}

/// A rendition as the profile it is: one of `profiles` when it is exactly
/// one of them, `custom` otherwise.
fn conversion_of(r: Option<&Value>, profiles: &[TranscodeProfile]) -> (String, Option<TranscodeProfile>) {
    let Some(r) = r else { return (COPY.into(), None) };
    let custom = TranscodeProfile { video: serde_json::from_value(r["video"].clone()).ok(), audio: serde_json::from_value(r["audio"].clone()).ok(), ..TranscodeProfile::default() };
    match profiles.iter().find(|p| rendition(p).as_ref() == Some(r)) {
        Some(p) => (p.id.clone(), None),
        None => (CUSTOM.into(), Some(custom)),
    }
}

/// A plan, from an import or a followed file, as a channel kept here.
pub fn channel_of(p: &ShowPlan, profiles: &[TranscodeProfile], origin: &str) -> HeadendChannel {
    let hls = p.outputs.iter().find(|o| o["id"] == HLS_OUTPUT).cloned().unwrap_or_default();
    let r = hls.get("rendition").filter(|r| r.is_object());
    let (profile, custom) = conversion_of(r, profiles);
    let outputs = p
        .outputs
        .iter()
        .filter(|o| o["id"] != HLS_OUTPUT)
        .map(|o| ChannelOutput { id: o["id"].as_str().unwrap_or_default().to_string(), uri: o["uri"].as_str().unwrap_or_default().to_string(), copy: r.is_some() && o.get("rendition").is_none() })
        .collect();
    HeadendChannel {
        name: p.name.clone(),
        title: p.title.clone(),
        enabled: !p.disabled,
        input: input_of(&p.input),
        profile,
        custom,
        hls: hls_of(&hls["params"]),
        outputs,
        origin: Some(origin.to_string()),
    }
}
