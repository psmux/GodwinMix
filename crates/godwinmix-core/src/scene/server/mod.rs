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

pub mod compose;
pub mod find;
pub mod graphics;
pub mod ops;
pub mod patch;
pub mod store;
pub mod view;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use tokio::sync::broadcast;

use crate::caps::CanvasCaps;
use crate::mixer::Placement;
use crate::scene::document::{Canvas, Collection, Scene};
use crate::scene::id::Id;
use crate::scene::validate::Finding;

pub use patch::{Patch, Update};
pub use view::{Geometry, SceneView};

/// How many patches the broadcast holds for a client that is behind. A client
/// that falls further behind than this is told to take a fresh snapshot,
/// which is `event/resync`.
const PATCH_QUEUE: usize = 256;

/// How deep the undo stack goes. Each entry is a diff of the records that
/// changed, not a copy of the document, so this is cheap.
const UNDO_DEPTH: usize = 200;

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

/// A working copy of one scene, edited off air.
#[derive(Debug, Clone)]
pub struct Draft {
    pub id: Id,
    /// The scene in the live document this draft came from.
    pub of: Id,
    /// The name of that scene when the draft was taken, for a message.
    pub name: String,
    /// True when the client asked to edit on air. The UI says so; the server
    /// only records the choice.
    pub live: bool,
    scene: Scene,
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
    /// Patches that undo what has been done, newest last.
    undo: Vec<Patch>,
    redo: Vec<Patch>,
    /// The label a client set with `scene.history.mark`, which merges the
    /// commands that follow into one undo step.
    group: Option<String>,
    /// The document as it was when the open transaction began.
    transaction: Option<Collection>,
    /// Patches gathered while a transaction is open.
    pending: Vec<Patch>,
    drafts: Vec<Draft>,
    /// The scene that is armed. `program.take {}` with no argument takes it.
    preview: Option<Id>,
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
                undo: Vec::new(),
                redo: Vec::new(),
                group: None,
                transaction: None,
                pending: Vec::new(),
                drafts: Vec::new(),
                preview: None,
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

    /// The armed scene's placements, at multiview size, for whoever is
    /// building the preview picture.
    ///
    /// Exposed rather than drawn here: the preview is composited in the
    /// multiview pipeline from the per source thumbnails, and `multiview.rs`
    /// is not this module's to change. Nothing is composited while nothing is
    /// armed, which is what "preview is on demand, not permanent" means.
    pub fn preview_layout(&self, width: i32, height: i32) -> Option<PreviewLayout> {
        let inner = self.inner.lock();
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
            })
            .collect();
        Some(PreviewLayout { scene: id, name: scene.name.clone(), width, height, cells })
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
        let mut inner = self.inner.lock();
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
        p.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        p.source_client = client.map(str::to_string);
        p.client_seq = client_seq;
        p.label = inner.group.clone();
        inner.doc = working;
        inner.remember(p.clone());
        inner.save();
        let batched = inner.transaction.is_some();
        drop(inner);
        // Inside a transaction nothing is published until commit: a client
        // that saw half a batch would draw a frame nobody asked for.
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

    // -- transactions --------------------------------------------------

    /// Begin a transaction. Everything until `commit` applies on one frame or
    /// not at all (CasparCG's `MIXER COMMIT`).
    pub fn begin(&self) -> Result<()> {
        let mut inner = self.inner.lock();
        if inner.transaction.is_some() {
            bail!(
                "a transaction is already open on this core. Commit it with \
                 scene.transaction.commit or throw it away with scene.transaction.abort"
            );
        }
        inner.transaction = Some(inner.doc.clone());
        inner.pending.clear();
        Ok(())
    }

    /// Commit: one patch for everything that happened, one undo step.
    pub fn commit(&self, client: Option<&str>) -> Result<Patch> {
        let mut inner = self.inner.lock();
        let Some(start) = inner.transaction.take() else {
            bail!("no transaction is open. Open one with scene.transaction.begin");
        };
        let before = start.to_flat();
        let after = inner.doc.to_flat();
        inner.pending.clear();
        let mut p = patch::diff(&before, &after);
        if p.is_empty() {
            return Ok(p);
        }
        p.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        p.source_client = client.map(str::to_string);
        // Nothing inside the transaction went on the undo stack, so the whole
        // batch is one step: that is what "applies on one frame or not at all"
        // means for somebody pressing Ctrl+Z afterwards.
        inner.redo.clear();
        inner.undo.push(p.inverse());
        inner.save();
        drop(inner);
        let _ = self.patches.send(p.clone());
        Ok(p)
    }

    /// Throw the transaction away. The document goes back to where it was when
    /// the transaction opened, in one patch, so a client's mirror follows.
    pub fn abort(&self, client: Option<&str>) -> Result<Patch> {
        let mut inner = self.inner.lock();
        let Some(start) = inner.transaction.take() else {
            bail!("no transaction is open. Open one with scene.transaction.begin");
        };
        let before = inner.doc.to_flat();
        let after = start.to_flat();
        let mut p = patch::diff(&before, &after);
        inner.doc = start;
        inner.pending.clear();
        if p.is_empty() {
            return Ok(p);
        }
        p.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        p.source_client = client.map(str::to_string);
        inner.save();
        drop(inner);
        let _ = self.patches.send(p.clone());
        Ok(p)
    }

    pub fn in_transaction(&self) -> bool {
        self.inner.lock().transaction.is_some()
    }

    // -- undo ----------------------------------------------------------

    /// Group the commands that follow into one undo step, until the next mark.
    /// A drag of forty moves is one Ctrl+Z.
    pub fn mark(&self, label: Option<String>) {
        self.inner.lock().group = label;
    }

    pub fn undo(&self, client: Option<&str>) -> Result<Patch> {
        self.step(client, true)
    }

    pub fn redo(&self, client: Option<&str>) -> Result<Patch> {
        self.step(client, false)
    }

    fn step(&self, client: Option<&str>, back: bool) -> Result<Patch> {
        let mut inner = self.inner.lock();
        if inner.transaction.is_some() {
            bail!("a transaction is open. Commit or abort it before undoing");
        }
        let taken = if back { inner.undo.pop() } else { inner.redo.pop() };
        let Some(step) = taken else {
            bail!(
                "there is nothing to {}. {} changes are on the stack",
                if back { "undo" } else { "redo" },
                if back { inner.undo.len() } else { inner.redo.len() }
            );
        };
        let mut working = inner.doc.clone();
        apply(&mut working, &step)?;
        let before = inner.doc.to_flat();
        let after = working.to_flat();
        let mut p = patch::diff(&before, &after);
        p.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        p.source_client = client.map(str::to_string);
        p.label = step.label.clone();
        inner.doc = working;
        // The step that undoes what we just did goes on the other stack.
        if back {
            inner.redo.push(p.inverse());
        } else {
            inner.undo.push(p.inverse());
        }
        inner.group = None;
        inner.save();
        drop(inner);
        let _ = self.patches.send(p.clone());
        Ok(p)
    }

    /// How many steps are on each stack, for a UI that greys out a button.
    pub fn history(&self) -> (usize, usize) {
        let inner = self.inner.lock();
        (inner.undo.len(), inner.redo.len())
    }

    // -- drafts --------------------------------------------------------

    /// Take a working copy of a scene, so editing happens off air.
    ///
    /// The reference designer opens as a modal on one of these, and a draft of
    /// the scene that is on air is applied on the next take or on an explicit
    /// apply, never on each keystroke. That is the OBS pitfall of editing the
    /// programme scene live, turned into a choice.
    pub fn edit_begin(&self, which: &str, live: bool) -> Result<Draft> {
        let mut inner = self.inner.lock();
        let scene = find::scene(&inner.doc, which)?.clone();
        let draft =
            Draft { id: Id::new(), of: scene.id, name: scene.name.clone(), live, scene };
        inner.drafts.push(draft.clone());
        Ok(draft)
    }

    /// The draft itself, for a command addressed to one.
    pub fn draft(&self, id: &str) -> Result<Draft> {
        let inner = self.inner.lock();
        find_draft(&inner.drafts, id).cloned()
    }

    /// Change a draft. Nothing is published: a draft is nobody else's business
    /// until it is applied.
    pub fn edit_draft(
        &self,
        id: &str,
        f: impl FnOnce(&mut Collection, usize) -> Result<()>,
    ) -> Result<SceneView> {
        let mut inner = self.inner.lock();
        let draft = find_draft(&inner.drafts, id)?.clone();
        // The draft is edited inside a copy of the whole document, so a
        // command that looks at another scene (a reference, a layout paste)
        // sees the collection it belongs to.
        let mut working = inner.doc.clone();
        match working.scenes.iter().position(|s| s.id == draft.of) {
            Some(index) => working.scenes[index] = draft.scene.clone(),
            None => working.scenes.push(draft.scene.clone()),
        }
        let index = working
            .scenes
            .iter()
            .position(|s| s.id == draft.of)
            .expect("the draft's scene was just put in");
        f(&mut working, index)?;
        let scene = working.scenes[index].clone();
        let view = SceneView::of(&working, &scene);
        if let Some(d) = inner.drafts.iter_mut().find(|d| d.id == draft.id) {
            d.scene = scene;
        }
        Ok(view)
    }

    /// Write a draft back into the live document.
    pub fn edit_apply(&self, client: Option<&str>, id: &str) -> Result<Outcome> {
        let draft = self.draft(id)?;
        let outcome = self.edit(client, |doc| {
            match doc.scenes.iter().position(|s| s.id == draft.of) {
                Some(index) => doc.scenes[index] = draft.scene.clone(),
                None => doc.scenes.push(draft.scene.clone()),
            }
            Ok(draft.of)
        })?;
        self.inner.lock().drafts.retain(|d| d.id != draft.id);
        let inner = self.inner.lock();
        let scene = inner.doc.scene(&outcome.0).map(|s| SceneView::of(&inner.doc, s));
        Ok(Outcome { scene, patch: outcome.1 })
    }

    /// Throw a draft away.
    pub fn edit_discard(&self, id: &str) -> Result<Draft> {
        let mut inner = self.inner.lock();
        let draft = find_draft(&inner.drafts, id)?.clone();
        inner.drafts.retain(|d| d.id != draft.id);
        Ok(draft)
    }

    /// Every draft that is open, so a UI can offer to come back to one.
    pub fn drafts(&self) -> Vec<Draft> {
        self.inner.lock().drafts.clone()
    }

    /// The drafts waiting on this scene going to air, applied by the take.
    pub fn apply_drafts_of(&self, client: Option<&str>, scene: Id) -> Vec<Outcome> {
        let waiting: Vec<Draft> =
            self.inner.lock().drafts.iter().filter(|d| d.of == scene && !d.live).cloned().collect();
        waiting
            .into_iter()
            .filter_map(|d| self.edit_apply(client, &d.id.to_string()).ok())
            .collect()
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
}

impl Inner {
    /// Put a step on the undo stack, merging it into the one before when the
    /// client marked them as one.
    fn remember(&mut self, p: Patch) {
        if self.transaction.is_some() {
            // Inside a transaction the batch is the step. See `commit`.
            return;
        }
        self.redo.clear();
        let inverse = p.inverse();
        let merge = self.group.is_some()
            && self.undo.last().is_some_and(|last| last.label == p.label);
        if merge {
            // The inverse of "a then b" is "inverse of b then inverse of a",
            // so the newer inverse goes first and the older one is folded into
            // it: undoing the pair puts everything back where it started.
            let older = self.undo.pop().expect("just checked");
            let mut merged = inverse;
            merged.merge(&older);
            self.undo.push(merged);
        } else {
            self.undo.push(inverse);
        }
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
    }

    /// Write the document out. A failure is loud in the log and does not fail
    /// the command: the show is in memory and on air, and refusing an edit
    /// because a disk is full would take a working mixer off the air.
    fn save(&self) {
        if self.transaction.is_some() {
            return;
        }
        let Some(path) = &self.path else { return };
        if let Err(e) = store::save(path, &self.doc) {
            tracing::error!(?e, path = %path.display(), "could not save the scene collection");
        }
    }
}

/// Apply a patch to a document.
fn apply(doc: &mut Collection, p: &Patch) -> Result<()> {
    let mut flat = doc.to_flat();
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

fn find_draft<'a>(drafts: &'a [Draft], id: &str) -> Result<&'a Draft> {
    let key = id.trim();
    drafts
        .iter()
        .find(|d| d.id.to_string() == key)
        .ok_or_else(|| {
            let open: Vec<String> =
                drafts.iter().map(|d| format!("{} (of {})", d.id, d.name)).collect();
            anyhow::anyhow!(
                "there is no draft {key:?}. Open one with scene.edit.begin. Open drafts: {}",
                if open.is_empty() { "none".into() } else { open.join(", ") }
            )
        })
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
mod tests;
