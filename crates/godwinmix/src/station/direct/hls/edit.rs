//! Adding and changing a show's `hls://` output, by the rules in
//! [`super::spec`].

use super::spec;
use godwinmix_protocol::destination::StoredDestination;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::shows::{ShowOutputSetRequest, ShowOutputSpec};

/// An HLS output: `hls://<name>`, served from the station's own port at
/// `/hls/<id>/master.m3u8?show=<show>`.
pub fn add(list: &mut Vec<StoredDestination>, label: Option<String>, uri: &str, out: &ShowOutputSpec) -> Result<String, RpcError> {
    let Some(name) = spec::name_of(uri) else {
        let msg = "an HLS output takes an address like hls://viewers, whose name is the start of its link.";
        return Err(RpcError::invalid_params(msg).with("field", "uri"));
    };
    let id = out.id.clone().unwrap_or_else(|| name.clone());
    if let Some(r) = &out.rendition {
        spec::check_rendition(&id, r)?;
    }
    let server = spec::address(&name, out.params.as_ref())?;
    let rendition = out.rendition.clone().filter(|r| crate::channels::transcode::request_for(&id, r).ok().flatten().is_some());
    // Made under the name first; a wanted id is given by `add` afterwards.
    let made = std::iter::once(name.clone())
        .chain((2..).map(|n| format!("{name}-{n}")))
        .find(|c| !list.iter().any(|d| &d.id == c))
        .unwrap_or(name);
    let enabled = out.enabled.unwrap_or(true);
    let label = label.unwrap_or_else(|| made.clone());
    list.push(StoredDestination { id: made.clone(), platform: spec::SCHEME.into(), label, server, key: None, stream: "main".into(), enabled, rendition });
    Ok(made)
}

/// A new name or new params for an HLS output. Params given replace all of
/// them; a new name keeps the params it had.
pub fn set(d: &mut StoredDestination, req: &ShowOutputSetRequest) -> Result<(), RpcError> {
    let name = match &req.uri {
        Some(uri) => spec::name_of(uri).ok_or_else(|| {
            RpcError::invalid_params("an hls output keeps its scheme: give an address like hls://viewers").with("field", "uri")
        })?,
        None => spec::name_of(&d.server).unwrap_or_else(|| d.id.clone()),
    };
    let kept = d.server.split_once('?').map(|(_, q)| q.to_string());
    d.server = match (&req.params, kept) {
        (Some(p), _) => spec::address(&name, Some(p))?,
        (None, Some(q)) => format!("hls://{name}?{q}"),
        (None, None) => format!("hls://{name}"),
    };
    Ok(())
}
