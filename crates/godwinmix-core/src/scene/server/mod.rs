//! The scene server: the core's authority over the document.
//!
//! The core is authoritative (11 section 4). Every edit, from the reference
//! designer, from a Tkinter example, from the CLI and from an agent, is a
//! command on the one protocol, and there is no privileged client. What lives
//! here is the document and everything that has to be true about it: the
//! store, the patches clients mirror off, transactions that apply on one frame
//! or not at all, undo as an inverse diff stack, validation, drafts, and the
//! armed preview.
//!
//! What does not live here is any GStreamer. The server hands out
//! [`crate::mixer::Placement`] lists and the mixer decides how to draw them,
//! which is what lets every test in this module run without a pipeline.
//!
//! ```text
//!   scene.* command  ->  edit(|doc| ...)  ->  diff  ->  Patch  ->  event/scene.patch
//!                                          \-> save             \-> undo stack
//! ```
//!
//! One rule holds the whole thing together: a command changes the document and
//! the patch is read off the result. No command describes its own change, so no
//! command can describe it wrongly.

mod clients;
pub mod compose;
pub mod conflict;
mod drafts;
mod stale;
pub mod find;
pub mod graphics;
pub mod ops;
pub mod patch;
mod refused;
pub mod store;
mod transaction;
mod undo;
pub mod view;
mod writers;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use parking_lot::Mutex;
use tokio::sync::broadcast;

use crate::caps::CanvasCaps;
use crate::mixer::Placement;
use crate::scene::document::{Canvas, Collection};
use crate::scene::id::Id;
use crate::scene::validate::Finding;

pub use conflict::Conflict;
pub use refused::Refused;
pub use drafts::Draft;
pub use stale::Stale;
pub use patch::{Patch, Update};
pub use view::{Geometry, SceneView};

/// How many patches the broadcast holds for a client that is behind. A client
/// that falls further behind than this is told to take a fresh snapshot,
/// which is `event/resync`.
const PATCH_QUEUE: usize = 256;

/// What a mutating command answers with: the resulting records plus the
/// derived geometry, so no follow up read is needed.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The scene the command worked on, when it worked on one.
    pub scene: Option<SceneView>,
    /// What changed. Empty when the command was asked for what was already
    /// true, which is what makes every one of them idempotent.
    pub patch: Patch,
}

/// The document, and everything that has to be true about it.
pub struct SceneServer {
    inner: Mutex<Inner>,
    patches: broadcast::Sender<Patch>,
    seq: AtomicU64,
}

struct Inner {
    doc: Collection,
    path: Option<PathBuf>,
    canvas: CanvasCaps,
    /// Every client's own undo and redo stacks, marks and open transaction.
    /// See `clients.rs`.
    clients: clients::Histories,
    /// Who changed each record last, so a refused undo can say who is in the
    /// way. See `conflict.rs`.
    writers: conflict::Writers,
    drafts: Vec<Draft>,
    /// The scene that is armed. `program.take {}` with no argument takes it.
    preview: Option<Id>,
    /// A draft the preview draws in place of the armed scene, while a designer
    /// has it open. See `SceneServer::show_draft`.
    preview_draft: Option<Id>,
}

impl SceneServer {
    /// Load the collection beside a runtime store, or start an empty one.
    pub fn open(runtime_store: Option<PathBuf>, canvas: CanvasCaps) -> Result<Arc<SceneServer>> {
        let document_canvas =
            Canvas { width: canvas.width as u32, height: canvas.height as u32, fps: canvas.fps.numer().max(1) as u32 };
        let path = runtime_store.as_deref().map(store::path_beside);
        let doc = match &path {
            Some(p) => store::load(p, document_canvas)?,
            None => Collection::new("Scenes", document_canvas),
        };
        Ok(Arc::new(SceneServer {
            inner: Mutex::new(Inner {
                doc,
                path,
                canvas,
                clients: clients::Histories::default(),
                writers: conflict::Writers::default(),
                drafts: Vec::new(),
                preview: None,
                preview_draft: None,
            }),
            patches: broadcast::channel(PATCH_QUEUE).0,
            seq: AtomicU64::new(0),
        }))
    }

    /// An in memory server with nothing on disk, for a test and for a core
    /// started with no runtime store.
    pub fn in_memory(canvas: CanvasCaps) -> Arc<SceneServer> {
        SceneServer::open(None, canvas).expect("an in memory server cannot fail to load")
    }

    /// Subscribe to `event/scene.patch`.
    pub fn subscribe(&self) -> broadcast::Receiver<Patch> {
        self.patches.subscribe()
    }

    /// The whole document, for `scene.list` and for a fresh snapshot.
    pub fn document(&self) -> Collection {
        self.inner.lock().doc.clone()
    }

    /// One scene, resolved by id or by name.
    pub fn scene(&self, which: &str) -> Result<SceneView> {
        let inner = self.inner.lock();
        let scene = find::scene(&inner.doc, which)?;
        Ok(SceneView::of(&inner.doc, scene))
    }

    /// Every scene, shallowly: what a picker needs and nothing more.
    pub fn list(&self) -> Vec<SceneSummary> {
        let inner = self.inner.lock();
        let preview = inner.preview;
        inner
            .doc
            .scenes
            .iter()
            .map(|s| SceneSummary {
                id: s.id,
                name: s.name.clone(),
                color: s.color.clone(),
                items: s.walk().len(),
                sources: {
                    let mut v: Vec<String> = compose::resolve(&inner.doc, s)
                        .iter()
                        .flat_map(sources_of)
                        .collect();
                    v.sort();
                    v.dedup();
                    v
                },
                armed: preview == Some(s.id),
            })
            .collect()
    }

    /// What `scene.validate` reports.
    pub fn validate(&self, which: Option<&str>) -> Result<Vec<Finding>> {
        let inner = self.inner.lock();
        match which {
            Some(which) => {
                let scene = find::scene(&inner.doc, which)?;
                Ok(crate::scene::validate::scene(scene, &inner.doc.canvas))
            }
            None => Ok(crate::scene::validate::collection(&inner.doc)),
        }
    }

    // -- applying to the compositor ------------------------------------

    /// The placements the mixer should draw for this scene.
    pub fn placements(&self, which: &str) -> Result<(String, Vec<Placement>)> {
        let inner = self.inner.lock();
        let scene = find::scene(&inner.doc, which)?;
        Ok((scene.name.clone(), compose::placements(&inner.doc, scene, &inner.canvas)))
    }

    /// A transition the collection stores under a name.
    ///
    /// A collection carries its own transitions (11 section 7), so a church
    /// that has settled on a 400 ms dissolve calls it "house" and every take
    /// in every scene means the same thing by it. `program.take {transition:
    /// "house"}` resolves here before the built in names are tried, so a
    /// collection can also give "fade" a duration of its own.
    pub fn transition(&self, name: &str) -> Option<crate::scene::Transition> {
        let name = name.trim();
        self.inner
            .lock()
            .doc
            .transitions
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name) || t.id.to_string() == name)
            .cloned()
    }

    /// Every transition the collection names, for an error that lists them.
    pub fn transition_names(&self) -> Vec<String> {
        self.inner.lock().doc.transitions.iter().map(|t| t.name.clone()).collect()
    }

    /// The armed scene, for `program.take` with no argument.
    pub fn armed(&self) -> Option<Id> {
        self.inner.lock().preview
    }

    /// Arm a scene. The armed scene is the preview, and nothing is composited
    /// for it until a client subscribes with `ext.preview`.
    pub fn arm(&self, which: Option<&str>) -> Result<Option<SceneSummary>> {
        let mut inner = self.inner.lock();
        let armed = match which {
            Some(which) => Some(find::scene(&inner.doc, which)?.id),
            None => None,
        };
        inner.preview = armed;
        drop(inner);
        Ok(armed.and_then(|id| self.list().into_iter().find(|s| s.id == id)))
    }

    /// Every source a named scene actually draws, for whoever has to say
    /// which cameras are live.
    ///
    /// The preview side of tally has always read `preview_layout`. The
    /// programme side had nothing to read, so it compared each source id
    /// against the one the mixer names, which is null for any scene of more
    /// than one item, and every camera in a two box read "off" while it was
    /// on air. Same composition, same alpha rule, so the two sides of a tally
    /// agree about what "drawn" means.
    pub fn sources_in(&self, which: &str) -> Vec<String> {
        let inner = self.inner.lock();
        let Ok(scene) = find::scene(&inner.doc, which) else { return Vec::new() };
        let mut sources: Vec<String> = compose::placements(&inner.doc, scene, &inner.canvas)
            .into_iter()
            .filter(|p| p.alpha > 0.0)
            .map(|p| p.source)
            .collect();
        sources.sort();
        sources.dedup();
        sources
    }

    /// The armed scene's placements, at multiview size, for whoever is
    /// building the preview picture.
    ///
    /// Exposed rather than drawn here: the preview is composited in the
    /// multiview pipeline from the per source thumbnails, and `multiview.rs`
    /// is not this module's to change. Nothing is composited while nothing is
    /// armed, which is what "preview is on demand, not permanent" means.
    pub fn preview_layout(&self, width: i32, height: i32) -> Option<PreviewLayout> {
        let inner = self.inner.lock();
        if let Some(layout) = Self::draft_layout(&inner, width, height) {
            return Some(layout);
        }
        let id = inner.preview?;
        let scene = inner.doc.scene(&id)?;
        let canvas = inner.doc.canvas;
        let (sx, sy) = (width as f64 / canvas.width as f64, height as f64 / canvas.height as f64);
        let cells = compose::placements(&inner.doc, scene, &inner.canvas)
            .into_iter()
            .map(|p| PreviewCell {
                source: p.source,
                x: (p.xpos as f64 * sx).round() as i32,
                y: (p.ypos as f64 * sy).round() as i32,
                width: (p.width as f64 * sx).round() as i32,
                height: (p.height as f64 * sy).round() as i32,
                alpha: p.alpha,
                rotation: p.rotation,
                crop: p.crop,
                fit: p.sizing.name().to_string(),
            })
            .collect();
        Some(PreviewLayout { scene: id, name: scene.name.clone(), width, height, cells })
    }

    /// Have the preview draw a draft in place of the armed scene, or stop.
    ///
    /// A designer moves boxes on a draft, and a draft is in no picture the
    /// core makes: the armed scene's preview shows the scene as saved and the
    /// programme shows what is on air, so the boxes moved and the video under
    /// them stayed where it was, and nobody could lay a scene out by eye. This
    /// points the one preview compositor at the draft, through the same
    /// placements the programme would get, so what is seen is what Apply
    /// gives.
    ///
    /// What is armed is left alone, because `program.take` with no argument
    /// takes it and a designer opening must not change what a take does. The
    /// draft going, applied or discarded, ends this by itself.
    pub fn show_draft(&self, draft: Option<&str>) -> Result<()> {
        let mut inner = self.inner.lock();
        inner.preview_draft = match draft {
            Some(id) => Some(drafts::find_draft(&inner.drafts, id)?.id),
            None => None,
        };
        Ok(())
    }

    /// The draft the preview is showing, laid out, when there is one.
    fn draft_layout(inner: &Inner, width: i32, height: i32) -> Option<PreviewLayout> {
        let shown = inner.preview_draft?;
        let draft = inner.drafts.iter().find(|d| d.id == shown)?;
        // Composed inside a copy of the document with the draft in its scene's
        // place, the way `edit_draft` edits it, so a reference to another
        // scene resolves against the collection the draft belongs to.
        let mut working = inner.doc.clone();
        drafts::put_in(&mut working, &draft.scene);
        let scene = working.scene(&draft.of)?;
        let canvas = working.canvas;
        let (sx, sy) = (width as f64 / canvas.width as f64, height as f64 / canvas.height as f64);
        let cells = compose::placements(&working, scene, &inner.canvas)
            .into_iter()
            .map(|p| PreviewCell {
                source: p.source,
                x: (p.xpos as f64 * sx).round() as i32,
                y: (p.ypos as f64 * sy).round() as i32,
                width: (p.width as f64 * sx).round() as i32,
                height: (p.height as f64 * sy).round() as i32,
                alpha: p.alpha,
                rotation: p.rotation,
                crop: p.crop,
                fit: p.sizing.name().to_string(),
            })
            .collect();
        Some(PreviewLayout { scene: draft.of, name: draft.name.clone(), width, height, cells })
    }

    // -- editing -------------------------------------------------------

    /// Run a change over the document and publish what it did.
    ///
    /// Everything that mutates goes through here, which is why nothing has to
    /// remember to emit a patch, save the file, or feed the undo stack. A
    /// change that fails leaves the document exactly as it was: the closure
    /// works on a copy and the copy is only kept when it succeeds.
    pub fn edit<T>(
        &self,
        client: Option<&str>,
        f: impl FnOnce(&mut Collection) -> Result<T>,
    ) -> Result<(T, Patch)> {
        self.edit_at(client, None, f)
    }

    /// The same, carrying the client's own sequence number so its optimistic
    /// drawing can be reconciled against the echo. See `Patch::client_seq`.
    pub fn edit_at<T>(
        &self,
        client: Option<&str>,
        client_seq: Option<u64>,
        f: impl FnOnce(&mut Collection) -> Result<T>,
    ) -> Result<(T, Patch)> {
        let mut guard = self.inner.lock();
        let inner = &mut *guard;
        let before = inner.doc.to_flat();
        let mut working = inner.doc.clone();
        let value = f(&mut working)?;
        working.check_refs().context("the change would make a scene contain itself")?;
        // Anything the change added has no order key yet. Giving it one here,
        // between its neighbours, is what makes the patch below name the one
        // record that moved instead of every sibling.
        working.renumber_order();
        let after = working.to_flat();
        let mut p = patch::diff(&before, &after);
        if p.is_empty() {
            return Ok((value, p));
        }
        p.source_client = client.map(str::to_string);
        p.client_seq = client_seq;
        let at = self.seq.load(Ordering::SeqCst);
        let history = inner.clients.of(client, at);
        p.label = history.group.clone();
        // Inside this client's transaction nothing is published until its
        // commit: a client that saw half a batch would draw a frame nobody
        // asked for. Nor is a number taken, so the patches every client does
        // see stay one apart and a mirror reads no gap into them. Another
        // client's edit in the meantime goes straight through.
        let batched = history.transaction.is_some();
        if !batched {
            p.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        }
        history.remember(p.clone());
        inner.writers.note(&p, client);
        inner.doc = working;
        if !batched {
            inner.save();
        }
        drop(guard);
        if !batched {
            let _ = self.patches.send(p.clone());
        }
        Ok((value, p))
    }

    /// The shape every `scene.*` command that works on one scene uses: change
    /// the document, then read the scene back with its geometry.
    pub fn edit_scene(
        &self,
        client: Option<&str>,
        which: &str,
        f: impl FnOnce(&mut Collection, usize) -> Result<()>,
    ) -> Result<Outcome> {
        self.edit_scene_at(client, None, which, f)
    }

    /// The same, carrying the client's own sequence number.
    pub fn edit_scene_at(
        &self,
        client: Option<&str>,
        client_seq: Option<u64>,
        which: &str,
        f: impl FnOnce(&mut Collection, usize) -> Result<()>,
    ) -> Result<Outcome> {
        let (id, patch) = self.edit_at(client, client_seq, |doc| {
            let index = find::scene_index(doc, which)?;
            f(doc, index)?;
            Ok(doc.scenes[index].id)
        })?;
        let inner = self.inner.lock();
        let scene = inner.doc.scene(&id).map(|s| SceneView::of(&inner.doc, s));
        Ok(Outcome { scene, patch })
    }

    /// The canvas the document is on.
    pub fn canvas(&self) -> Canvas {
        self.inner.lock().doc.canvas
    }
}

/// What `scene.list` answers with per scene.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SceneSummary {
    pub id: Id,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// How many items, groups counted with their children.
    pub items: usize,
    /// Every source the scene draws, so a picker can grey out one whose
    /// sources are missing without reading the whole document.
    pub sources: Vec<String>,
    /// True for the armed scene, which is the preview.
    pub armed: bool,
}

/// The armed scene laid out at multiview size, for whoever draws the preview.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct PreviewLayout {
    pub scene: Id,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub cells: Vec<PreviewCell>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct PreviewCell {
    pub source: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub alpha: f64,
    /// Degrees clockwise, as the item asks. Whoever draws it snaps it.
    pub rotation: f64,
    /// Fractions of the source's picture to trim: left, top, right, bottom.
    pub crop: (f64, f64, f64, f64),
    /// `fill`, `contain` or `cover`: how the picture sits in its box.
    pub fit: String,
}

impl Inner {
    /// Write the document out. A failure is loud in the log and does not fail
    /// the command: the show is in memory and on air, and refusing an edit
    /// because a disk is full would take a working mixer off the air.
    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Err(e) = store::save(path, &self.doc) {
            tracing::error!(?e, path = %path.display(), "could not save the scene collection");
        }
    }
}

/// Apply a patch to a document.
fn apply(doc: &mut Collection, p: &Patch) -> Result<()> {
    let mut flat = doc.to_flat();
    if let Some(header) = &p.header {
        header.after.onto(&mut flat);
    }
    for id in &p.removed {
        flat.records.retain(|r| r.id != *id);
    }
    for update in &p.updated {
        match flat.records.iter_mut().find(|r| r.id == update.after.id) {
            Some(record) => *record = update.after.clone(),
            None => flat.records.push(update.after.clone()),
        }
    }
    for record in &p.added {
        if !flat.records.iter().any(|r| r.id == record.id) {
            flat.records.push(record.clone());
        }
    }
    *doc = flat.to_tree().context("the change would not rebuild into a document")?;
    Ok(())
}

/// Every source an item draws, itself and its children.
fn sources_of(item: &crate::scene::document::Item) -> Vec<String> {
    let mut out = Vec::new();
    if let crate::scene::document::Content::Source { source } = &item.content {
        out.push(source.clone());
    }
    for child in item.children() {
        out.extend(sources_of(child));
    }
    out
}

#[cfg(test)]
mod clients_tests;
#[cfg(test)]
mod collab_tests;
#[cfg(test)]
mod draft_tests;
#[cfg(test)]
mod tests;
