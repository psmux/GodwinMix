//! What a mutating command answers with.
//!
//! Every scene command returns the resulting records plus the derived
//! geometry, so a client draws handles without recomputing layout and an agent
//! checks its own work without a second call (11 section 4, 09 section 5 item
//! 5). The geometry is `scene::geometry::flatten`, which is the one place that
//! arithmetic lives; there is no second implementation here.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::scene::document::{Canvas, Collection, Content, Scene};
use crate::scene::flat::Record;
use crate::scene::geometry;
use crate::scene::id::Id;
use crate::scene::validate::Finding;

/// One scene as a command answers with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneView {
    pub id: Id,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub canvas: Canvas,
    /// The scene's own record and one per item, parents before children.
    pub records: Vec<Record>,
    /// Where each item actually lands, after groups are flattened and
    /// references resolved. Bottom of the stack first, which is the order the
    /// compositor takes them in.
    pub geometry: Vec<Geometry>,
    /// What `scene.validate` would say about it, so a client shows a warning
    /// without asking again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
}

/// One item's derived box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Geometry {
    pub item: Id,
    /// The names from the top item down, so a message can say
    /// `corner / pulpit` rather than an id.
    pub path: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// The item's own opacity multiplied by every group's above it.
    pub opacity: f64,
    /// Present for an item whose content is a source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The canvas, which is what the document knows about a source's own size
    /// until the mixer says otherwise. Named so a client that does know can
    /// tell the two apart.
    pub source_width: f64,
    pub source_height: f64,
}

impl SceneView {
    /// Read one scene out of a document.
    pub fn of(doc: &Collection, scene: &Scene) -> SceneView {
        let flat = doc.to_flat();
        let wanted: Vec<Id> = scene.walk().iter().map(|i| i.id).collect();
        let records: Vec<Record> = flat
            .records
            .into_iter()
            .filter(|r| r.id == scene.id || wanted.contains(&r.id))
            .collect();
        SceneView {
            id: scene.id,
            name: scene.name.clone(),
            color: scene.color.clone(),
            canvas: doc.canvas,
            records,
            geometry: geometry_of(doc, scene),
            findings: crate::scene::validate::scene(scene, &doc.canvas),
        }
    }
}

/// Every item's box, flattened, in the order the compositor takes them.
pub fn geometry_of(doc: &Collection, scene: &Scene) -> Vec<Geometry> {
    let resolved = super::compose::resolve(doc, scene);
    geometry::flatten(&resolved, &doc.canvas)
        .into_iter()
        .map(|p| Geometry {
            item: p.item.id,
            path: p.path.clone(),
            x: p.rect.x,
            y: p.rect.y,
            width: p.rect.w,
            height: p.rect.h,
            opacity: p.opacity,
            source: match &p.item.content {
                Content::Source { source } => Some(source.clone()),
                _ => None,
            },
            source_width: doc.canvas.width as f64,
            source_height: doc.canvas.height as f64,
        })
        .collect()
}
