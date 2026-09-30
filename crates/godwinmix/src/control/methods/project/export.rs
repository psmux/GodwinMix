//! `project.export`: the running mixer, as one file.
//!
//! Sources and outputs come from the mixer as it is now (what the runtime
//! store is written from), not from the config file, because a source added
//! in the page lives in the store. The scenes are the live collection. The
//! settings are what the config file writes.

use super::bundle::Bundle;
use super::{media, redact, settings};
use crate::control::call::Call;
use godwinmix_protocol::error::RpcError;
use serde_json::Value;

/// What the export was asked for.
pub struct Ask {
    pub name: String,
    pub secrets: bool,
    pub media: bool,
    pub page: Value,
}

/// Build the whole file.
pub async fn build(call: &Call, ask: Ask) -> Result<Bundle, RpcError> {
    let mut bundle = Bundle::new(&ask.name, ask.secrets);
    let (show, machine) = settings::read(&call.app.config_path, ask.secrets);
    bundle.settings = show;
    bundle.machine = machine;

    let configs = call.app.mixer.configs().await.map_err(|e| call.mixer_error(e))?;
    // A source that could not start here is still part of the show: a scene
    // draws it, and the next machine may have what it needs.
    let waiting = configs.unstarted.iter().map(|u| &u.config).filter(|c| !configs.sources.iter().any(|s| s.id == c.id));
    for source in configs.sources.iter().chain(waiting) {
        let mut value = serde_json::to_value(source).map_err(encode)?;
        if !ask.secrets {
            note(&mut bundle.removed, "source", &source.id, redact::entry(&mut value));
        }
        bundle.sources.push(value);
    }
    for output in &configs.outputs {
        let mut value = serde_json::to_value(output).map_err(encode)?;
        if !ask.secrets {
            note(&mut bundle.removed, "output", &output.id, redact::entry(&mut value));
        }
        bundle.outputs.push(value);
    }

    bundle.channels = call.app.channels.project_export(ask.secrets);
    if !ask.secrets {
        for channel in &bundle.channels {
            let id = channel["id"].as_str().unwrap_or_default();
            bundle.removed.push(format!("channel {id}: its keys and its destinations' addresses"));
        }
    }
    bundle.scenes = serde_json::to_value(call.app.scenes.document()).map_err(encode)?;
    bundle.page = ask.page;

    let dir = call.app.library.dir().to_path_buf();
    let depth = call.app.library.cfg().max_depth;
    let with_bytes = ask.media;
    bundle.media = tokio::task::spawn_blocking(move || {
        let mut found = media::list(&dir, depth);
        if with_bytes {
            media::include(&dir, &mut found)?;
        }
        Ok::<_, RpcError>(found)
    })
    .await
    .map_err(|e| RpcError::internal(format!("listing the media folder stopped: {e}")))??;
    Ok(bundle)
}

fn note(removed: &mut Vec<String>, kind: &str, id: &str, taken: Vec<String>) {
    if !taken.is_empty() {
        removed.push(format!("{kind} {id}: {}", taken.join(", ")));
    }
}

fn encode(e: serde_json::Error) -> RpcError {
    RpcError::internal(format!("writing the project: {e}"))
}
