//! Doing what the plan says, through the same doors every other change uses.
//!
//! Settings go through `config.set`, so the config file keeps its comments and
//! a live key applies at once. Sources and outputs go to the mixer as the
//! commands `source.add` and `output.add` send, as `preset.apply` does. Scenes
//! go through the scene server's `edit`, so every page gets the patch and the
//! change can be undone. Channels go into their table and the listener is
//! handed the new one. Nothing here restarts the encoder.

use super::plan::Plan;
use super::{invoke, media, Report};
use crate::control::call::Call;
use crate::control::methods::scenes::client;
use godwinmix_core::mixer::Command;
use godwinmix_core::scene::document::Collection;
use godwinmix_core::scene::server::find::free_scene_name;
use godwinmix_protocol::error::RpcError;
use serde_json::{json, Value};

/// Check the settings the plan writes, writing nothing. A value this mixer
/// would refuse refuses the whole file, before anything has moved.
pub async fn check_settings(call: &Call, plan: &Plan) -> Result<(), RpcError> {
    if plan.settings.is_empty() || call.app.config_path.as_os_str().is_empty() {
        return Ok(());
    }
    invoke(call, "config.set", json!({"values": plan.settings, "dry_run": true})).await.map(|_| ())
}

pub async fn run(call: &Call, plan: Plan, replace: bool, report: &mut Report) -> Result<(), RpcError> {
    write_settings(call, &plan, report).await?;
    for id in plan.sources.remove {
        if let Err(e) = call.app.mixer.request(|ack| Command::RemoveSource(id.clone(), Some(ack))).await {
            report.failed.push(format!("source {id} could not be taken out: {e}"));
        }
    }
    for id in plan.outputs.remove {
        if let Err(e) = call.app.mixer.request(|ack| Command::RemoveOutput(id.clone(), Some(ack))).await {
            report.failed.push(format!("output {id} could not be taken out: {e}"));
        }
    }
    for source in plan.sources.add {
        let id = source.id.clone();
        if let Err(e) = call.app.mixer.request(|ack| Command::AddSource(Box::new(source), Some(ack))).await {
            report.failed.push(format!("source {id} did not start: {e}"));
        }
    }
    for output in plan.outputs.add {
        let id = output.id.clone();
        if call.app.rehearsal {
            report.waiting.push(format!("output {id} was not started, because this is a rehearsal core"));
            continue;
        }
        if let Err(e) = call.app.mixer.request(|ack| Command::AddOutput(Box::new(output), Some(ack))).await {
            report.failed.push(format!("output {id} did not start: {e}"));
        }
    }
    if let Some(incoming) = plan.scenes {
        put_scenes(call, incoming, replace)?;
    }
    let waiting = call.app.channels.project_import(plan.channels, replace)?;
    report.waiting.extend(waiting);
    write_media(call, plan.media, report).await;
    Ok(())
}

async fn write_settings(call: &Call, plan: &Plan, report: &mut Report) -> Result<(), RpcError> {
    if plan.settings.is_empty() {
        return Ok(());
    }
    if call.app.config_path.as_os_str().is_empty() {
        report.waiting.push("this mixer has no config file, so the project's settings were not written".into());
        return Ok(());
    }
    let answer = invoke(call, "config.set", json!({"values": plan.settings})).await?;
    for key in answer["needs_restart"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        report.needs_restart.push(format!("{key} is written to the config file and takes effect when the mixer restarts"));
    }
    Ok(())
}

/// Replace: the file's collection becomes this one, keeping the canvas the
/// mixer runs at. Merge: its scenes are added beside these, renamed where a
/// name is taken, with its assets, transitions and labels where they are new.
fn put_scenes(call: &Call, incoming: Collection, replace: bool) -> Result<(), RpcError> {
    call.app
        .scenes
        .edit(client(call).as_deref(), move |doc| {
            if replace {
                doc.scenes = incoming.scenes;
                doc.transitions = incoming.transitions;
                doc.assets = incoming.assets;
                doc.sources = incoming.sources;
                doc.params = incoming.params;
                return Ok(());
            }
            for mut scene in incoming.scenes {
                scene.name = free_scene_name(doc, &scene.name);
                doc.scenes.push(scene);
            }
            for t in incoming.transitions {
                if !doc.transitions.iter().any(|d| d.id == t.id) {
                    doc.transitions.push(t);
                }
            }
            for (id, asset) in incoming.assets {
                doc.assets.entry(id).or_insert(asset);
            }
            for (id, label) in incoming.sources {
                doc.sources.entry(id).or_insert(label);
            }
            Ok(())
        })
        .map(|_| ())
        .map_err(|e| RpcError::not_in_state(format!("the scenes could not be put in: {e:#}")).with("part", "scenes"))
}

async fn write_media(call: &Call, entries: Vec<super::bundle::MediaEntry>, report: &mut Report) {
    if entries.is_empty() {
        return;
    }
    let dir = call.app.library.dir().to_path_buf();
    let failed = tokio::task::spawn_blocking(move || {
        entries.iter().filter_map(|e| media::write(&dir, e).err()).collect::<Vec<_>>()
    })
    .await
    .unwrap_or_else(|e| vec![format!("writing the clips stopped: {e}")]);
    report.failed.extend(failed);
}
