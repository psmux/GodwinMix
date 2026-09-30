//! A scene that draws sources this mixer does not have.
//!
//! A take used to be refused for it, which kept a whole service off air over
//! one camera that was not plugged in. The compositor already skips a
//! placement it has no source for, so the take goes ahead with what is here,
//! the missing items draw nothing, and the answer and an alert name them. The
//! one scene still refused is one where nothing at all is here: taking it
//! would swap whatever is on air for the slate, which nobody asks for by
//! taking a scene, and the slate has its own take for when they do.

use crate::control::call::Call;
use godwinmix_core::mixer::slots::Placement;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::types::{Event, Severity};
use serde_json::json;

/// Every source a scene's visible placements draw, groups included, sorted.
pub fn drawn(placements: &[Placement]) -> Vec<String> {
    let mut out = Vec::new();
    collect(placements, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect(placements: &[Placement], out: &mut Vec<String>) {
    for p in placements.iter().filter(|p| p.alpha > 0.0) {
        if p.group.is_empty() {
            out.push(p.source.to_string());
        } else {
            collect(&p.group, out);
        }
    }
}

/// The drawn sources that are not among `here`.
pub fn absent(drawn: &[String], here: &[String]) -> Vec<String> {
    drawn.iter().filter(|s| !here.contains(s)).cloned().collect()
}

/// The sources a named scene draws that are not here, or none when the name
/// is not a scene (a source id taken as shorthand is always here).
pub fn in_scene(call: &Call, scene: &str, here: &[String]) -> Vec<String> {
    match call.app.scenes.placements(scene) {
        Ok((_, placements)) => absent(&drawn(&placements), here),
        Err(_) => Vec::new(),
    }
}

/// The refusal for a scene with nothing here at all, or `None` when there
/// is something to put on air.
pub fn refusal(name: &str, drawn: &[String], missing: &[String], here: &[String]) -> Option<RpcError> {
    if drawn.is_empty() || missing.len() < drawn.len() {
        return None;
    }
    Some(
        RpcError::new(
            ErrorCode::NotFound,
            format!(
                "none of the sources the scene {name:?} draws is running ({}), so taking it \
                 would put only the slate on air, and the programme is left as it was. Add \
                 or restore one of them, or take a scene that has a source here. Sources \
                 here: {}",
                missing.join(", "),
                if here.is_empty() { "none".into() } else { here.join(", ") }
            ),
        )
        .with("missing", json!(missing))
        .with("scene", name),
    )
}

/// The names a person gave the missing sources, where the mixer remembers
/// them (one that could not start, or was removed), else their ids.
pub async fn names(call: &Call, missing: &[String]) -> Vec<String> {
    let Ok(configs) = call.app.mixer.configs().await else { return missing.to_vec() };
    let named = |id: &String| {
        let unstarted = configs.unstarted.iter().map(|u| &u.config);
        unstarted.chain(configs.removed.iter()).find(|c| &c.id == id).and_then(|c| c.name.clone())
    };
    missing.iter().map(|id| named(id).unwrap_or_else(|| id.clone())).collect()
}

/// Tell every client that a scene went to air with holes in it.
pub fn alert(call: &Call, name: &str, missing: &[String]) {
    if missing.is_empty() {
        return;
    }
    let one = missing.len() == 1;
    let (they, are, them) = if one { ("It", "is", "it") } else { ("They", "are", "them") };
    call.app.mixer.emit(Event::Alert {
        severity: Severity::Warning,
        message: format!(
            "{name} went to air without {}. {they} {are} in the scene but not running, so \
             nothing is drawn there. Start {them}, or remove {them} from the scene.",
            missing.join(", ")
        ),
        action: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_scene_with_one_source_here_is_taken() {
        let drawn = ids(&["cam1", "cam2", "slides"]);
        let missing = absent(&drawn, &ids(&["cam1"]));
        assert_eq!(missing, ids(&["cam2", "slides"]));
        assert!(refusal("Default", &drawn, &missing, &ids(&["cam1"])).is_none());
    }

    #[test]
    fn a_scene_with_nothing_here_is_refused_with_the_names() {
        let drawn = ids(&["cam2", "slides"]);
        let missing = absent(&drawn, &ids(&["cam1"]));
        let e = refusal("Default", &drawn, &missing, &ids(&["cam1"])).expect("a refusal");
        assert_eq!(e.data["missing"], json!(["cam2", "slides"]));
        assert!(e.message.contains("cam2, slides"), "{}", e.message);
        assert!(e.message.contains("Sources here: cam1"), "{}", e.message);
    }

    #[test]
    fn an_empty_scene_is_not_refused() {
        assert!(refusal("Empty", &[], &[], &ids(&["cam1"])).is_none());
    }
}
