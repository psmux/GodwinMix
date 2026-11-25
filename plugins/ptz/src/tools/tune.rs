//! `track_tune`: read or change how a camera follows, live. With no numbers
//! it answers with the tuning as it stands, so an agent can look first.

use godwinmix_sdk::prelude::{codes, RpcError, ToolResult};
use serde_json::{json, Value};

use super::answer;
use super::args::{camera, invalid};
use crate::follow::{tune::FIELDS, Ctl, Loop};
use crate::rig::Rig;

pub fn tune(rig: &mut Rig, a: &Value) -> Result<ToolResult, RpcError> {
    let data_dir = rig.settings.data_dir.clone();
    let cam = camera(rig, a)?;
    let id = cam.spec.id.clone();
    let changed = cam.tuning.merge(a).map_err(invalid)?;
    if changed.is_empty() {
        let list: Vec<String> = FIELDS.iter().map(|f| format!("{} ({} to {}): {}", f.0, f.1, f.2, f.3)).collect();
        return Ok(answer(format!("'{id}' follows with these numbers. Set any of: {}; and fallback: stay, wide or preset:<n>.", list.join("; ")), json!({"camera": id, "tuning": cam.tuning.to_json(), "changed": []})));
    }
    cam.tuning.save(&data_dir, &id).map_err(|why| RpcError::new(codes::WRONG_STATE, format!("'{id}' took the new tuning but could not keep it for the next start: {why}")))?;
    let live = cam.follow.as_ref().filter(|l| l.is_running());
    if let Some(l) = live {
        l.send(Ctl::Tune(cam.tuning.clone()));
    }
    let when = if live.is_some_and(Loop::is_running) { "now" } else { "the next time it tracks" };
    Ok(answer(format!("'{id}' changed {}; it follows with them {when}.", changed.join(", ")), json!({"camera": id, "tuning": cam.tuning.to_json(), "changed": changed})))
}
