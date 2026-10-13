//! Keeping everything that names a library file working when it moves.
//!
//! A source holds its file in `uri` and sometimes `params.uri`, and a
//! graphic source holds pictures by library name in `params.fields`. A
//! scene's transitions and graphics hold files in their `params`. An SVG or
//! HTML template in the library, and a Graphics gallery item's
//! `graphic.toml`, can name a picture in the library. A move rewrites each
//! of those in the same call. A source cannot change its address in place,
//! and on Windows a file a source has open cannot be renamed at all, so a
//! source playing a file about to move is taken out first and put back
//! under the same id at the new place, with its fader and backup rule.

use super::media_refs_map::Mapping;
pub use super::media_refs_live::{check, release, restore};
use crate::control::call::Call;
use godwinmix_core::config::SourceConfig;
use godwinmix_core::media_org::Moved;
use godwinmix_core::mixer::Command;

/// After the move: point every source, scene and template at the new
/// places. Answers what was changed, in words; a part that could not be
/// changed is said too, never hidden.
pub async fn follow(call: &Call, moved: &[Moved], released: Vec<SourceConfig>) -> Vec<String> {
    let root = call.app.library.dir().to_path_buf();
    let map = Mapping::new(&root, moved);
    let mut out = Vec::new();
    for s in released {
        let id = s.id.clone();
        let next = rewritten(&map, &s).unwrap_or(s);
        match call.app.mixer.request(|ack| Command::AddSource(Box::new(next), Some(ack))).await {
            Ok(()) => out.push(format!("source {id}")),
            Err(e) => out.push(format!("source {id} did not start again at the new place: {e}. Bring it back with source.restore")),
        }
    }
    if map.is_empty() {
        return out;
    }
    out.extend(sources(call, &map).await);
    out.extend(scenes(call, &map));
    let gallery = godwinmix_core::gallery::dir();
    let depth = call.app.library.cfg().max_depth;
    let files = tokio::task::spawn_blocking(move || super::media_refs_files::rewrite(&root, &gallery, depth, &map)).await;
    out.extend(files.unwrap_or_default());
    out
}

/// A source's config with every reference to a moved file rewritten, or
/// `None` when it names none.
fn rewritten(map: &Mapping, s: &SourceConfig) -> Option<SourceConfig> {
    let mut next = s.clone();
    let mut changed = false;
    if let Some(u) = map.address(&s.uri) {
        next.uri = u;
        changed = true;
    }
    let mut params = serde_json::to_value(&s.params).ok()?;
    if map.json(&mut params) {
        next.params = serde_json::from_value(params).ok()?;
        changed = true;
    }
    changed.then_some(next)
}

async fn sources(call: &Call, map: &Mapping) -> Vec<String> {
    let Ok(configs) = call.app.mixer.configs().await else { return vec!["sources: the mixer did not answer, so none was pointed at the new place".into()] };
    let all = configs.sources.into_iter().chain(configs.unstarted.into_iter().map(|u| u.config));
    let mut out = Vec::new();
    for s in all {
        let Some(next) = rewritten(map, &s) else { continue };
        let id = s.id.clone();
        let _ = call.app.mixer.request(|ack| Command::RemoveSource(id.clone(), Some(ack))).await;
        match call.app.mixer.request(|ack| Command::AddSource(Box::new(next), Some(ack))).await {
            Ok(()) => out.push(format!("source {id}")),
            Err(e) => out.push(format!("source {id} did not start again at the new place: {e}. Bring it back with source.restore")),
        }
    }
    out
}

fn scenes(call: &Call, map: &Mapping) -> Vec<String> {
    let mut doc = match serde_json::to_value(call.app.scenes.document()) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    if !params_only(map, &mut doc) {
        return Vec::new();
    }
    let edit = call.app.scenes.edit(super::scenes::client(call).as_deref(), |c| {
        *c = serde_json::from_value(doc)?;
        Ok(())
    });
    match edit {
        Ok(_) => vec!["the scenes".into()],
        Err(e) => vec![format!("the scenes were not changed: {e:#}")],
    }
}

/// Rewrite strings only under a `params` key, where transitions and
/// graphics keep their files; ids and names elsewhere are left alone.
fn params_only(map: &Mapping, v: &mut serde_json::Value) -> bool {
    match v {
        serde_json::Value::Object(o) => o.iter_mut().fold(false, |c, (k, x)| (if k == "params" { map.json(x) } else { params_only(map, x) }) | c),
        serde_json::Value::Array(a) => a.iter_mut().fold(false, |c, x| params_only(map, x) | c),
        _ => false,
    }
}
