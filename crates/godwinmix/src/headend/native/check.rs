//! What a channel and a profile must be before the station keeps them.
//! Each refusal says what is wrong and what to send instead.

use godwinmix_flussonic::import::HLS_OUTPUT;
use godwinmix_protocol::headend::profile::{COPY, CUSTOM};
use godwinmix_protocol::headend::{ChannelInput, HeadendChannel, TranscodeProfile};
use std::collections::BTreeSet;

/// A public name: the path segment of `/<name>/index.m3u8`.
pub fn name(n: &str) -> Result<(), String> {
    let ok = !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && n.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    if ok {
        return Ok(());
    }
    Err(format!("'{n}' cannot be a channel's name: it is the path players use, /NAME/index.m3u8, so it is 1 to 64 letters, digits, '-', '_' or '.', starting with a letter or a digit. CH117 or news-hd, say."))
}

fn input(i: &ChannelInput, what: &str) -> Result<(), String> {
    let uri = i.uri.trim();
    if uri.is_empty() {
        return Err(format!("the {what} has no address. Give one such as udp://@239.0.0.1:5000, srt://192.0.2.10:9000 or rtmp://host/app/key."));
    }
    if !(uri.contains("://") || uri.starts_with("channel:")) {
        return Err(format!("the {what} '{uri}' has no scheme. Write it as udp://@{uri} for multicast, or with srt://, rtmp://, rtsp://, https:// or file://."));
    }
    if i.program == Some(0) {
        return Err(format!("program 0 of the {what} is the network table, not a programme. Give the program number the scan lists, or leave it out to take the first."));
    }
    Ok(())
}

pub fn channel(c: &HeadendChannel, profiles: &[TranscodeProfile]) -> Result<(), String> {
    name(&c.name)?;
    input(&c.input, "input")?;
    if let Some(b) = &c.input.backup {
        input(&b.as_input(), "backup input")?;
    }
    super::compile::profile_of(c, profiles)?;
    if c.profile == CUSTOM {
        profile_body(c.custom.as_ref().expect("profile_of checked it"))?;
    }
    if !matches!(c.hls.segments.as_str(), "both" | "ts" | "cmaf") {
        return Err(format!("HLS segments '{}' is not one of both, ts or cmaf.", c.hls.segments));
    }
    if !matches!(c.hls.access.as_str(), "open" | "keyed") {
        return Err(format!("HLS access '{}' is not open or keyed.", c.hls.access));
    }
    if !(500..=10_000).contains(&c.hls.segment_ms) {
        return Err(format!("an HLS segment of {} ms is outside 500 to 10000.", c.hls.segment_ms));
    }
    let mut ids = BTreeSet::new();
    for (i, o) in c.outputs.iter().enumerate() {
        let id = if o.id.is_empty() { format!("push-{}", i + 1) } else { o.id.clone() };
        if !o.uri.contains("://") {
            return Err(format!("output {id} has no address. Give udp://239.0.1.1:5000, srt://192.0.2.30:9000 or rtmp://host/app/key."));
        }
        if id == HLS_OUTPUT || !ids.insert(id.clone()) {
            return Err(format!("two outputs are called {id}, or one has the HLS output's id. Give each its own."));
        }
    }
    Ok(())
}

/// A profile's settings, whoever names it.
fn profile_body(p: &TranscodeProfile) -> Result<(), String> {
    if let Some(v) = &p.video {
        if v.bitrate_kbps.is_some_and(|b| !(100..=100_000).contains(&b)) {
            return Err("a picture bitrate is 100 to 100000 kbit/s.".into());
        }
        if v.width.is_some_and(|w| w % 2 == 1 || w > 7680) || v.height.is_some_and(|h| h % 2 == 1 || h > 4320) {
            return Err("a size is even on both sides, up to 7680x4320.".into());
        }
        if v.keyframe_frames == Some(0) || v.keyframe_ms == Some(0) {
            return Err("a keyframe interval of 0 is never; leave it out for the encoder's own.".into());
        }
    }
    if let Some(a) = &p.audio {
        if a.bitrate_kbps.is_some_and(|b| !(16..=512).contains(&b)) {
            return Err("a sound bitrate is 16 to 512 kbit/s.".into());
        }
    }
    Ok(())
}

/// A profile someone saves.
pub fn profile(p: &TranscodeProfile, builtins: &[TranscodeProfile]) -> Result<(), String> {
    let slug = !p.id.is_empty() && p.id.len() <= 48 && p.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !p.id.starts_with('-');
    if !slug {
        return Err(format!("'{}' cannot be a profile's id: lower case letters, digits and '-', such as sd-news.", p.id));
    }
    if p.id == COPY || p.id == CUSTOM || builtins.iter().any(|b| b.id == p.id) {
        return Err(format!("{} is a built in profile, which stays as it is. Save yours under another id.", p.id));
    }
    if p.name.trim().is_empty() {
        return Err("a profile needs a name the screen can show.".into());
    }
    profile_body(p)
}
