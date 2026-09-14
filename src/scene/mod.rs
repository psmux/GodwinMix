//! Scenes: the document, its two projections, the built in layouts, the
//! validator and the OBS importer.
//!
//! What is here is everything about a scene that is pure data. The live part
//! (the `scene.*` commands, the compositor's slot pool, undo) is the scene
//! server and belongs beside the mixer; it reads and writes these types.
//!
//! 11 section 2 is the specification these types follow, key for key.

pub mod cli;
pub mod document;
pub mod expr;
pub mod flat;
pub mod geometry;
pub mod id;
pub mod layout;
pub mod migrate;
pub mod obs_import;
pub mod order;
pub mod schema;
pub mod validate;

pub use document::{
    Align, Asset, Audio, Blend, Canvas, Collection, Content, Crop, Filter, Fit, Frame, Item,
    Override, Scene, Transform, Transition, Vec2, SCHEMA_VERSION,
};
pub use flat::{FlatContent, FlatDocument, ItemProps, Props, Record};
pub use id::Id;
