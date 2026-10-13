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

/// The sources taken out for a move, and what the programme showed, so it
/// can show it again once they are back.
#[derive(Default)]
pub struct Released {
    pub sources: Vec<SourceConfig>,
    program: Option<String>,
    scene: Option<String>,
}

/// Before the move: take out every source playing from what moves, so no
/// file is held open.
pub async fn release(call: &Call, froms: &[PathBuf]) -> Released {
    let map = probe(call, froms);
    let status = call.app.mixer.status().await.ok();
    let Ok(configs) = call.app.mixer.configs().await else { return Released::default() };
    let mut out = Vec::new();
    for s in configs.sources.into_iter().chain(configs.unstarted.into_iter().map(|u| u.config)).filter(|s| uses(&map, s)) {
        let id = s.id.clone();
        if call.app.mixer.request(|ack| Command::RemoveSource(id, Some(ack))).await.is_ok() {
            out.push(s);
        }
    }
    let (program, scene) = status.map(|s| (s.program.map(|p| p.to_string()), s.scene)).unwrap_or_default();
    let on_air = |id: &str| program.as_deref() == Some(id) || scene.as_ref().is_some_and(|sc| call.app.scenes.sources_in(sc).iter().any(|x| x == id));
    let live = out.iter().any(|s| on_air(&s.id.to_string()));
    Released { sources: out, program: program.filter(|_| live), scene: scene.filter(|_| live) }
}

/// The move failed: put the sources back as they were.
pub async fn restore(call: &Call, released: Released) {
    for s in released.sources {
        let _ = call.app.mixer.request(|ack| Command::AddSource(Box::new(s), Some(ack))).await;
    }
    let _ = retake(call, released.program, released.scene).await;
}

/// Put back on programme what was there before a source on it was taken
/// out, scene first. `None` when there was nothing to take again or it
/// went back; otherwise what to say.
pub async fn retake(call: &Call, program: Option<String>, scene: Option<String>) -> Option<String> {
    let params = match (scene, program) {
        (Some(scene), _) => serde_json::json!({ "scene": scene }),
        (None, Some(source)) => serde_json::json!({ "source": source }),
        (None, None) => return None,
    };
    match super::project::invoke(call, "program.take", params.clone()).await {
        Ok(_) => None,
        Err(e) => Some(format!("the programme did not go back to {params} after the move: {}. Take it again", e.message)),
    }
}

impl Released {
    pub fn on_air(&self) -> (Option<String>, Option<String>) {
        (self.program.clone(), self.scene.clone())
    }
}

/// Whether a source names anything the mapping moves. Only the paths are
/// looked at here, before the move, so a probe mapping does not misread a
/// folder as a file.
fn uses(map: &Mapping, s: &SourceConfig) -> bool {
    map.address(&s.uri).is_some() || s.params.get("uri").and_then(|u| u.as_str()).is_some_and(|u| map.address(u).is_some())
}
