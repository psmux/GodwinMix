//! Before a move: refuse one that would touch what is on programme, and
//! take out the sources playing from what moves, so no file is held open.

use super::media_refs_map::Mapping;
use crate::control::call::Call;
use godwinmix_core::config::SourceConfig;
use godwinmix_core::media_org::Moved;
use godwinmix_core::mixer::Command;
use godwinmix_protocol::error::RpcError;
use std::path::PathBuf;

/// Refuse a move of anything on programme now, unless `force`. `froms` are
/// the files and folders about to move.
pub async fn check(call: &Call, froms: &[PathBuf], force: bool) -> Result<(), RpcError> {
    if force || froms.is_empty() {
        return Ok(());
    }
    let map = probe(call, froms);
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    let mut live: Vec<String> = status.program.iter().map(|s| s.to_string()).collect();
    if let Some(scene) = &status.scene {
        live.extend(call.app.scenes.sources_in(scene));
    }
    let configs = call.app.mixer.configs().await.map_err(|e| call.mixer_error(e))?;
    let mut using: Vec<String> = configs.sources.iter().filter(|s| live.contains(&s.id.to_string()) && uses(&map, s)).map(|s| s.id.to_string()).collect();
    if status.ad.as_ref().is_some_and(|ad| map.address(&ad.uri).is_some()) {
        using.push("the ad break".into());
    }
    using.dedup();
    if using.is_empty() {
        return Ok(());
    }
    Err(RpcError::not_in_state(format!(
        "{} on programme now and plays from what this would move. Take something else to programme first, \
         or pass force true to move it anyway, which restarts it on air.",
        using.join(", ") + if using.len() == 1 { " is" } else { " are" }
    ))
    .with("sources", using)
    .with("field", "force"))
}

/// A mapping of the places about to move onto themselves, which says
/// whether something names one of them.
fn probe(call: &Call, froms: &[PathBuf]) -> Mapping {
    let moved: Vec<Moved> = froms.iter().map(|p| Moved { from: String::new(), to: String::new(), from_path: p.clone(), to_path: p.clone(), also: Vec::new() }).collect();
    Mapping::new(call.app.library.dir(), &moved)
}

/// Before the move: take out every source playing from what moves, so no
/// file is held open. Answers their configs, to put back.
pub async fn release(call: &Call, froms: &[PathBuf]) -> Vec<SourceConfig> {
    let map = probe(call, froms);
    let Ok(configs) = call.app.mixer.configs().await else { return Vec::new() };
    let mut out = Vec::new();
    for s in configs.sources.into_iter().chain(configs.unstarted.into_iter().map(|u| u.config)).filter(|s| uses(&map, s)) {
        let id = s.id.clone();
        if call.app.mixer.request(|ack| Command::RemoveSource(id, Some(ack))).await.is_ok() {
            out.push(s);
        }
    }
    out
}

/// The move failed: put the sources back as they were.
pub async fn restore(call: &Call, released: Vec<SourceConfig>) {
    for s in released {
        let _ = call.app.mixer.request(|ack| Command::AddSource(Box::new(s), Some(ack))).await;
    }
}

/// Whether a source names anything the mapping moves. Only the paths are
/// looked at here, before the move, so a probe mapping does not misread a
/// folder as a file.
fn uses(map: &Mapping, s: &SourceConfig) -> bool {
    map.address(&s.uri).is_some() || s.params.get("uri").and_then(|u| u.as_str()).is_some_and(|u| map.address(u).is_some())
}
