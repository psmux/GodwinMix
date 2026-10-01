//! What an output request may say, over a show's list of outputs.
//!
//! RTMP and SRT outputs go through the same rules a channel destination
//! does (`channel_destinations::rules`), so a platform needs its key and an
//! address needs a scheme it can carry. Outputs a channel never has (UDP,
//! RTP and RIST, unicast or multicast) are checked here and carry their
//! scheme as their platform.

use crate::control::methods::channel_destinations::rules;
use godwinmix_protocol::destination::{AddDestinationRequest, SetDestinationRequest, StoredDestination};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::{ShowOutputSetRequest, ShowOutputSpec};

/// Schemes an output may have that no channel destination does.
pub const PLAIN: [&str; 3] = ["udp", "rtp", "rist"];

fn scheme(uri: &str) -> String {
    uri.split_once("://").map(|(s, _)| s.to_ascii_lowercase()).unwrap_or_default()
}

/// Add one output to `list`, by the rules, and answer its id.
pub fn add(list: &mut Vec<StoredDestination>, show: &str, spec: &ShowOutputSpec) -> Result<String, RpcError> {
    let uri = spec.uri.as_deref().map(str::trim).unwrap_or_default();
    let platform = match spec.platform.as_deref() {
        Some(p) => p.to_string(),
        None if scheme(uri) == "srt" => "srt".into(),
        None if PLAIN.contains(&scheme(uri).as_str()) => scheme(uri),
        None => "custom".into(),
    };
    let label = spec.label.clone().or_else(|| spec.id.clone());
    let made = if PLAIN.contains(&platform.as_str()) {
        plain(list, &platform, label, uri, spec)?
    } else {
        let req = AddDestinationRequest {
            id: show.to_string(),
            platform,
            label,
            server: (!uri.is_empty()).then(|| uri.to_string()),
            key: spec.key.clone(),
            stream: Some("main".into()),
            enabled: spec.enabled,
            rendition: spec.rendition.clone(),
        };
        rules::add(list, &req)?
    };
    match &spec.id {
        Some(wanted) if *wanted != made => rename(list, &made, wanted),
        _ => Ok(made),
    }
}

/// An output with a scheme of its own: UDP, RTP or RIST.
fn plain(list: &mut Vec<StoredDestination>, platform: &str, label: Option<String>, uri: &str, spec: &ShowOutputSpec) -> Result<String, RpcError> {
    if scheme(uri) != platform {
        let msg = format!("a {platform} output takes an address like {platform}://239.2.2.2:5000, and this one does not start that way.");
        return Err(RpcError::invalid_params(msg).with("field", "uri"));
    }
    let base = label.unwrap_or_else(|| platform.to_string());
    let slug = match crate::channels::keys::slug(&base) {
        s if s.is_empty() => platform.to_string(),
        s => s,
    };
    let id = std::iter::once(slug.clone())
        .chain((2..).map(|n| format!("{slug}-{n}")))
        .find(|c| !list.iter().any(|d| &d.id == c))
        .unwrap_or(slug);
    let rendition = match &spec.rendition {
        Some(r) => crate::channels::transcode::request_for(&id, r)
            .map_err(|e| RpcError::invalid_params(e).with("field", "rendition"))?
            .map(|_| r.clone()),
        None => None,
    };
    let enabled = spec.enabled.unwrap_or(true);
    let d = StoredDestination { id: id.clone(), platform: platform.into(), label: base, server: uri.into(), key: None, stream: "main".into(), enabled, rendition };
    list.push(d);
    Ok(id)
}

fn rename(list: &mut [StoredDestination], from: &str, to: &str) -> Result<String, RpcError> {
    let ok = !to.is_empty() && to.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok {
        let msg = format!("an output id is a slug such as \"youtube-2\", and \"{to}\" is not one");
        return Err(RpcError::invalid_params(msg).with("field", "id"));
    }
    if list.iter().any(|d| d.id == to) {
        let msg = format!("the show already has an output called {to}. Pick another id, or leave it out to have one made.");
        return Err(RpcError::invalid_params(msg).with("field", "id").with("output", to));
    }
    if let Some(d) = list.iter_mut().find(|d| d.id == from) {
        d.id = to.to_string();
    }
    Ok(to.to_string())
}

/// Change one output, naming only what moves.
pub fn set(list: &mut [StoredDestination], req: &ShowOutputSetRequest) -> Result<(), RpcError> {
    let ids: Vec<String> = list.iter().map(|d| d.id.clone()).collect();
    let Some(d) = list.iter_mut().find(|d| d.id == req.output) else {
        return Err(RpcError::not_found("output", &req.output, &ids).with("show", req.id.clone()));
    };
    if !PLAIN.contains(&d.platform.as_str()) {
        let asked = SetDestinationRequest {
            id: req.id.clone(),
            destination: req.output.clone(),
            label: req.label.clone(),
            server: req.uri.clone(),
            key: req.key.clone(),
            stream: None,
            enabled: req.enabled,
            rendition: req.rendition.clone(),
        };
        return rules::set(std::slice::from_mut(d), &asked).map_err(|e| e.with("show", req.id.clone()));
    }
    let mut wanted = d.clone();
    if let Some(label) = &req.label {
        wanted.label = label.clone();
    }
    if let Some(uri) = &req.uri {
        if scheme(uri) != wanted.platform {
            let msg = format!("a {} output keeps its scheme. Remove it and add another to send somewhere else.", wanted.platform);
            return Err(RpcError::invalid_params(msg).with("field", "uri"));
        }
        wanted.server = uri.trim().to_string();
    }
    if let Some(enabled) = req.enabled {
        wanted.enabled = enabled;
    }
    if let Some(r) = &req.rendition {
        wanted.rendition = r.clone();
    }
    *d = wanted;
    Ok(())
}
