//! `/<name>/preview.jpg`: the show's vitals picture, which the direct host
//! decodes from keyframes only while somebody asks for one.

use super::state::Headend;
use base64::Engine;
use serde_json::{json, Value};
use std::time::Duration;

/// The JPEG, or why there is none yet.
pub async fn jpeg(h: &Headend, show: &str) -> Result<Vec<u8>, String> {
    let Some(plugins) = h.st.direct.plugins().cloned().filter(|p| p.is_running(crate::channels::PLUGIN)) else {
        return Err("the ingest plugin, which runs the streams, is not running".into());
    };
    let args = json!({"name": "direct.thumbnail", "arguments": {"show": show, "width": 640}});
    let asked = tokio::task::spawn_blocking(move || plugins.call_provide(crate::channels::PLUGIN, "discover", "tool.call", args));
    let answer: Value = match tokio::time::timeout(Duration::from_secs(3), asked).await {
        Ok(Ok(Ok(v))) => v,
        Ok(Ok(Err(e))) => return Err(format!("the direct host said {e:#}")),
        _ => return Err("the direct host did not answer in time".into()),
    };
    match answer["jpeg"].as_str() {
        Some(b64) => base64::engine::general_purpose::STANDARD.decode(b64).map_err(|_| "the picture would not decode".to_string()),
        None => Err("the first picture is on its way; ask again in a second".into()),
    }
}
