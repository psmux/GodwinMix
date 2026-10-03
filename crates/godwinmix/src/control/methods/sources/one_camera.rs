//! One camera, one source, where the frame bus is not there to share it.
//!
//! With the bus a second source of a camera reads the first one's frames.
//! Without it (Windows today) the second source opened the camera again, the
//! platform would not let it, and the mixer spent five seconds of its own
//! thread finding that out: every other call waited, the add came back as
//! "the mixer is busy", and the source quietly went. This answers at once and
//! names the source that already has the camera.

use crate::control::call::Call;
use godwinmix_core::plugin::host::device_key as device;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::requests::AddSourceRequest;
use serde_json::Value;

pub(super) async fn refuse_a_second_open(call: &Call, req: &AddSourceRequest) -> Result<(), RpcError> {
    if device::shared_on_this_machine() {
        return Ok(());
    }
    let Some(type_id) = req.params.get("type").and_then(Value::as_str) else { return Ok(()) };
    let wanted = inner_params(&Value::Object(req.params.clone()));
    let Some(key) = device::device_key(type_id, &wanted) else { return Ok(()) };
    let label = label_of(&wanted);
    let configs = call.app.mixer.configs().await.map_err(|e| call.mixer_error(e))?;
    for s in configs.sources.iter().filter(|s| s.type_id.as_deref() == Some(type_id)) {
        let theirs = inner_params(&device::params_value(&s.params));
        let same_key = device::device_key(type_id, &theirs).as_deref() == Some(key.as_str());
        let same_label = !label.is_empty() && label_of(&theirs) == label;
        if same_key || same_label {
            let shown = wanted.get("label").or_else(|| wanted.get("device")).and_then(Value::as_str).unwrap_or("");
            let what = if shown.is_empty() { "this camera".to_string() } else { format!("the camera '{shown}'") };
            return Err(RpcError::not_in_state(format!(
                "{what} is already the source '{}', and on this machine one camera can feed only one \
                 source. Put '{}' in this scene instead, or remove it first.",
                s.id, s.id
            ))
            .with("source", s.id.clone())
            .with("device", key)
            // Asking again changes nothing; using the other source does.
            .with("retryable", false));
        }
    }
    Ok(())
}

/// The params a camera reads: under `params` when the request nests them, as
/// the API's own examples do, or at the top level when it does not.
fn inner_params(v: &Value) -> Value {
    match v.get("params") {
        Some(inner) if inner.is_object() => inner.clone(),
        _ => v.clone(),
    }
}

fn label_of(v: &Value) -> String {
    v.get("label").and_then(Value::as_str).unwrap_or("").trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nested_and_flat_params_are_read_alike() {
        let nested = json!({ "type": "camera/source", "params": { "device": "Cam", "label": "Cam" } });
        let flat = json!({ "type": "camera/source", "device": "Cam", "label": "Cam" });
        assert_eq!(inner_params(&nested)["device"], "Cam");
        assert_eq!(inner_params(&flat)["device"], "Cam");
        assert_eq!(label_of(&inner_params(&nested)), "cam");
    }
}
