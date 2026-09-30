//! What an import would do, worked out before anything is touched.
//!
//! Every part of the file is read and checked here, so a file that is wrong
//! anywhere is refused whole and a dry run is the same walk as the real one.

use super::bundle::{Bundle, MediaEntry};
use super::{entries, ids, media, settings, Change, Report};
use crate::channels::project::{self as channel_file, Incoming};
use crate::control::call::Call;
use godwinmix_core::config::keys::{self, Applies};
use godwinmix_core::config::{OutputConfig, SourceConfig};
use godwinmix_core::scene::document::Collection;
use godwinmix_protocol::error::RpcError;
use serde_json::Value;

/// Everything an import will do, in the order it does it.
pub struct Plan {
    pub settings: serde_json::Map<String, Value>,
    pub sources: entries::Moves<SourceConfig>,
    pub outputs: entries::Moves<OutputConfig>,
    pub scenes: Option<Collection>,
    pub channels: Vec<Incoming>,
    pub media: Vec<MediaEntry>,
}

pub async fn build(call: &Call, bundle: &Bundle, replace: bool, machine: bool, report: &mut Report) -> Result<Plan, RpcError> {
    let here = call.app.mixer.configs().await.map_err(|e| call.mixer_error(e))?;
    let settings = plan_settings(call, bundle, replace, machine, report);
    let sources = entries::plan("source", &bundle.sources, &here.sources, |s| &s.id, |s, id| s.id = id, replace, report)?;
    let outputs = entries::plan("output", &bundle.outputs, &here.outputs, |o| &o.id, |o, id| o.id = id, replace, report)?;
    let scenes = plan_scenes(call, bundle, replace, &sources.renamed, report)?;
    let channels = plan_channels(call, bundle, replace, report)?;
    let media = plan_media(call, bundle, report);
    Ok(Plan { settings, sources, outputs, scenes, channels, media })
}

fn plan_settings(call: &Call, bundle: &Bundle, replace: bool, machine: bool, report: &mut Report) -> serde_json::Map<String, Value> {
    let mut wanted = bundle.settings.clone();
    if machine {
        wanted.extend(bundle.machine.clone());
    } else if !bundle.machine.is_empty() {
        report.changes.push(Change::new("setting", "machine", "keep").note(
            "the file's machine settings (addresses, folders, hardware) were left as this mixer has them",
        ));
    }
    let mut out = serde_json::Map::new();
    for (key, value, now) in settings::changes(&call.app.config_path, &wanted, replace) {
        let from = now.map(|v| v.to_string()).unwrap_or_else(|| "its default".into());
        report.changes.push(Change::new("setting", &key, "set").note(&format!("{from} to {value}")));
        // Named from the keys this import writes, not from whatever else in
        // the file is pending: that was somebody else's change.
        if keys::find(&key).is_some_and(|k| k.applies == Applies::Restart) {
            report.needs_restart.push(format!("{key} is written to the config file and takes effect when the mixer restarts"));
        }
        out.insert(key, value);
    }
    out
}

fn plan_scenes(
    call: &Call,
    bundle: &Bundle,
    replace: bool,
    renamed: &std::collections::BTreeMap<String, String>,
    report: &mut Report,
) -> Result<Option<Collection>, RpcError> {
    let here = call.app.scenes.document();
    let mut value = match (&bundle.scenes, replace) {
        (Value::Null, false) => return Ok(None),
        // A file with no scenes, replacing: an empty collection, which is
        // what New project sends.
        (Value::Null, true) => empty_collection(&here),
        (scenes, _) => scenes.clone(),
    };
    ids::rename_sources(&mut value, renamed);
    if !replace {
        for part in ["scenes", "transitions"] {
            if let Some(v) = value.get_mut(part) {
                ids::refresh(v);
            }
        }
    }
    let text = serde_json::to_string(&value).unwrap_or_default();
    let incoming = Collection::from_json(&text).map_err(|e| {
        RpcError::invalid_params(format!(
            "the scenes in this file are damaged: {e:#}. Nothing was changed. Export the project again from the mixer it came from."
        ))
        .with("field", "file")
        .with("reason", "damaged")
    })?;
    for scene in &incoming.scenes {
        let clash = here.scenes.iter().any(|s| s.name == scene.name);
        let action = match (replace, clash) {
            (true, true) => "replace",
            (false, true) => "rename",
            _ => "add",
        };
        let mut change = Change::new("scene", &scene.name, action);
        if action == "rename" {
            change = change.to(&godwinmix_core::scene::server::find::free_scene_name(&here, &scene.name));
        }
        report.changes.push(change);
    }
    if replace {
        for gone in here.scenes.iter().filter(|s| !incoming.scenes.iter().any(|i| i.name == s.name)) {
            report.changes.push(Change::new("scene", &gone.name, "remove"));
        }
    }
    Ok(Some(incoming))
}

fn plan_channels(call: &Call, bundle: &Bundle, replace: bool, report: &mut Report) -> Result<Vec<Incoming>, RpcError> {
    let here = call.app.channels.project_ids();
    let mut placed: Vec<(String, String)> = Vec::new();
    let mut out = Vec::new();
    for value in &bundle.channels {
        let taken = |name: &str, what: &str| {
            let pick = |(id, app): &(String, String)| if what == "id" { id == name } else { app == name };
            placed.iter().any(pick) || (!replace && here.iter().any(pick))
        };
        let (incoming, was) = channel_file::read(value, &taken).map_err(|e| {
            RpcError::invalid_params(format!("{e}. Nothing was changed. Export the project again from the mixer it came from."))
                .with("field", "file")
                .with("reason", "damaged")
        })?;
        let id = incoming.record.id.clone();
        let change = match (&was, here.iter().any(|(h, _)| h == &id)) {
            (Some(old), _) => Change::new("channel", old, "rename").to(&id),
            (None, true) => Change::new("channel", &id, "replace"),
            (None, false) => Change::new("channel", &id, "add"),
        };
        report.changes.push(change);
        placed.push((id, incoming.record.app.clone()));
        out.push(incoming);
    }
    if replace {
        for (gone, _) in here.iter().filter(|(h, _)| !placed.iter().any(|(p, _)| p == h)) {
            report.changes.push(Change::new("channel", gone, "remove"));
        }
    }
    Ok(out)
}

fn plan_media(call: &Call, bundle: &Bundle, report: &mut Report) -> Vec<MediaEntry> {
    let dir = call.app.library.dir();
    let mut out = Vec::new();
    for entry in &bundle.media {
        let action = media::action(dir, entry);
        let mb = entry.size as f64 / 1_048_576.0;
        match action {
            "missing" => report.waiting.push(format!(
                "the clip {} ({mb:.1} MB) is not in this mixer's media folder: copy it into {}, or save the project again with media included",
                entry.name,
                dir.display()
            )),
            "add" => out.push(entry.clone()),
            _ => {}
        }
        let mut change = Change::new("media", &entry.name, action);
        if action == "skip" {
            change = change.note("a different clip of that name is already here, and it stays");
        }
        report.changes.push(change);
    }
    out
}

/// This mixer's collection with nothing in it.
fn empty_collection(here: &Collection) -> Value {
    let mut empty = here.clone();
    empty.scenes.clear();
    empty.transitions.clear();
    empty.assets.clear();
    empty.sources.clear();
    serde_json::to_value(empty).unwrap_or(Value::Null)
}
