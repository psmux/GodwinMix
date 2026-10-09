//! What a destination request may say, as pure functions over a channel's
//! list, so each rule is tested without a mixer or a store.

use godwinmix_protocol::destination::{
    platform, platform_ids, AddDestinationRequest, Carriage, KeyRule, Platform,
    SetDestinationRequest, StoredDestination,
};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::rendition::RenditionChoice;
use godwinmix_render::presets::builtin;

use crate::channels::transcode::request_for;

/// Add a destination to `list`, and answer its id.
pub fn add(list: &mut Vec<StoredDestination>, req: &AddDestinationRequest) -> Result<String, RpcError> {
    let p = platform(&req.platform).ok_or_else(|| unknown_platform(&req.platform))?;
    let label = clean(req.label.as_deref()).unwrap_or_else(|| p.title.to_string());
    let server = clean(req.server.as_deref()).unwrap_or_else(|| p.server.to_string());
    let mut d = StoredDestination {
        id: unique_id(list, &slug(&label).unwrap_or_else(|| p.id.to_string())),
        platform: p.id.to_string(),
        label,
        server,
        key: key_for(p, req.key.as_deref()),
        stream: clean(req.stream.as_deref()).unwrap_or_else(|| "*".into()),
        enabled: req.enabled.unwrap_or(true),
        rendition: None,
    };
    d.rendition = rendition(req.rendition.as_ref(), &d.id)?;
    check(p, &d)?;
    let id = d.id.clone();
    list.push(d);
    Ok(id)
}

/// Change one destination, naming only what moves.
pub fn set(list: &mut [StoredDestination], req: &SetDestinationRequest) -> Result<(), RpcError> {
    let ids: Vec<String> = list.iter().map(|d| d.id.clone()).collect();
    let d = list
        .iter_mut()
        .find(|d| d.id == req.destination)
        .ok_or_else(|| not_found(&req.id, &req.destination, &ids))?;
    let p = platform(&d.platform).ok_or_else(|| unknown_platform(&d.platform))?;
    let mut wanted = d.clone();
    if let Some(label) = clean(req.label.as_deref()) {
        wanted.label = label;
    }
    if let Some(server) = req.server.as_deref() {
        wanted.server = clean(Some(server)).unwrap_or_else(|| p.server.to_string());
    }
    if let Some(key) = req.key.as_deref() {
        wanted.key = key_for(p, Some(key));
    }
    if let Some(stream) = clean(req.stream.as_deref()) {
        wanted.stream = stream;
    }
    if let Some(enabled) = req.enabled {
        wanted.enabled = enabled;
    }
    if let Some(ask) = &req.rendition {
        wanted.rendition = rendition(ask.as_ref(), &wanted.id)?;
    }
    check(p, &wanted)?;
    *d = wanted;
    Ok(())
}

/// Take one destination off the list, and answer it.
pub fn remove(
    list: &mut Vec<StoredDestination>,
    channel: &str,
    id: &str,
) -> Result<StoredDestination, RpcError> {
    let ids: Vec<String> = list.iter().map(|d| d.id.clone()).collect();
    let at = list.iter().position(|d| d.id == id).ok_or_else(|| not_found(channel, id, &ids))?;
    Ok(list.remove(at))
}

/// What to keep of a rendition ask: nothing for a copy, the ask itself
/// when it names a preset or a request that can be read.
fn rendition(ask: Option<&RenditionChoice>, id: &str) -> Result<Option<RenditionChoice>, RpcError> {
    let Some(ask) = ask else { return Ok(None) };
    let request = request_for(id, ask).map_err(|why| {
        let singles: Vec<String> = builtin().into_iter().filter(|p| p.ladder.is_none()).map(|p| p.id).collect();
        RpcError::invalid_params(format!("That rendition cannot be used here: {why}"))
            .with("field", "rendition")
            .with("presets", singles)
    })?;
    Ok(request.map(|_| ask.clone()))
}

fn clean(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// A platform with no key keeps none, whatever was sent.
fn key_for(p: &Platform, key: Option<&str>) -> Option<String> {
    if p.key == KeyRule::None {
        return None;
    }
    clean(key)
}

/// The rules a stored destination has to meet, whichever method made it.
fn check(p: &Platform, d: &StoredDestination) -> Result<(), RpcError> {
    let field = |e: RpcError, f: &str| e.with("field", f).with("platform", p.id).with("destination", d.id.clone());
    if p.carriage.is_local() {
        return local(p, d).map_err(|(e, f)| field(e, f));
    }
    let example = if p.carriage == Carriage::Srt { "srt://192.168.1.50:9000" } else { "rtmp://host/live" };
    if d.server.is_empty() {
        let msg = format!("{} needs a server address. Send `server`, as in {example}.", p.title);
        return Err(field(RpcError::invalid_params(msg), "server"));
    }
    let scheme = d.server.split_once("://").map(|(s, _)| s.to_ascii_lowercase()).unwrap_or_default();
    let fits = match p.carriage {
        Carriage::Rtmp => scheme == "rtmp" || scheme == "rtmps",
        Carriage::Srt => scheme == "srt",
        Carriage::File | Carriage::Hls => true,
    };
    if !fits {
        let msg = format!("{} takes an address like {example}, and this one does not start that way.", p.title);
        return Err(field(RpcError::invalid_params(msg), "server"));
    }
    let keyless = d.key.is_none();
    if keyless && p.key == KeyRule::Required {
        let msg = format!("{} needs its stream key. Copy it from the platform and send it as `key`.", p.title);
        return Err(field(RpcError::invalid_params(msg), "key"));
    }
    let path = d.server.split_once("://").map(|(_, r)| r).unwrap_or("");
    if keyless && p.carriage == Carriage::Rtmp && path.split('/').filter(|s| !s.is_empty()).count() < 3 {
        let msg = "that address has no stream key on the end. Send `key`, or the whole address \
                   with the key on it, as in rtmp://host/live/key.";
        return Err(field(RpcError::invalid_params(msg), "key"));
    }
    Ok(())
}

/// A recording or a watch link: no key, an optional folder or `hls://`
/// params, and the stream as it arrives.
fn local(p: &Platform, d: &StoredDestination) -> Result<(), (RpcError, &'static str)> {
    if d.rendition.is_some() {
        let msg = format!(
            "{} copies the stream as the encoder sends it and converts nothing. Leave `rendition` out; \
             to change the format, change it in the encoder.",
            p.title
        );
        return Err((RpcError::invalid_params(msg), "rendition"));
    }
    let server = d.server.trim();
    let scheme = server.split_once("://").map(|(s, _)| s.to_ascii_lowercase());
    match p.carriage {
        Carriage::Hls if !server.is_empty() && scheme.as_deref() != Some("hls") => {
            let msg = "A watch link takes no address. Leave `server` out, or send hls:// with params, as in hls://?segment_ms=2000.";
            Err((RpcError::invalid_params(msg), "server"))
        }
        Carriage::File if scheme.as_deref().is_some_and(|s| s != "file") => {
            let msg = "Record takes a folder on this machine, as in D:/Recordings or file:///srv/recordings, or nothing for the recordings folder.";
            Err((RpcError::invalid_params(msg), "server"))
        }
        _ => Ok(()),
    }
}

fn slug(label: &str) -> Option<String> {
    let mut out = String::new();
    for c in label.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    out.starts_with(|c: char| c.is_ascii_alphabetic()).then_some(out)
}

fn unique_id(list: &[StoredDestination], base: &str) -> String {
    let taken = |id: &str| list.iter().any(|d| d.id == id);
    if !taken(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| !taken(id)).expect("an unbounded range ends")
}

fn unknown_platform(id: &str) -> RpcError {
    let ids = platform_ids();
    RpcError::invalid_params(format!("'{id}' is not a platform. Use one of {}.", ids.join(", ")))
        .with("field", "platform")
        .with("platforms", ids)
}

fn not_found(channel: &str, id: &str, ids: &[String]) -> RpcError {
    RpcError::not_found("destination", id, ids).with("channel", channel)
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;
