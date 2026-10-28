//! A plan, from an import or a followed file, back as a channel kept here:
//! the other half of `compile.rs`.

use super::compile::rendition;
use godwinmix_flussonic::import::{ShowPlan, HLS_OUTPUT};
use godwinmix_protocol::headend::profile::{COPY, CUSTOM};
use godwinmix_protocol::headend::{ChannelBackup, ChannelHls, ChannelInput, ChannelOutput, HeadendChannel, TranscodeProfile};
use serde_json::Value;

/// The GPU a rendition names, and the rendition without it, which is the
/// profile it was made from. A ladder's rungs all name the first's.
fn off_gpu(r: &Value) -> (Option<u32>, Value) {
    let mut r = r.clone();
    if let Some(rungs) = r.get_mut("ladder").and_then(Value::as_array_mut) {
        let mut gpu = None;
        for rung in rungs.iter_mut() {
            let (g, bare) = off_gpu(rung);
            gpu = gpu.or(g);
            *rung = bare;
        }
        return (gpu, r);
    }
    let gpu = r.get_mut("video").and_then(Value::as_object_mut).and_then(|v| v.remove("gpu"));
    (gpu.and_then(|g| g.as_u64()).and_then(|g| u32::try_from(g).ok()), r)
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
    let first = super::compile::top(r);
    let ladder = r["ladder"].as_array().into_iter().flatten().skip(1).filter_map(|rung| serde_json::from_value(rung["video"].clone()).ok()).collect();
    let custom = TranscodeProfile { video: serde_json::from_value(first["video"].clone()).ok(), audio: serde_json::from_value(first["audio"].clone()).ok(), ladder, ..TranscodeProfile::default() };
    match profiles.iter().find(|p| rendition(p, None).as_ref() == Some(r)) {
        Some(p) => (p.id.clone(), None),
        None => (CUSTOM.into(), Some(custom)),
    }
}

/// A plan, from an import or a followed file, as a channel kept here.
pub fn channel_of(p: &ShowPlan, profiles: &[TranscodeProfile], origin: &str) -> HeadendChannel {
    let hls = p.outputs.iter().find(|o| o["id"] == HLS_OUTPUT).cloned().unwrap_or_default();
    let r = hls.get("rendition").filter(|r| r.is_object());
    let (gpu, bare) = match r {
        Some(r) => {
            let (g, b) = off_gpu(r);
            (g, Some(b))
        }
        None => (None, None),
    };
    let (profile, custom) = conversion_of(bare.as_ref(), profiles);
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
        video_overrides: None,
        hls: hls_of(&hls["params"]),
        outputs,
        gpu,
        origin: Some(origin.to_string()),
    }
}
