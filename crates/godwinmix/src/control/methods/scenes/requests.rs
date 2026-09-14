//! The request types for `scene.*`.
//!
//! Every one that names a scene or an item takes a name or an id in the same
//! field, because an operator types a name and a client holds an id, and
//! making them two fields would be a method with two arguments that can
//! contradict each other.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Anything that names one scene.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SceneRequest {
    /// The scene's name or its id.
    pub scene: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ValidateRequest {
    /// Leave it out to check the whole collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddSceneRequest {
    pub name: String,
    /// A colour for every client, the tally and the Stream Deck to agree on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateFromRequest {
    /// Source ids, in the order they should be laid out.
    pub sources: Vec<String>,
    /// A layout name from `scene.layout.list`. Left out, the number of sources
    /// picks one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RenameSceneRequest {
    pub scene: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DuplicateSceneRequest {
    pub scene: String,
    /// What to call the copy. A name already in use gets a number after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ExportRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// `json` in this build. `zip`, with the assets, is Phase 5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImportObsRequest {
    /// The collection JSON exported from OBS (Scene Collection, Export), as a
    /// path on the machine the core is running on.
    pub path: String,
}

/// `scene.item.add`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddItemRequest {
    pub scene: String,
    /// What the item shows: `{"source": "cam1"}`, `{"ref": "<scene id>"}` or
    /// `{"graphic": "plugin/id"}`.
    pub content: Value,
    /// What to call it. Left out, a source item is named after its source,
    /// because a model reasons about words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where it goes. Left out, the next free cell of a grid over what is
    /// already there, so a drop on a scene never needs a dialog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Value>,
    /// A draft id from `scene.edit.begin`, to change a working copy instead of
    /// the live document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
}

/// Anything that names one item.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ItemRequest {
    pub scene: String,
    /// The item's name or its id.
    pub item: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
}

/// `scene.item.set`: a state assignment. Only the keys named move.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetItemRequest {
    pub scene: String,
    pub item: String,
    /// Any of `name`, `transform`, `crop`, `opacity`, `blend`, `visible`,
    /// `locked`, `audio`, `content`. A key left out is left alone.
    pub props: Map<String, Value>,
    /// How long to take getting there, in milliseconds. 0 is a cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// `linear` or `ease`. Only meaningful with a duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    /// A client's own sequence number, echoed on the patch so a drag can
    /// discard the echoes of moves it has already drawn past.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
}

/// `scene.item.align`, `distribute`, `fit_to_canvas`, `cover_canvas`,
/// `arrange_grid`, `match_size`, `group`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ItemsRequest {
    pub scene: String,
    /// Item names or ids.
    pub items: Vec<String>,
    /// `align`: left, right, top, bottom, center-x, center-y.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<String>,
    /// `distribute`: horizontal or vertical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<String>,
    /// `arrange_grid`: how many columns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cols: Option<usize>,
    /// `match_size`: the item to match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// `group`: what to call the group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    /// A client's own sequence number, echoed on the patch. See
    /// `SetItemRequest::seq`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
}

/// `scene.item.reorder`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReorderRequest {
    pub scene: String,
    pub item: String,
    /// Put it behind this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Put it in front of this one. With neither, it goes to the front.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// A client's own sequence number, echoed on the patch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
}

/// `scene.item.move` and `scene.item.copy`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MoveItemRequest {
    pub scene: String,
    pub item: String,
    /// The scene it is going to.
    pub to_scene: String,
}

/// `scene.item.filter.add`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddItemFilterRequest {
    pub scene: String,
    pub item: String,
    /// A filter type id, as `plugin.list` reports them.
    #[serde(rename = "type")]
    pub type_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
}

/// `scene.item.filter.set` and `remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ItemFilterRequest {
    pub scene: String,
    pub item: String,
    /// The filter's name, or its position in the item's chain from 0.
    pub filter: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    /// Turn a filter off without taking it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
}

/// `scene.item.bind`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BindRequest {
    pub scene: String,
    pub item: String,
    /// A geometry path such as `frame.w` or `position.x`.
    pub prop: String,
    /// An expression over the collection's params and `W`, `H`. An empty
    /// string takes the binding off.
    pub param: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
}

/// `scene.params.set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ParamsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    #[serde(default)]
    pub values: Map<String, Value>,
}

/// `scene.apply_layout`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApplyLayoutRequest {
    /// A layout name from `scene.layout.list`.
    pub layout: String,
    /// The layout's parameters by name: its source slots and its numbers.
    #[serde(default)]
    pub values: Map<String, Value>,
    /// The scene to apply it to. Left out, a new one is made. Applying onto an
    /// existing scene keeps the item ids, so an animated layout change is a
    /// property ramp rather than a cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How long the change takes, in milliseconds. 0 is a cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
}

/// `scene.layout.copy` and `paste`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LayoutClipboardRequest {
    pub scene: String,
    /// What `scene.layout.copy` answered with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Value>,
    /// `name` matches item names first and falls back to slot order; `order`
    /// uses slot order alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#match: Option<String>,
}

/// `scene.preview.set`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PreviewRequest {
    /// The scene to arm. Null or omitted disarms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
}

/// `scene.preview.frame`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PreviewFrameRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

/// `scene.edit.begin`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EditBeginRequest {
    pub scene: String,
    /// True to edit the scene that is on air as you go. The default is off
    /// air: the draft is applied on the next take or on an explicit apply.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub live: bool,
}

/// `scene.edit.apply` and `discard`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DraftRequest {
    pub draft: String,
}

/// `scene.history.mark`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct MarkRequest {
    /// What to call the group of changes that follows. Omit it to end the
    /// group, so the next change is its own undo step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// `source.set`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetSourceMetaRequest {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// `source.group`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GroupSourcesRequest {
    pub sources: Vec<String>,
    /// The tray folder to put them in. Null takes them out of the one they
    /// are in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
