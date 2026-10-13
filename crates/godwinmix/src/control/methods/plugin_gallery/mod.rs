//! `plugin.gallery`: every plugin every marketplace lists, for the Plugins
//! page, with what is installed here and whether each one runs here.
//!
//! The first open on a fresh mixer adds the community index (see
//! `godwinmix_host::marketplace::gallery`), and every open after reads a copy
//! no older than ten minutes. Nothing is fetched until somebody opens the
//! page. Pictures are URLs the browser loads from where they live; the core
//! never fetches one.

mod entries;
mod types;

pub use types::*;

use super::{body, handler};
use crate::control::call::Call;
use entries::{entries, Here};
use godwinmix_core::plugin::{first_party, loader};
use godwinmix_host::marketplace::{self, gallery};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "plugin.gallery",
            Scope::Read,
            "Every plugin the marketplaces list, once each, with its pictures, stars, \
             downloads and tier, whether it is installed here, and whether it runs on this \
             machine. Install one by passing its `install` to plugin.add. The community index \
             is read by default; a copy younger than ten minutes is used as it is, and the \
             last copy is used when the network is not there.",
            handler(answer),
        )
        .params(schema_of::<GalleryRequest>)
        .result(schema_of::<Gallery>),
    );
}

async fn answer(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryRequest = call.params(&params).unwrap_or_default();
    let only = call.app.marketplaces_only.clone();
    let gallery = tokio::task::spawn_blocking(move || build(&only, req.refresh))
        .await
        .map_err(|e| RpcError::internal(format!("reading the marketplaces did not finish: {e}")))?;
    body(gallery)
}

/// Fetch what is due, then read every copy on disk.
fn build(only: &[String], refresh: bool) -> Gallery {
    let ttl = Some(if refresh { 0 } else { gallery::TTL_SECS });
    let fresh = gallery::refresh_now(only, ttl);
    let docs = marketplace::documents(only);
    let installed: Vec<(String, String)> = loader::list()
        .iter()
        .map(|p| (p.name().to_string(), p.version().to_string()))
        .collect();
    let platform = godwinmix_host::launch::this_platform();
    let shipped = |name: &str| first_party::find(name).is_some();
    let here = Here { platform, installed: &installed, shipped: &shipped };
    Gallery {
        platform: platform.to_string(),
        offline: fresh.iter().any(|f| f.problem.is_some()),
        marketplaces: fresh
            .into_iter()
            .map(|f| GalleryMarketplace {
                title: docs.iter().find(|d| d.name == f.name).map(|d| d.title.clone()).unwrap_or_default(),
                name: f.name,
                source: f.source,
                fetched: f.fetched,
                problem: f.problem,
            })
            .collect(),
        plugins: entries(&docs, &here),
    }
}

#[cfg(test)]
mod tests;
