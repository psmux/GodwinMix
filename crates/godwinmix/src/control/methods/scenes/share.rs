//! `scene.export` and `scene.import`: a collection as a thing you can send
//! somebody.
//!
//! The work is `godwinmix_core::scene::collection`, which is pure. What is here
//! is what it needs from a running core: where the assets resolve from, what
//! plugin versions it was built against, and putting the imported scenes into
//! the live document through the same `edit` every other command uses, so the
//! patch reaches every client.
//!
//! An export that writes to a `path` answers with the path. One that does not
//! answers with the bytes, base64, because a client on the other side of a
//! WebSocket has no filesystem in common with the core. A church laptop's
//! collection is tens of kilobytes; a bundle with video in it should be written
//! to a path and copied, and the message says so.

use super::{client, scene_error, server};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::scene::collection::{self, Options};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Past this, a bundle is handed back as a path and not as base64 in a JSON-RPC
/// answer. Eight megabytes of base64 is eleven on the wire and no client wants
/// it in a log.
const INLINE_LIMIT: usize = 8 * 1024 * 1024;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.import",
            Scope::Operate,
            "Read a collection bundle, a zip or the directory it unpacks to, and add its \
             scenes to this one. Answers with a relink report for any asset that did not \
             come across.",
            handler(import),
        )
        .params(schema_of::<ImportRequest>)
        .result(schema_of::<ImportedReport>)
        .tool(
            "import_collection",
            Tier::Search,
            "Add somebody else's scenes to this mixer from a collection bundle: the .zip \
             that `scene.export` wrote, or the folder it unpacks to, as a path on the \
             machine the mixer is running on. Answers with what came across and what has \
             to be relinked. To read an OBS collection instead, use scene.import.obs.",
        ),
    );
}

/// `scene.export`, registered beside the other `scene.*` rows in `scenes.rs`
/// because it was there before this module was.
pub async fn export(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ExportRequest = call.params(&params)?;
    let doc = server(&call).document();
    let format = req.format.as_deref().unwrap_or("json").trim().to_ascii_lowercase();
    if format == "json" {
        return body(doc);
    }
    let options = Options { root: collection_root(&call), versions: installed_versions() };
    match format.as_str() {
        "dir" | "directory" => {
            let to = PathBuf::from(req.path.as_deref().ok_or_else(|| {
                RpcError::invalid_params(
                    "scene.export with format \"dir\" needs a `path`: the directory to \
                     write the collection into, on the machine the mixer is running on.",
                )
            })?);
            let bundle = collection::export_dir(&doc, &to, &options)
                .map_err(|e| scene_error(&call, e))?;
            Ok(json!({
                "format": "dir",
                "path": to.display().to_string(),
                "bundle": bundle,
            }))
        }
        "zip" => {
            let (bundle, bytes) =
                collection::export_zip(&doc, &options).map_err(|e| scene_error(&call, e))?;
            match &req.path {
                Some(path) => {
                    std::fs::write(path, &bytes).map_err(|e| {
                        RpcError::new(
                            ErrorCode::NotInState,
                            format!("could not write {path}: {e}"),
                        )
                    })?;
                    Ok(json!({ "format": "zip", "path": path, "size": bytes.len(), "bundle": bundle }))
                }
                None if bytes.len() > INLINE_LIMIT => Err(RpcError::invalid_params(format!(
                    "this collection is {} bytes, too big to hand back in one answer. \
                     Give a `path` and the mixer will write the zip there.",
                    bytes.len()
                ))),
                None => {
                    use base64::Engine;
                    Ok(json!({
                        "format": "zip",
                        "encoding": "base64",
                        "size": bytes.len(),
                        "zip": base64::engine::general_purpose::STANDARD.encode(&bytes),
                        "bundle": bundle,
                    }))
                }
            }
        }
        other => Err(RpcError::invalid_params(format!(
            "scene.export writes \"json\" (the document), \"zip\" (a bundle with its \
             assets) or \"dir\" (the same, unpacked). It cannot write {other:?}."
        ))),
    }
}

async fn import(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ImportRequest = call.params(&params)?;
    let at = PathBuf::from(&req.path);
    let assets_to = collection_root(&call);
    let imported = collection::import(&at, assets_to.as_deref()).map_err(|e| {
        let text = format!("{e:#}");
        let code = if text.contains("no such") || text.contains("reading") {
            ErrorCode::NotFound
        } else {
            ErrorCode::NotInState
        };
        RpcError::new(code, text).with("method", call.method)
    })?;

    // The canvas the core is running at wins, as it does when a collection is
    // loaded off disk: a bundle authored at 1080p opened on a 720p core is the
    // ordinary case and the layouts resolve against the running canvas.
    let incoming = imported.document.clone();
    let (added, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| {
            let mut names = Vec::new();
            for scene in &incoming.scenes {
                let mut scene = scene.clone();
                scene.name = godwinmix_core::scene::server::find::free_scene_name(doc, &scene.name);
                names.push(scene.name.clone());
                doc.scenes.push(scene);
            }
            // The assets come with the scenes, or a relink report names them.
            for (id, asset) in &incoming.assets {
                doc.assets.entry(*id).or_insert_with(|| asset.clone());
            }
            for transition in &incoming.transitions {
                if !doc.transitions.iter().any(|t| t.id == transition.id) {
                    doc.transitions.push(transition.clone());
                }
            }
            Ok(names)
        })
        .map_err(|e| scene_error(&call, e))?;

    let missing = missing_plugins(&imported.bundle);
    body(ImportedReport {
        scenes: added,
        items: incoming.scenes.iter().map(|s| s.walk().len()).sum(),
        bundle: imported.bundle,
        relink: imported.relink,
        missing_plugins: missing,
        assets_at: imported.assets_at.map(|p| p.display().to_string()),
    })
}

/// The plugins the bundle needs that this core has not got.
fn missing_plugins(bundle: &collection::Bundle) -> Vec<String> {
    bundle
        .requires
        .iter()
        .filter(|r| {
            godwinmix_core::plugin::loader::get(&r.plugin).is_none()
                && godwinmix_core::plugin::loader::provide_manifest(&r.plugin).is_none()
        })
        .map(|r| format!("{} {}", r.plugin, r.versions))
        .collect()
}

/// Where the collection's relative asset paths resolve from: the directory the
/// scene document is saved in. A core with no runtime store has none, and an
/// export of a collection with assets then says so rather than guessing.
fn collection_root(call: &Call) -> Option<PathBuf> {
    let store = godwinmix_core::config::Config::runtime_store_path(&call.app.config_path);
    godwinmix_core::scene::server::store::path_beside(&store)
        .parent()
        .map(std::path::Path::to_path_buf)
}

/// What each installed plugin's version is, so the bundle records the range it
/// was built against rather than `*`.
fn installed_versions() -> std::collections::BTreeMap<String, String> {
    godwinmix_core::plugin::loader::list()
        .into_iter()
        .map(|p| (p.name().to_string(), p.version().to_string()))
        .collect()
}

/// `scene.export`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ExportRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// `json` for the document alone, `zip` for a bundle with its assets, or
    /// `dir` for the same bundle unpacked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Where to write it, on the machine the mixer is running on. Required for
    /// `dir`. For `zip`, leaving it out hands the bytes back as base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// `scene.import`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportRequest {
    /// The bundle: a `.zip` or the directory it unpacks to, as a path on the
    /// machine the core is running on.
    pub path: String,
}

/// What `scene.import` answers with.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportedReport {
    /// The scenes that were added, by the names they ended up with.
    pub scenes: Vec<String>,
    pub items: usize,
    /// What the bundle said about itself.
    pub bundle: collection::Bundle,
    /// Assets that did not come across, with the items that draw them. Empty
    /// when everything landed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relink: Vec<collection::Relink>,
    /// Plugins the collection needs that this core has not got. The scenes
    /// still came across; those items will draw nothing until it does.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_plugins: Vec<String>,
    /// Where the assets were written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets_at: Option<String>,
}
