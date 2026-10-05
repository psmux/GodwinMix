//! What a take sees of the fx library: a name it may use, and the plan the
//! mixer runs for it.

use super::*;
use godwinmix_core::mixer::transition::{Easing, Kind, TransitionSpec};
use godwinmix_protocol::requests::Transition;
use godwinmix_protocol::transitions::TransitionEntry;

/// The spec for a take that names an fx item, `None` for any other name.
///
/// A built in name or a running plugin's is never looked up here, so an
/// item cannot take `fade` away from the mixer. The take's own params on top
/// of the item's: `cut_at_ms` moves a clip's cut for this take, `easing`
/// shapes a matte or a shader.
pub fn spec_for(call: &Call, asked: &Transition, plugins: &[String]) -> Result<Option<TransitionSpec>, RpcError> {
    let name = asked.type_id();
    let built_in = godwinmix_protocol::requests::TRANSITIONS.contains(&name.as_str());
    if built_in || plugins.iter().any(|p| p.eq_ignore_ascii_case(&name)) {
        return Ok(None);
    }
    let Ok((m, folder)) = library::find(&media(call), &name) else { return Ok(None) };
    if !m.transition {
        return Err(RpcError::invalid_params(format!(
            "{} is an effect, not a transition: fire it with fx.fire {{\"name\": \"{}\"}}, or let takes use it with fx.set {{\"name\": \"{}\", \"transition\": true}}",
            m.name, m.name, m.name
        ))
        .with("name", json!(m.name)));
    }
    let asked_ms = asked.full().and_then(|r| r.duration_ms);
    let mut plan = Plan::of(&m, &folder, asked_ms).map_err(|e| RpcError::internal(format!("{e:#}")))?;
    if let (Some(ms), fx::Look::Clip { cut_at_ms, .. }) = (asked.param_u64("cut_at_ms"), &mut plan.look) {
        *cut_at_ms = ms.min(plan.duration_ms);
    }
    let easing = Easing::parse(asked.param_str("easing").as_deref());
    Ok(Some(TransitionSpec { duration_ms: plan.duration_ms, kind: Kind::Fx(Box::new(plan)), easing }))
}

/// Every fx item a take may name.
pub fn transition_names(call: &Call) -> Vec<String> {
    let (all, _) = library::list(&media(call));
    all.into_iter().filter(|(m, _)| m.transition).map(|(m, _)| m.name).collect()
}

/// The fx items as `program.transitions` lists them, origin `fx`.
pub fn catalogue(call: &Call) -> Vec<TransitionEntry> {
    let (all, _) = library::list(&media(call));
    all.into_iter()
        .filter(|(m, _)| m.transition)
        .map(|(m, _)| TransitionEntry {
            name: m.name.clone(),
            origin: "fx".to_string(),
            type_id: format!("{:?}", m.kind).to_lowercase(),
            params: match m.kind {
                FxKind::Stinger | FxKind::Overlay => vec!["cut_at_ms".to_string()],
                _ => vec!["easing".to_string()],
            },
            duration_ms: Some(m.duration_ms),
        })
        .collect()
}

/// The transition `fx.assign` set for this take, when it named none: its
/// scene's (the one named, or the armed one) and then the default. A take
/// of a bare source has no scene and gets the default.
pub fn assigned(call: &Call, req: &godwinmix_protocol::requests::TakeRequest) -> Option<Transition> {
    let scene = match (req.source_id(), req.scene_name()) {
        (Some(_), _) => None,
        (None, Some(name)) => Some(name),
        (None, None) => call.app.scenes.armed().and_then(|id| call.app.scenes.scene(&id.to_string()).ok()).map(|s| s.name),
    };
    fx::assign::for_scene(&media(call), scene.as_deref()).map(Transition::Named)
}
