//! What the direct host says of the scan's probes: their numbers from
//! `direct.stats`, asked for by name, and their tracks from the last
//! `direct.input`, made into the programs `headend.scan` answers.

use crate::station::state::Station;
use godwinmix_protocol::headend::{ScannedProgram, ScannedTrack};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// Each probe's input numbers, by id. Empty while the host is not up.
pub async fn stats(st: &Arc<Station>, ids: &[String]) -> Result<BTreeMap<String, Value>, String> {
    let Some(plugins) = st.direct.plugins().cloned().filter(|p| p.is_running(crate::channels::PLUGIN)) else {
        return Err("the ingest plugin, which reads every channel's input, is not running. Start it from Sources > More plugins, or wait for the station to install it.".into());
    };
    let args = json!({"name": "direct.stats", "arguments": {"ids": ids}});
    let asked = tokio::task::spawn_blocking(move || plugins.call_provide(crate::channels::PLUGIN, "discover", "tool.call", args));
    let answer = match tokio::time::timeout(Duration::from_secs(3), asked).await {
        Ok(Ok(Ok(v))) => v,
        Ok(Ok(Err(e))) => return Err(format!("the direct host said {e:#}")),
        _ => return Err("the direct host did not answer in time".into()),
    };
    Ok(answer["shows"].as_array().into_iter().flatten().filter_map(|r| Some((r["id"].as_str()?.to_string(), r["input"].clone()))).collect())
}

/// The programs a probe of the whole feed listed.
pub fn listed(input: &Value) -> Vec<ScannedProgram> {
    input["programs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some(ScannedProgram {
                number: u16::try_from(p["number"].as_u64()?).ok()?,
                name: p["name"].as_str().unwrap_or_default().to_string(),
                provider: p["provider"].as_str().unwrap_or_default().to_string(),
                streams: p["streams"].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(String::from)).collect(),
                ..ScannedProgram::default()
            })
        })
        .collect()
}

fn num(v: &Value) -> Option<u32> {
    v.as_u64().filter(|n| *n > 0).map(|n| n as u32)
}

fn track(t: &Value) -> ScannedTrack {
    ScannedTrack {
        kind: t["kind"].as_str().unwrap_or_default().to_string(),
        codec: t["codec"].as_str().unwrap_or_default().to_string(),
        width: num(&t["width"]),
        height: num(&t["height"]),
        fps: t["fps"].as_f64().filter(|f| *f > 0.0),
        interlaced: t["field_coded"].as_bool().unwrap_or(false),
        channels: num(&t["channels"]),
        lang: t["lang"].as_str().map(String::from),
        pid: t["pid"].as_u64().and_then(|p| u16::try_from(p).ok()),
    }
}

/// One program's probe, read into its program: the tracks the host said in
/// its last `direct.input`, with the picture or sound its numbers have and
/// that event did not yet, since the event can be a second behind.
pub fn fill(p: &mut ScannedProgram, input: Option<&Value>, event: Option<&Value>) {
    let Some(i) = input else { return };
    p.kbps = num(&i["kbps"]);
    p.scrambled = i["scrambled"].as_u64().unwrap_or(0) > 0 || event.is_some_and(|e| e["scrambled"].as_u64().unwrap_or(0) > 0);
    p.tracks = event.and_then(|e| e["tracks"].as_array()).into_iter().flatten().map(track).collect();
    let has = |p: &ScannedProgram, kind: &str| p.tracks.iter().any(|t| t.kind == kind);
    if let Some(codec) = i["video_codec"].as_str().filter(|c| !c.is_empty() && !has(p, "video")) {
        let v = ScannedTrack { kind: "video".into(), codec: codec.into(), width: num(&i["width"]), height: num(&i["height"]), fps: i["fps"].as_f64().filter(|f| *f > 0.0), ..ScannedTrack::default() };
        p.tracks.insert(0, v);
    }
    if let Some(codec) = i["audio_codec"].as_str().filter(|c| !c.is_empty() && !has(p, "audio")) {
        p.tracks.push(ScannedTrack { kind: "audio".into(), codec: codec.into(), channels: num(&i["audio_channels"]), ..ScannedTrack::default() });
    }
}

/// Whether the PMT lists a picture.
fn has_video(p: &ScannedProgram) -> bool {
    p.streams.iter().any(|s| ["264", "265", "hevc", "mpeg2", "mpeg video", "video", "av1"].iter().any(|k| s.contains(k)))
}

/// Whether a program's probe has said enough to stop waiting for it: its
/// picture's size, or its sound for a program with no picture.
pub fn heard(p: &ScannedProgram, input: Option<&Value>) -> bool {
    let Some(i) = input else { return false };
    let said = |k: &str| i[k].as_str().is_some_and(|c| !c.is_empty());
    if has_video(p) {
        said("video_codec") && num(&i["width"]).is_some()
    } else {
        said("audio_codec")
    }
}
