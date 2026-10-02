//! `source.set` with new params, taken by the running source when its kind
//! can take them in place.
//!
//! A text, a ticker and anything else whose `configure` answers `Applied`
//! changes on air with no rebuild and no gap. A kind that cannot answers
//! `RestartRequired`, nothing has changed, and the caller rebuilds the source
//! the way it always did.

use crate::control::call::Call;
use godwinmix_core::config::SourceConfig;
use godwinmix_core::mixer::Command;
use godwinmix_core::plugin::Configure;
use godwinmix_protocol::error::RpcError;

/// What came of offering the params to the running source.
pub(super) enum Offered {
    /// Taken in place. The name, if one was given, is changed too.
    Applied,
    /// The kind has to be built again to take them.
    Rebuild,
}

/// Offer `wanted.params` to the running source `id`.
pub(super) async fn offer(call: &Call, id: &str, wanted: &SourceConfig, name: Option<String>) -> Result<Offered, RpcError> {
    let outcome = call.app.mixer.configure_source(id.to_string(), wanted.params.clone()).await;
    match outcome {
        Ok(Configure::Applied) => {
            if let Some(name) = name {
                let _ = call.app.mixer.request(|ack| Command::RenameSource(id.to_string(), name, Some(ack))).await;
            }
            Ok(Offered::Applied)
        }
        Ok(Configure::RestartRequired(_)) => Ok(Offered::Rebuild),
        // A source the mixer is not running cannot be changed in place; the
        // rebuild below is how it gets its new params.
        Err(e) if e.to_string().contains("there is no source") => Ok(Offered::Rebuild),
        Err(e) => Err(RpcError::invalid_params(format!(
            "{e}. The source is unchanged and still on air as it was; send the params again with that fixed."
        ))
        .with("id", id)
        .with("params", serde_json::to_value(&wanted.params).unwrap_or_default())),
    }
}
