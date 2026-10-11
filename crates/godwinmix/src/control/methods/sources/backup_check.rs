//! The checks `source.on_loss.set` makes before the mixer sees the rule:
//! the backup exists, no loop, and a transition this core can run.

use crate::control::call::Call;
use godwinmix_protocol::backup::OnLoss;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::requests::AddSourceRequest;
use godwinmix_protocol::types::SourceStatus;
use serde_json::json;

/// The rule against every other one and against the transitions there are.
pub(super) fn check(call: &Call, sources: &[SourceStatus], id: &str, rule: &OnLoss) -> Result<(), RpcError> {
    let ids: Vec<&str> = sources.iter().map(|s| s.id.as_str()).collect();
    let mut rules: Vec<(&str, &OnLoss)> = sources
        .iter()
        .filter(|s| s.id != id)
        .filter_map(|s| Some((s.id.as_str(), s.on_loss.as_ref()?)))
        .collect();
    rules.push((id, rule));
    godwinmix_protocol::backup::check(&rules, &ids).map_err(|p| {
        p.data.into_iter().fold(RpcError::invalid_params(p.message), |e, (k, v)| e.with(&k, v))
    })?;
    let plugins = call.app.transition_names();
    let name = rule.transition_name();
    if !godwinmix_protocol::backup::known_transition(&name) && !plugins.iter().any(|p| p.eq_ignore_ascii_case(&name)) {
        let mut names: Vec<String> = godwinmix_protocol::requests::TRANSITIONS.iter().map(|s| s.to_string()).collect();
        names.retain(|n| n != "stinger" && n != "move");
        names.extend(plugins);
        return Err(RpcError::invalid_params(format!(
            "this core has no transition called {name:?}. Use one of: {}.",
            names.join(", ")
        ))
        .with("transitions", json!(names)));
    }
    Ok(())
}

/// `source.add` with `on_loss` in it: the same checks, before anything is
/// built, so a source never arrives with a rule the mixer would refuse.
pub(super) async fn check_add(call: &Call, req: &AddSourceRequest) -> Result<(), RpcError> {
    let Some(raw) = req.params.get("on_loss") else { return Ok(()) };
    let rule: OnLoss = serde_json::from_value(raw.clone()).map_err(|e| {
        RpcError::invalid_params(format!(
            "on_loss is not a backup rule: {e}. Write {{\"backup\": \"cam-wide\", \"after_secs\": 3}}."
        ))
    })?;
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    let id = req.id.clone().unwrap_or_else(|| "the new source".into());
    check(call, &status.sources, &id, &rule)
}
