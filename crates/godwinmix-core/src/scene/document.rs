//! The scene document: the types, their JSON, and the rules that hold between
//! them.
//!
//! One document is a collection (11 section 1): canvases, scenes, transitions,
//! parameters and assets under one name. It has two projections. On disk it is
//! the nested tree written here, pretty printed with one property per line so
//! a diff in git reads like the picture it describes. In the core and on the
//! wire it is a flat record store (see `flat.rs`), because two clients editing
//! two items must not collide on a JSON path.
//!
//! Every rule below was paid for by a failure in a system the research
//! studied, and the comment on each says which.

use crate::scene::id::Id;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The schema version this build writes. Reading an older document runs the
/// migration table in `migrate.rs`.
pub const SCHEMA_VERSION: u32 = 1;

/// A whole collection: the file on disk, the thing a share exports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    /// The document format version. See `migrate`.
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub id: Id,
    pub name: String,
    pub canvas: Canvas,
    /// A JSON Schema block in OGraf's manifest shape, readable on its own
    /// without fetching values, so an agent discovers what is fillable before
    /// filling it. `{{name}}` in a string prop binds to a property here.
    #[serde(default = "empty_params")]
    pub params: Value,
    pub scenes: Vec<Scene>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<Transition>,
    /// Assets by id, each a path relative to the collection root with a hash.
    /// Never an absolute path: OBS stores those, which is why every commercial
    /// scene bundle ships a relink wizard.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<Id, Asset>,
}

/// An empty but valid params block.
pub fn empty_params() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {}
    })
}

/// The output raster. One per collection in this release; 11 section 1 leaves
/// room for several.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas { width: 1920, height: 1080, fps: 30 }
    }
}

impl Canvas {
    /// Parse the `1920x1080` form an operator types on the command line.
    pub fn parse(s: &str) -> Result<Canvas, String> {
        let (w, h) = s.split_once(['x', 'X']).ok_or_else(|| {
            format!("{s:?} is not a canvas size. Write it as WIDTHxHEIGHT, for example 1920x1080")
        })?;
        let parse = |v: &str, which: &str| {
            v.trim()
                .parse::<u32>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("{v:?} is not a {which}. Write it as WIDTHxHEIGHT, for example 1920x1080"))
        };
        Ok(Canvas { width: parse(w, "width")?, height: parse(h, "height")?, ..Canvas::default() })
    }
}

/// A named, ordered composition of items on the canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: Id,
    pub name: String,
    /// Bottom of the stack first, the way the compositor takes them.
    pub items: Vec<Item>,
    /// A free colour for every client, the tally and the Stream Deck to agree
    /// on (11 section 6b). Absent means the client picks one by kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl Scene {
    /// An empty scene with a fresh id.
    pub fn new(name: impl Into<String>) -> Scene {
        Scene { id: Id::new(), name: name.into(), items: Vec::new(), color: None }
    }

    /// Every item in the scene, parents before children.
    pub fn walk(&self) -> Vec<&Item> {
        fn push<'a>(items: &'a [Item], out: &mut Vec<&'a Item>) {
            for item in items {
                out.push(item);
                if let Content::Children { children } = &item.content {
                    push(children, out);
                }
            }
        }
        let mut out = Vec::new();
        push(&self.items, &mut out);
        out
    }
}

/// One placement of content in a scene. Not a "layer": the code already uses
/// that word for the superimposed page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub content: Content,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub crop: Crop,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default)]
    pub blend: Blend,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub audio: Audio,
    /// Filters belong to the item, not to the source. OBS attaches them to
    /// sources only, so a camera keyed in one scene is keyed in all of them and
    /// the workaround is a nested scene per placement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    /// Layout bindings: a geometry path such as `frame.w` against a small
    /// arithmetic expression over the layout's params and `W`, `H`. Present
    /// only in a layout preset; `layout::apply` resolves them and leaves this
    /// empty on the scene it produces.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bind: BTreeMap<String, String>,
}

fn one() -> f64 {
    1.0
}

fn yes() -> bool {
    true
}

impl Item {
    /// An item with everything at its default but the content.
    pub fn new(content: Content) -> Item {
        Item {
            id: Id::new(),
            name: None,
            content,
            transform: Transform::default(),
            crop: Crop::default(),
            opacity: 1.0,
            blend: Blend::Normal,
            visible: true,
            locked: false,
            audio: Audio::Follow,
            filters: Vec::new(),
            bind: BTreeMap::new(),
        }
    }

    /// The item's children, empty unless it is a group.
    pub fn children(&self) -> &[Item] {
        match &self.content {
            Content::Children { children } => children,
            _ => &[],
        }
    }
}

/// What an item shows.
///
/// A group is an item whose content is an inline list of children: no special
/// type and no special rules. OBS made a group a modified scene and collected a
/// decade of transform corruption bugs (2913, 4173, 5478, 9297, 9298, 9558).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
pub enum Content {
    /// A source id: a slug, because an operator types it.
    Source { source: String },
    /// Another scene in this collection, with sparse overrides keyed by the
    /// target's item id (USD's `over`, Godot's revertable override). Unity
    /// keys overrides by array position and silently attaches them to the
    /// wrong entry when the source changes.
    Ref {
        #[serde(rename = "ref")]
        scene: Id,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        overrides: BTreeMap<Id, Override>,
    },
    /// A graphic template, by its plugin qualified id.
    Graphic {
        graphic: String,
        #[serde(default, skip_serializing_if = "Value::is_null")]
        params: Value,
    },
    /// A group.
    Children { children: Vec<Item> },
}

/// A sparse change to one item of a referenced scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Override {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<Crop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
}

/// Where an item sits and how it is sized.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    /// Canvas pixels, of the item's anchor point.
    #[serde(default)]
    pub position: Vec2,
    /// Degrees, clockwise, about the anchor.
    #[serde(default)]
    pub rotation: f64,
    #[serde(default = "Vec2::one")]
    pub scale: Vec2,
    /// Normalised 0 to 1 within the item's own box: (0,0) top left, (0.5,0.5)
    /// centre, (1,1) bottom right.
    #[serde(default)]
    pub anchor: Vec2,
    /// The rectangle the content is fitted into, in canvas pixels. Absent
    /// means the content's own size, scaled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<Frame>,
    #[serde(default)]
    pub fit: Fit,
    #[serde(default)]
    pub align: Align,
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            anchor: Vec2::ZERO,
            frame: None,
            fit: Fit::None,
            align: Align::Center,
        }
    }
}

/// A point or a pair of factors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Vec2 {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    pub const ONE: Vec2 = Vec2 { x: 1.0, y: 1.0 };

    pub fn new(x: f64, y: f64) -> Vec2 {
        Vec2 { x, y }
    }

    /// Serde's default for `scale`, which is one and not zero.
    pub fn one() -> Vec2 {
        Vec2::ONE
    }
}

impl Default for Vec2 {
    fn default() -> Self {
        Vec2::ZERO
    }
}

/// The rectangle an item is fitted into.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub w: f64,
    pub h: f64,
}

impl Frame {
    pub fn new(w: f64, h: f64) -> Frame {
        Frame { w, h }
    }
}

/// How content fills its frame. SVG's vocabulary, which replaces OBS's seven
/// bounds types and maps onto `sizing-policy` on a `glvideomixer` pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Fit {
    /// The content's own size, scaled by `scale`. The frame is ignored.
    #[default]
    None,
    /// Fit inside the frame, keeping the aspect ratio.
    Contain,
    /// Fill the frame, keeping the aspect ratio, cropping the overflow.
    Cover,
    /// Fill the frame exactly, distorting the aspect ratio.
    Stretch,
    /// Scale so the width matches; the height falls where it falls.
    FitWidth,
    /// Scale so the height matches; the width falls where it falls.
    FitHeight,
    /// Like `contain`, but never scale up past the content's own size.
    Max,
}

/// The nine alignment keywords, used to place content inside its frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Align {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    #[default]
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Align {
    /// The alignment as a pair of 0, 0.5, 1 factors.
    pub fn factors(self) -> Vec2 {
        use Align::*;
        let x = match self {
            TopLeft | CenterLeft | BottomLeft => 0.0,
            TopCenter | Center | BottomCenter => 0.5,
            TopRight | CenterRight | BottomRight => 1.0,
        };
        let y = match self {
            TopLeft | TopCenter | TopRight => 0.0,
            CenterLeft | Center | CenterRight => 0.5,
            BottomLeft | BottomCenter | BottomRight => 1.0,
        };
        Vec2::new(x, y)
    }

    /// Build from a pair of factors, rounding to the nearest keyword.
    pub fn from_factors(x: f64, y: f64) -> Align {
        use Align::*;
        const ROW: [[Align; 3]; 3] = [
            [TopLeft, TopCenter, TopRight],
            [CenterLeft, Center, CenterRight],
            [BottomLeft, BottomCenter, BottomRight],
        ];
        let bucket = |v: f64| if v < 0.25 { 0 } else if v > 0.75 { 2 } else { 1 };
        ROW[bucket(y)][bucket(x)]
    }
}

/// How much of the content's own pixels to trim, normalised 0 to 1 so it
/// survives a canvas change. vMix and CasparCG do this; OBS crops in pixels,
/// which is why an OBS collection moved from 1080p to 720p loses its crops.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Crop {
    /// True when nothing is trimmed.
    pub fn is_none(&self) -> bool {
        *self == Crop::default()
    }
}

/// OBS's blend enum, so an import carries across unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    #[default]
    Normal,
    Add,
    Screen,
    Multiply,
    Lighten,
    Darken,
    Subtract,
}

/// Whether the item's source is heard. A source is audible when any live item
/// of it says so, which is OBS's behaviour and changes no pad topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Audio {
    /// Heard while the item is visible on programme.
    #[default]
    Follow,
    /// Heard whether the item is visible or not.
    Always,
    /// Never heard.
    Never,
}

/// One filter in an item's chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    /// The plugin qualified provide id, for example `chroma/filter`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub params: Value,
}

/// A named transition between two scenes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub id: Id,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub duration_ms: u32,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

/// A file the collection carries with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    /// Relative to the collection root, always.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl Collection {
    /// An empty collection at the current schema version.
    pub fn new(name: impl Into<String>, canvas: Canvas) -> Collection {
        Collection {
            schema_version: SCHEMA_VERSION,
            id: Id::new(),
            name: name.into(),
            canvas,
            params: empty_params(),
            scenes: Vec::new(),
            transitions: Vec::new(),
            assets: BTreeMap::new(),
        }
    }

    /// Read a document from JSON text, running the migrations first.
    pub fn from_json(text: &str) -> anyhow::Result<Collection> {
        let mut value: Value = serde_json::from_str(text)?;
        crate::scene::migrate::migrate(&mut value)?;
        let doc: Collection = serde_json::from_value(value)?;
        doc.check_refs()?;
        Ok(doc)
    }

    /// The on disk form: pretty printed, keys in the order declared above, one
    /// property per line, with the trailing newline a text file wants.
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("a scene document always serialises");
        s.push('\n');
        s
    }

    /// The scene with this id.
    pub fn scene(&self, id: &Id) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.id == *id)
    }

    /// The scene with this name, for a command line that takes one.
    pub fn scene_by_name(&self, name: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.name == name)
    }

    /// Refuse a reference cycle. A scene that contains itself, directly or at
    /// any depth, would flatten forever at apply time, so it is refused at edit
    /// time (11 section 2).
    pub fn check_refs(&self) -> Result<(), RefError> {
        for scene in &self.scenes {
            let mut path = vec![scene.id];
            self.visit(scene, &mut path)?;
        }
        Ok(())
    }

    fn visit(&self, scene: &Scene, path: &mut Vec<Id>) -> Result<(), RefError> {
        for item in scene.walk() {
            let Content::Ref { scene: target, .. } = &item.content else { continue };
            if path.contains(target) {
                let mut cycle: Vec<String> = path.iter().map(|id| self.label(id)).collect();
                cycle.push(self.label(target));
                return Err(RefError::Cycle { via: item.id, path: cycle });
            }
            let Some(next) = self.scene(target) else {
                return Err(RefError::Missing { item: item.id, scene: *target });
            };
            path.push(*target);
            self.visit(next, path)?;
            path.pop();
        }
        Ok(())
    }

    /// A scene's name when it has one, else its id, for an error message.
    fn label(&self, id: &Id) -> String {
        self.scene(id).map(|s| s.name.clone()).unwrap_or_else(|| id.to_string())
    }
}

/// A reference that cannot be followed.
#[derive(Debug, Clone, PartialEq)]
pub enum RefError {
    /// The chain of scenes comes back to where it started.
    Cycle { via: Id, path: Vec<String> },
    /// The item points at a scene that is not in this collection.
    Missing { item: Id, scene: Id },
}

impl std::fmt::Display for RefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefError::Cycle { via, path } => write!(
                f,
                "scene reference cycle: {}. Item {via} closes it. Point that item at a different scene, or copy the scene it names.",
                path.join(" -> ")
            ),
            RefError::Missing { item, scene } => write!(
                f,
                "item {item} references scene {scene}, which is not in this collection. Add that scene, or change the item's content to a source."
            ),
        }
    }
}

impl std::error::Error for RefError {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small collection used by several tests here and in `flat`.
    pub(crate) fn sample() -> Collection {
        let mut doc = Collection::new("Sunday service", Canvas::default());
        let mut stage = Item::new(Content::Source { source: "cam-wide".into() });
        stage.name = Some("stage".into());
        stage.transform.frame = Some(Frame::new(1920.0, 1080.0));
        stage.transform.fit = Fit::Cover;

        let mut child = Item::new(Content::Source { source: "cam-pulpit".into() });
        child.transform.position = Vec2::new(1400.0, 40.0);
        child.transform.frame = Some(Frame::new(480.0, 270.0));
        child.transform.fit = Fit::Cover;
        child.filters.push(Filter {
            kind: "chroma/filter".into(),
            name: None,
            enabled: true,
            params: json!({ "key": "#00ff00" }),
        });
        let mut group = Item::new(Content::Children { children: vec![child] });
        group.name = Some("corner".into());
        group.opacity = 0.9;

        let mut scene = Scene::new("Wide with lower third");
        scene.items = vec![stage, group];
        doc.scenes.push(scene);
        doc
    }

    #[test]
    fn the_document_of_the_specification_round_trips_through_json() {
        let doc = sample();
        let text = doc.to_json();
        let back = Collection::from_json(&text).unwrap();
        assert_eq!(doc, back);
        assert!(text.ends_with("}\n"));
        // One property per line is what makes a git diff readable.
        assert!(text.contains("\n  \"schemaVersion\": 1,\n"), "{text}");
    }

    #[test]
    fn keys_come_out_in_the_canonical_order() {
        let text = sample().to_json();
        let order: Vec<usize> = ["schemaVersion", "\"id\"", "\"name\"", "canvas", "params", "scenes"]
            .iter()
            .map(|k| text.find(k).unwrap_or_else(|| panic!("{k} missing from {text}")))
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(order, sorted, "document keys came out in the wrong order");
    }

    #[test]
    fn the_three_content_shapes_are_told_apart_by_their_keys() {
        for (text, ok) in [
            (r#"{"source":"cam1"}"#, true),
            (r#"{"ref":"0192f3a4-1b2c-7d3e-8f40-51a2b3c4d5e6"}"#, true),
            (r#"{"graphic":"lowerthird/graphic","params":{"name":"Ada"}}"#, true),
            (r#"{"children":[]}"#, true),
            (r#"{"sauce":"cam1"}"#, false),
        ] {
            assert_eq!(serde_json::from_str::<Content>(text).is_ok(), ok, "{text}");
        }
    }

    #[test]
    fn a_reference_cycle_is_refused_with_the_path_in_the_message() {
        let mut doc = Collection::new("cycle", Canvas::default());
        let a = Scene::new("A");
        let mut b = Scene::new("B");
        b.items.push(Item::new(Content::Ref { scene: a.id, overrides: BTreeMap::new() }));
        let mut a = a;
        a.items.push(Item::new(Content::Ref { scene: b.id, overrides: BTreeMap::new() }));
        doc.scenes = vec![a, b];
        let err = doc.check_refs().unwrap_err();
        assert!(err.to_string().contains("A -> B -> A"), "{err}");
    }

    #[test]
    fn a_reference_to_a_scene_that_is_not_here_names_the_item() {
        let mut doc = Collection::new("missing", Canvas::default());
        let mut scene = Scene::new("A");
        let item = Item::new(Content::Ref { scene: Id::new(), overrides: BTreeMap::new() });
        let id = item.id;
        scene.items.push(item);
        doc.scenes.push(scene);
        let err = doc.check_refs().unwrap_err();
        assert!(err.to_string().contains(&id.to_string()), "{err}");
    }

    #[test]
    fn a_canvas_size_is_read_from_the_command_line_form() {
        assert_eq!(Canvas::parse("1280x720").unwrap().width, 1280);
        assert_eq!(Canvas::parse("1280x720").unwrap().height, 720);
        assert!(Canvas::parse("1280").unwrap_err().contains("WIDTHxHEIGHT"));
        assert!(Canvas::parse("0x720").unwrap_err().contains("width"));
    }

    #[test]
    fn alignment_keywords_and_factors_are_the_same_nine_things() {
        for a in [
            Align::TopLeft,
            Align::TopCenter,
            Align::TopRight,
            Align::CenterLeft,
            Align::Center,
            Align::CenterRight,
            Align::BottomLeft,
            Align::BottomCenter,
            Align::BottomRight,
        ] {
            let f = a.factors();
            assert_eq!(Align::from_factors(f.x, f.y), a);
        }
    }
}
