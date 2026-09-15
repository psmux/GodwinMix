//! `scene.graphic.list`, `scene.item.schema` and `scene.apply_graphic`.
//!
//! The three calls a designer client and an agent both need for graphics, and
//! they are the same three: what templates are there, what fields does this one
//! take, fill them by name. Canva's autofill shape (11 section 6): discover
//! fields by name, fill by name, never by coordinate, and every mutating call
//! answers with the records so the caller can check its own work.
//!
//! What is here that is not in `scene::server::graphics` is everything that
//! needs the mixer: starting the source a graphic's page is rendered into, and
//! telling the graphics host what to put in it. The document half is pure and
//! stays there.
//!
//! ```text
//!   scene.apply_graphic {graphic, values}
//!        |
//!        +-- graphics::apply    write the values onto every matching item
//!        +-- ensure_source      source.add a browser source on the host's page,
//!        |                      once per item, idempotent
//!        +-- tool.call ograf    load, then play or stop if asked
//!        \-- SceneView          the records, and a frame if one was asked for
//! ```

use super::{client, scene_error, server};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::scene::document::Collection;
use godwinmix_core::scene::id::Id;
use godwinmix_core::scene::server::graphics;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::method::{any_object, schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new(
            "scene.graphic.list",
            Scope::Read,
            "Every graphic template this core can place, with what each one takes.",
            handler(|_call: Call, _| async move {
                body(GraphicListing { graphics: graphics::catalogue() })
            }),
        )
        .result(schema_of::<GraphicListing>)
        .tool(
            "list_graphics",
            Tier::Search,
            "The graphic templates this mixer has: lower thirds, straps, title cards, \
             anything a plugin ships. Each one says what fields it takes. Use it before \
             `apply_graphic`, and put one on a scene with `add_scene_item` using \
             {content: {graphic: \"<the id>\"}}.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.item.schema",
            Scope::Read,
            "What one item type takes: a graphic's OGraf schema, or a source or filter \
             plugin's settings schema. The same JSON Schema every client renders an \
             inspector from.",
            handler(item_schema),
        )
        .params(schema_of::<ItemSchemaRequest>)
        .result(any_object)
        .tool(
            "scene_item_schema",
            Tier::Search,
            "The fields one item type takes, as JSON Schema: give it a graphic id like \
             \"ograf/lower-third\", or a source or filter type. Read this before filling \
             a graphic in, so you fill fields it has.",
        ),
    );

    reg.register(
        MethodDef::new(
            "scene.apply_graphic",
            Scope::Operate,
            "Fill a graphic that is on a scene, by field name, and optionally play it on \
             or take it off. Answers with the records and, if asked, a still.",
            handler(apply_graphic),
        )
        .params(schema_of::<ApplyGraphicRequest>)
        .result(any_object)
        .tool(
            "apply_graphic",
            Tier::Search,
            "Put words into a graphic that is already on a scene, by field name: \
             {graphic: \"ograf/lower-third\", values: {name: \"Ada Lovelace\", title: \
             \"Analyst\"}, play: true}. `scene_item_schema` says what fields it takes. \
             With two of the same graphic on the canvas, give `item` to say which, by the \
             name you called it. `play: true` brings it on, `stop: true` takes it off. \
             Ask for `frame: true` to get a still back and see what you did.",
        ),
    );
}

async fn item_schema(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ItemSchemaRequest = call.params(&params)?;
    let kind = req.r#type.trim();
    // A graphic first, because that is what this method is mostly asked for
    // and because a graphic's schema is its OGraf one and not its settings.
    if let Ok(graphic) = graphics::find(kind) {
        return Ok(json!({
            "type": kind,
            "kind": "graphic",
            "title": graphic.title(),
            "schema": graphic.ograf.schema,
            "step_count": graphic.ograf.step_count,
            "designer": graphic.designer,
        }));
    }
    match godwinmix_core::plugin::loader::settings_schema(kind) {
        Some(schema) => Ok(json!({ "type": kind, "kind": "plugin", "schema": schema })),
        None => Err(RpcError::not_found("item type", kind, &known_types())),
    }
}

/// Every item type a schema can be asked for, for an error that lists them.
fn known_types() -> Vec<String> {
    let mut out = graphics::names();
    for kind in ["source", "filter"] {
        out.extend(godwinmix_core::plugin::loader::provides_of_kind(kind));
    }
    out.sort();
    out.dedup();
    out
}

async fn apply_graphic(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ApplyGraphicRequest = call.params(&params)?;
    let graphic = req.graphic.trim().to_string();
    // Refused before anything is written: a graphic this core has not got would
    // put values on an item nothing can render.
    graphics::find(&graphic).map_err(|e| {
        RpcError::new(ErrorCode::NotFound, format!("{e:#}")).with("method", call.method)
    })?;
    if req.play && req.stop {
        return Err(RpcError::invalid_params(
            "scene.apply_graphic takes `play` or `stop`, not both. Play brings the \
             graphic on, stop takes it off.",
        ));
    }

    let values = req.values.clone();
    let which = req.item.clone();
    let (touched, _) = server(&call)
        .edit(client(&call).as_deref(), |doc| graphics::apply(doc, &graphic, &values, which.as_deref()))
        .map_err(|e| scene_error(&call, e))?;

    // Now the live half: a source per placement, then the host is told what to
    // show. Both are idempotent, so calling this twice changes nothing twice.
    let doc = server(&call).document();
    let mut sources = Vec::new();
    for item in &touched {
        match place(&call, &doc, &graphic, item, req.play, req.stop).await {
            Ok(source) => sources.push(source),
            Err(e) => {
                // The document edit landed. A host that is not running is worth
                // saying so about, not worth undoing the words over: the
                // graphic comes up filled in when the plugin starts.
                tracing::warn!(%graphic, ?e, "the graphic was filled in but not pushed to its host");
            }
        }
    }

    // Everything that changed a scene on air has to reach the pipeline, or the
    // new source would sit in the document until the next take.
    for scene in scenes_with(&doc, &touched) {
        if let Ok(view) = server(&call).scene(&scene) {
            super::edit::reapply_if_on_air(&call, &view).await;
        }
    }
    super::edit::push_preview(&call);

    let mut answer = serde_json::to_value(graphics::applied(&doc, &graphic, &touched))
        .map_err(|e| RpcError::internal(format!("{e}")))?;
    answer["sources"] = json!(sources);
    answer["playing"] = json!(req.play);
    if req.frame {
        answer["frame"] = frame_of(&call).await;
    }
    Ok(answer)
}

/// The names of every scene holding one of these items.
fn scenes_with(doc: &Collection, items: &[Id]) -> Vec<String> {
    doc.scenes
        .iter()
        .filter(|s| s.walk().iter().any(|i| items.contains(&i.id)))
        .map(|s| s.name.clone())
        .collect()
}

/// Make sure this placement has a source rendering its page, and tell the host
/// what to put in it.
///
/// Called on every apply and on `scene.item.add`, and safe both times: a source
/// that is already there is left alone, and `load` merges rather than blanking.
pub(crate) async fn place(
    call: &Call,
    doc: &Collection,
    graphic: &str,
    item: &Id,
    play: bool,
    stop: bool,
) -> Result<String, RpcError> {
    let source = graphics::source_id(graphic, item);
    let base = host_base(call).await?;
    if !call.source_ids().await.contains(&source) {
        let uri = graphics::page_url(&base, graphic, item);
        let name = doc
            .scenes
            .iter()
            .flat_map(|s| s.walk())
            .find(|i| i.id == *item)
            .and_then(|i| i.name.clone());
        let request = godwinmix_protocol::requests::AddSourceRequest {
            id: Some(source.clone()),
            name,
            uri,
            kind: Some("web".into()),
            superimpose: None,
            params: Map::new(),
        };
        crate::control::add_source_now(&call.app, request)
            .await
            .map_err(|e| call.mixer_error(e))?;
    }
    // The values as the graphic will render them: the schema's defaults, the
    // item's own params, and the collection's parameters resolved. The host
    // never sees a `{{name}}`.
    let ograf = graphics::find(graphic).ok().map(|g| g.ograf);
    let values = doc
        .scenes
        .iter()
        .flat_map(|s| s.walk())
        .find(|i| i.id == *item)
        .map(|i| graphics::values(doc, i, ograf.as_ref()))
        .unwrap_or(Value::Null);
    host_call(call, json!({
        "action": "load",
        "instance": source,
        "graphic": graphic,
        "values": values,
    }))
    .await?;
    if play || stop {
        let action = if play { "play" } else { "stop" };
        host_call(call, json!({ "action": action, "instance": source })).await?;
    }
    Ok(source)
}

/// Take a placement off the host when its item is removed.
pub(crate) async fn forget(call: &Call, graphic: &str, item: &Id) {
    let source = graphics::source_id(graphic, item);
    let _ = host_call(call, json!({ "action": "forget", "instance": source })).await;
}

/// Where the graphics host is listening, asked once per call.
///
/// Asked rather than configured: the host takes a free port when the one it
/// wants is busy, and a core that had cached the wrong one would point every
/// graphic at nothing.
async fn host_base(call: &Call) -> Result<String, RpcError> {
    let answer = host_call(call, json!({ "action": "where" })).await?;
    answer
        .get("structured_content")
        .and_then(|s| s.get("base"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            RpcError::new(
                ErrorCode::NotInState,
                "the graphics host did not say where it is listening. Check it with \
                 `plugin.list`; it may have failed to bind a port."
                    .to_string(),
            )
        })
}

/// Call the graphics host's one tool, off the async runtime's thread.
async fn host_call(call: &Call, arguments: Value) -> Result<Value, RpcError> {
    let supervisor = call.app.plugins.clone();
    let answered = tokio::task::spawn_blocking(move || {
        supervisor.tool_call(graphics::HOST_TOOL, arguments)
    })
    .await
    .map_err(|e| RpcError::internal(format!("{e}")))?;
    answered.map_err(|e| {
        RpcError::new(
            ErrorCode::NotInState,
            format!(
                "{e:#}\n\nA graphic needs the graphics host running. Install it with \
                 `gmx plugin add ./plugins/ograf` and check it with `plugin.list`."
            ),
        )
        .with("method", "scene.apply_graphic")
    })
}

/// A still of the armed scene, when one was asked for. Never a failure: the
/// values landed, and a client that wanted a picture and did not get one is
/// better off being told than having the whole call refused.
async fn frame_of(call: &Call) -> Value {
    match crate::control::methods::scenes::edit::preview_still(call).await {
        Ok(frame) => frame,
        Err(e) => json!({ "unavailable": format!("{e}") }),
    }
}

/// `scene.graphic.list`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphicListing {
    pub graphics: Vec<graphics::GraphicType>,
}

/// `scene.item.schema`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ItemSchemaRequest {
    /// The item type: a graphic id like `ograf/lower-third`, or a plugin
    /// provide like `camera/source`.
    pub r#type: String,
}

/// `scene.apply_graphic`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ApplyGraphicRequest {
    /// The graphic to fill, `ograf/lower-third`.
    pub graphic: String,
    /// The fields, by name. `scene.item.schema` says which there are.
    #[serde(default)]
    pub values: Map<String, Value>,
    /// Which placement, by the name you gave the item or by its id. Left out,
    /// every placement of this graphic is filled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// Bring it on after filling it in.
    #[serde(default)]
    pub play: bool,
    /// Take it off.
    #[serde(default)]
    pub stop: bool,
    /// Answer with a still of the armed scene as well as the records.
    #[serde(default)]
    pub frame: bool,
}
