//! `source.missing` and `source.restart`: what a scene lacks, and why.
//!
//! A scene can name a source that is not running. The take goes ahead
//! without it (see `program/missing.rs`); these two are what lets a page say
//! which ones, give the reason in the source's own words, and offer the
//! button that brings each back.

use super::super::{body, handler};
use crate::control::call::Call;
use godwinmix_core::mixer::{Command, RuntimeConfigs};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::requests::*;
use godwinmix_protocol::scope::Scope;
use godwinmix_protocol::types::{SourceState, SourceStatus};
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "source.missing",
            Scope::Read,
            "Sources that are not running, and why: failed, could not be started (with the \
             error and the action that fixes it), removed, or unknown. Pass the ids a scene \
             draws, or none for every one the mixer knows about.",
            handler(missing),
        )
        .params(schema_of::<MissingRequest>)
        .result(schema_of::<Vec<MissingSource>>),
    );

    reg.register(
        MethodDef::new(
            "source.restart",
            Scope::Operate,
            "Build a source's pipeline again now, rather than waiting for its next retry. \
             For a source that could not be started or was removed, use source.restore.",
            handler(restart),
        )
        .params(schema_of::<IdRequest>)
        .result(schema_of::<SourceStatus>),
    );
}

async fn missing(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: MissingRequest = call.params(&params)?;
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    let configs = call.app.mixer.configs().await.map_err(|e| call.mixer_error(e))?;
    body(report(&req.ids, &status.sources, &configs))
}

/// One entry an asked for id that is not live, in the order asked; with no
/// ids, every failed, unstarted and removed source.
pub fn report(ids: &[String], here: &[SourceStatus], configs: &RuntimeConfigs) -> Vec<MissingSource> {
    let asked: Vec<String> = if ids.is_empty() {
        let mut all: Vec<String> =
            here.iter().filter(|s| s.state == SourceState::Failed).map(|s| s.id.clone()).collect();
        all.extend(configs.unstarted.iter().map(|u| u.config.id.clone()));
        all.extend(configs.removed.iter().map(|c| c.id.clone()));
        all.dedup();
        all
    } else {
        ids.to_vec()
    };
    asked.iter().filter_map(|id| one(id, here, configs)).collect()
}

fn one(id: &str, here: &[SourceStatus], configs: &RuntimeConfigs) -> Option<MissingSource> {
    if let Some(s) = here.iter().find(|s| s.id == id) {
        if s.state != SourceState::Failed {
            return None;
        }
        let kind = s.extra.get("type").and_then(Value::as_str).map(str::to_string);
        return Some(entry(id, Some(s.name.clone()), kind, MissingWhy::Failed, false));
    }
    if let Some(u) = configs.unstarted.iter().find(|u| u.config.id == id) {
        let mut e = entry(id, u.config.name.clone(), u.config.type_id.clone(), MissingWhy::NotStarted, true);
        e.error = Some(u.error.clone());
        e.action = u.action.clone();
        return Some(e);
    }
    if let Some(c) = configs.removed.iter().rev().find(|c| c.id == id) {
        return Some(entry(id, c.name.clone(), c.type_id.clone(), MissingWhy::Removed, true));
    }
    Some(entry(id, None, None, MissingWhy::Unknown, false))
}

fn entry(id: &str, name: Option<String>, kind: Option<String>, why: MissingWhy, restore: bool) -> MissingSource {
    MissingSource { id: id.to_string(), name, type_id: kind, why, error: None, action: None, restore }
}

async fn restart(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: IdRequest = call.params(&params)?;
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    let Some(source) = status.sources.iter().find(|s| s.id == req.id).cloned() else {
        let ids: Vec<String> = status.sources.iter().map(|s| s.id.clone()).collect();
        return Err(RpcError::not_found("source", &req.id, &ids)
            .with("restore", "source.restore puts back one that was removed or never started"));
    };
    call.app.mixer.send(Command::RestartSource(req.id.clone())).map_err(|e| call.mixer_error(e))?;
    body(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_core::config::SourceConfig;
    use godwinmix_core::mixer::unstarted::UnstartedList;

    fn config(id: &str) -> SourceConfig {
        toml::from_str(&format!("id = \"{id}\"\nname = \"{id} name\"\nuri = \"media/{id}.mp4\"\n"))
            .expect("a source")
    }

    #[test]
    fn each_reason_is_told_apart_and_a_live_source_is_left_out() {
        let mut unstarted = UnstartedList::default();
        unstarted.note(&config("slides"), &anyhow::anyhow!("there is no file at media/slides.mp4"));
        let configs = RuntimeConfigs {
            removed: vec![config("cam2")],
            unstarted: unstarted.entries().to_vec(),
            ..Default::default()
        };
        let ids: Vec<String> = ["slides", "cam2", "ghost"].iter().map(|s| s.to_string()).collect();
        let found = report(&ids, &[], &configs);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].why, MissingWhy::NotStarted);
        assert!(found[0].error.as_deref().unwrap_or_default().contains("slides.mp4"));
        assert!(found[0].restore);
        assert_eq!(found[1].why, MissingWhy::Removed);
        assert_eq!(found[1].name.as_deref(), Some("cam2 name"));
        assert_eq!(found[2].why, MissingWhy::Unknown);
        assert!(!found[2].restore);

        let everything = report(&[], &[], &configs);
        assert_eq!(everything.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["slides", "cam2"]);
    }
}
