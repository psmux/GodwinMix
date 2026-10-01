//! A direct show's outputs, sealed and unsealed.
//!
//! The record in the list of shows says what a list shows. The address and
//! the key are sealed in the secret store under `show.<id>.output`, the way
//! a channel's destinations are, and unsealed only to build the host's
//! table.

use crate::station::registry::OutputRecord;
use godwinmix_protocol::destination::{uri_host, StoredDestination};
use godwinmix_protocol::error::RpcError;
use serde::{Deserialize, Serialize};

/// Where a show's output addresses and keys are sealed.
pub fn scope(show: &str) -> String {
    format!("show.{show}.output")
}

#[derive(Default, Serialize, Deserialize)]
struct Sealed {
    server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

fn secrets() -> &'static godwinmix_core::secrets::Secrets {
    crate::control::methods::plugins::secrets()
}

/// The outputs with their addresses and keys unsealed.
pub fn stored(show: &str, records: &[OutputRecord]) -> Vec<StoredDestination> {
    records
        .iter()
        .map(|o| {
            let sealed: Sealed = secrets().get(&scope(show), &o.id).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            StoredDestination {
                id: o.id.clone(),
                platform: o.platform.clone(),
                label: o.label.clone(),
                server: sealed.server,
                key: sealed.key,
                stream: "main".into(),
                enabled: o.enabled,
                rendition: o.rendition.clone(),
            }
        })
        .collect()
}

/// Seal what changed, forget what went, and answer the records to keep.
pub fn seal(show: &str, before: &[StoredDestination], after: &[StoredDestination]) -> Result<Vec<OutputRecord>, RpcError> {
    let scope = scope(show);
    for gone in before.iter().filter(|b| !after.iter().any(|a| a.id == b.id)) {
        let _ = secrets().set(&scope, &gone.id, "");
    }
    after
        .iter()
        .map(|d| {
            let was = before.iter().find(|b| b.id == d.id);
            if was.is_none_or(|b| b.server != d.server || b.key != d.key) {
                let text = serde_json::to_string(&Sealed { server: d.server.clone(), key: d.key.clone() }).unwrap_or_default();
                secrets().set(&scope, &d.id, &text).map_err(|e| RpcError::internal(format!("sealing an output: {e:#}")))?;
            }
            Ok(record_of(d))
        })
        .collect()
}

/// What the list keeps of one output.
pub fn record_of(d: &StoredDestination) -> OutputRecord {
    OutputRecord {
        id: d.id.clone(),
        platform: d.platform.clone(),
        label: d.label.clone(),
        uri_host: uri_host(&d.server),
        has_key: d.has_key(),
        enabled: d.enabled,
        rendition: d.rendition.clone(),
    }
}

/// Forget every sealed output of a show that went.
pub fn forget(show: &str) {
    secrets().forget(&scope(show));
}
