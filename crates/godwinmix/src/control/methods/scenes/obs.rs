//! `scene.import.obs`: an OBS Studio scene collection, from a path on the
//! mixer's machine or from the file's text sent by a page.
//!
//! A browser's file picker hands the page a file on the operator's computer,
//! which is not a path the mixer can open, so `content` carries the text
//! itself. With `add_sources` the sources the scenes draw are added too,
//! through `source.add`'s own handler, so the report ends in a mixer that
//! shows the scenes and not in a block of TOML to paste somewhere.
//! `gmx import obs` reads the same collection with the same importer
//! (`godwinmix_core::scene::obs_import`), offline.

use godwinmix_core::scene::obs_import::{self, ImportedSource, Outcome};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::requests::ImportObsRequest;
use super::{client, scene_error, server};
use crate::control::call::Call;
use crate::control::methods::body;

/// Past this the collection is refused rather than parsed. A real one is tens
/// of kilobytes; the same ceiling `scene.export` puts on an inline answer.
const CONTENT_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportReport {
    /// The scenes that were added, by the names they ended up with.
    pub scenes: Vec<String>,
    pub items: usize,
    /// What could not be brought across, and why, one line each.
    pub skipped: Vec<String>,
    /// The sources the collection needs, by id. Without `add_sources` they
    /// have to be added separately.
    pub sources: Vec<String>,
    /// Every OBS source and what became of it: carried across, needing a
    /// plugin that is not installed, or skipped with the reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_report: Vec<obs_import::SourceReport>,
    /// OBS attaches a filter to a source, so a camera keyed in one scene is
    /// keyed in all of them. Here filters belong to the item, so a source
    /// filter is copied onto each placement and each copy is named here. This
    /// is the one thing an import changes the meaning of, so it is reported
    /// rather than left for somebody to find on air.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters_duplicated: Vec<obs_import::FilterReport>,
    /// With `add_sources`: the sources added to the mixer, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources_added: Option<Vec<String>>,
    /// With `add_sources`: the sources that were not added, each with why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources_not_added: Option<Vec<SourceNotAdded>>,
    /// The `[[sources]]` block for a config file. Only for an import that did
    /// not add the sources itself, which is what the command line wants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_toml: Option<String>,
}

/// A source the import found and did not add, and why.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceNotAdded {
    pub id: String,
    pub reason: String,
    /// The plugin that plays it, when that is what is missing, so a page can
    /// offer to install it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

pub(super) async fn import_obs(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ImportObsRequest = call.params(&params)?;
    let text = collection_text(&req)?;
    let options = obs_import::Options { canvas: Some(server(&call).canvas()), ..Default::default() };
    let imported = obs_import::import(&text, &options).map_err(|e| scene_error(&call, e))?;
    let scenes = add_scenes(&call, &imported)?;
    let mut report = ImportReport {
        scenes,
        items: imported.report.items,
        skipped: imported.report.notes.clone(),
        sources: imported.sources.iter().map(|s| s.id.clone()).collect(),
        source_report: imported.report.sources.clone(),
        filters_duplicated: imported.report.filters_duplicated.clone(),
        sources_added: None,
        sources_not_added: None,
        config_toml: None,
    };
    if req.add_sources {
        let (added, not_added) = add_sources(&call, &imported).await?;
        report.sources_added = Some(added);
        report.sources_not_added = Some(not_added);
    } else {
        report.config_toml = imported.to_config_toml().ok();
    }
    body(report)
}

/// The collection's text, from exactly one of `content` and `path`.
fn collection_text(req: &ImportObsRequest) -> Result<String, RpcError> {
    match (&req.content, &req.path) {
        (Some(content), None) if content.len() > CONTENT_LIMIT => Err(RpcError::invalid_params(format!(
            "this collection is {} bytes, and an OBS scene collection is tens of kilobytes. \
             Check it is the file OBS writes with Scene Collection, Export.",
            content.len()
        ))
        .with("limit", CONTENT_LIMIT)),
        (Some(content), None) => Ok(content.clone()),
        (None, Some(path)) => std::fs::read_to_string(path).map_err(|e| {
            RpcError::new(
                ErrorCode::NotFound,
                format!(
                    "could not read {path} on the mixer's machine: {e}. Export the collection \
                     from OBS with Scene Collection, Export, and send the file's text as \
                     `content` instead, which is what a page's file picker does."
                ),
            )
            .with("path", path.as_str())
            .with("use", "content")
        }),
        (Some(_), Some(_)) => Err(RpcError::invalid_params(
            "scene.import.obs was given both `content` and `path`. Send one: `content` with \
             the collection's text, or `path` to a file on the mixer's machine.",
        )
        .with("fields", json!(["content", "path"]))),
        (None, None) => Err(RpcError::invalid_params(
            "scene.import.obs needs the collection: `content` with the JSON in it (what a \
             page's file picker reads), or `path` to a file on the mixer's machine.",
        )
        .with("fields", json!(["content", "path"]))),
    }
}

/// Add the scenes under the names they end up with, not the ones they came
/// with: a core that already has a "Main" gives the incoming one "Main 2", and
/// a report that said "Main" would name a scene the caller cannot address.
fn add_scenes(call: &Call, imported: &obs_import::Import) -> Result<Vec<String>, RpcError> {
    let incoming = imported.document.scenes.clone();
    let (added, _) = server(call)
        .edit(client(call).as_deref(), |doc| {
            let mut names = Vec::new();
            for scene in &incoming {
                let mut scene = scene.clone();
                scene.name = godwinmix_core::scene::server::find::free_scene_name(doc, &scene.name);
                names.push(scene.name.clone());
                doc.scenes.push(scene);
            }
            Ok(names)
        })
        .map_err(|e| scene_error(call, e))?;
    Ok(added)
}

/// Each source the scenes draw, through `source.add`'s own handler. A source
/// needing a plugin that is not installed, or whose id is already on the
/// mixer, is left out with the reason; one the mixer refuses is left out with
/// the mixer's own words.
async fn add_sources(
    call: &Call,
    imported: &obs_import::Import,
) -> Result<(Vec<String>, Vec<SourceNotAdded>), RpcError> {
    let existing = call.source_ids().await?;
    let (mut added, mut not_added) = (Vec::new(), Vec::new());
    for source in &imported.sources {
        if let Some(why) = reason_to_leave_out(source, &imported.report.sources, &existing) {
            not_added.push(why);
            continue;
        }
        let asked = add_request(source);
        match super::super::sources::add(call.clone(), asked).await {
            Ok(record) => added.push(record["id"].as_str().unwrap_or(&source.id).to_string()),
            Err(e) => not_added.push(SourceNotAdded { id: source.id.clone(), reason: e.message, plugin: None }),
        }
    }
    Ok((added, not_added))
}

fn reason_to_leave_out(
    source: &ImportedSource,
    report: &[obs_import::SourceReport],
    existing: &[String],
) -> Option<SourceNotAdded> {
    if existing.contains(&source.id) {
        return Some(SourceNotAdded {
            id: source.id.clone(),
            reason: format!(
                "this mixer already has a source called {}, and the imported scenes use it. \
                 Remove it first to have the imported one added in its place.",
                source.id
            ),
            plugin: None,
        });
    }
    // The importer works offline and names the plugin a kind comes from; only
    // this mixer knows whether that plugin is loaded here.
    if godwinmix_core::plugin::loader::source_provide(&source.kind).is_some() {
        return None;
    }
    report.iter().find_map(|line| match &line.outcome {
        Outcome::NeedsPlugin { id, plugin, .. } if *id == source.id => Some(SourceNotAdded {
            id: source.id.clone(),
            reason: format!(
                "{} needs the {plugin} plugin, which is not installed. Install it, then import \
                 again or add the source.",
                source.name
            ),
            plugin: Some(plugin.clone()),
        }),
        _ => None,
    })
}

/// What `source.add` is sent for one imported source. A source with no
/// address of its own gives its type id as the address, as `source.add`
/// asks.
fn add_request(source: &ImportedSource) -> Value {
    let mut asked = Map::new();
    asked.insert("id".into(), json!(source.id));
    asked.insert("name".into(), json!(source.name));
    asked.insert("uri".into(), json!(source.uri.clone().unwrap_or_else(|| source.kind.clone())));
    asked.insert("type".into(), json!(source.kind));
    if !source.params.is_null() {
        asked.insert("params".into(), source.params.clone());
    }
    Value::Object(asked)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(path: Option<&str>, content: Option<&str>) -> ImportObsRequest {
        ImportObsRequest { path: path.map(Into::into), content: content.map(Into::into), add_sources: false }
    }

    #[test]
    fn content_is_taken_as_it_is_and_one_of_the_two_is_needed() {
        assert_eq!(collection_text(&req(None, Some("{}"))).unwrap(), "{}");
        let none = collection_text(&req(None, None)).unwrap_err();
        assert!(none.message.contains("`content`") && none.message.contains("`path`"), "{}", none.message);
        let both = collection_text(&req(Some("/x.json"), Some("{}"))).unwrap_err();
        assert!(both.message.contains("both"), "{}", both.message);
        let missing = collection_text(&req(Some("/no/such/collection.json"), None)).unwrap_err();
        assert!(missing.message.contains("send the file's text as `content`"), "{}", missing.message);
    }

    #[test]
    fn a_source_with_no_address_sends_its_type_as_the_address() {
        let source = ImportedSource {
            id: "backdrop".into(),
            name: "Backdrop".into(),
            kind: "test/source".into(),
            uri: None,
            params: json!({ "pattern": "solid" }),
        };
        let asked = add_request(&source);
        assert_eq!(asked["uri"], "test/source");
        assert_eq!(asked["type"], "test/source");
        assert_eq!(asked["params"]["pattern"], "solid");
    }
}
