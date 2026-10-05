//! Drafts: a working copy of one scene, edited off air.
//!
//! The reference designer opens as a modal on one of these, and a draft of the
//! scene that is on air is applied on the next take or on an explicit apply,
//! never on each keystroke. That is the OBS pitfall of editing the programme
//! scene live, turned into a choice.
//!
//! A draft remembers the scene as it was when it was taken and the document's
//! revision at that moment. Applying it used to replace the scene whatever had
//! happened since, so a person who opened the designer on a phone could wipe
//! out every move somebody else had made on a laptop in the meantime. Now an
//! apply over a scene that changed is refused with `Stale`, which lists what
//! changed and who changed it, unless the caller says `force`.

use anyhow::Result;
use std::sync::atomic::Ordering;

use super::conflict::{self, Conflict};
use super::{find, patch, Outcome, SceneServer, SceneView};
use crate::scene::document::{Collection, Scene};
use crate::scene::id::Id;

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
    /// The client that opened it.
    pub owner: Option<String>,
    /// The document's revision (the last patch `seq`) when it was taken.
    pub base_seq: u64,
    pub(super) scene: Scene,
    /// The live scene as it was when the draft was taken, which is what an
    /// apply compares against to tell whether somebody changed it since.
    pub(super) base: Scene,
}

impl SceneServer {
    /// Take a working copy of a scene, so editing happens off air.
    pub fn edit_begin(&self, client: Option<&str>, which: &str, live: bool) -> Result<Draft> {
        let mut inner = self.inner.lock();
        let scene = find::scene(&inner.doc, which)?.clone();
        let draft = Draft {
            id: Id::new(),
            of: scene.id,
            name: scene.name.clone(),
            live,
            owner: client.map(str::to_string),
            base_seq: self.seq.load(Ordering::SeqCst),
            base: scene.clone(),
            scene,
        };
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
        let index = put_in(&mut working, &draft.scene);
        f(&mut working, index)?;
        let scene = working.scenes[index].clone();
        let view = SceneView::of(&working, &scene);
        if let Some(d) = inner.drafts.iter_mut().find(|d| d.id == draft.id) {
            d.scene = scene;
        }
        Ok(view)
    }

    /// Write a draft back into the live document.
    ///
    /// Refused with `Stale` when the scene changed after the draft was taken,
    /// unless `force`, which replaces the scene with the draft as it always
    /// used to.
    pub fn edit_apply(&self, client: Option<&str>, id: &str, force: bool) -> Result<Outcome> {
        let draft = self.draft(id)?;
        if !force {
            if let Some(stale) = self.stale(&draft) {
                return Err(stale.into());
            }
        }
        let outcome = self.edit(client, |doc| {
            put_in(doc, &draft.scene);
            Ok(draft.of)
        })?;
        let mut inner = self.inner.lock();
        inner.drafts.retain(|d| d.id != draft.id);
        let scene = inner.doc.scene(&outcome.0).map(|s| SceneView::of(&inner.doc, s));
        Ok(Outcome { scene, patch: outcome.1 })
    }

    /// What changed in the draft's scene since it was taken, or None when
    /// nothing did.
    fn stale(&self, draft: &Draft) -> Option<Stale> {
        let inner = self.inner.lock();
        let live = inner.doc.scene(&draft.of);
        let changes = match live {
            None => Vec::new(),
            Some(now) => {
                let (was, is) = (alone(&inner.doc, &draft.base), alone(&inner.doc, now));
                if was.scenes == is.scenes {
                    return None;
                }
                let since = patch::diff(&was.to_flat(), &is.to_flat());
                conflict::changes(&since, &inner.writers, draft.owner.as_deref())
            }
        };
        Some(Stale {
            draft: draft.id,
            scene: draft.name.clone(),
            base_seq: draft.base_seq,
            seq: self.seq.load(Ordering::SeqCst),
            removed: live.is_none(),
            changes,
        })
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
    ///
    /// A draft whose scene changed after it was taken is not applied: the take
    /// goes ahead with the scene as it is, and the draft stays open for its
    /// owner to look at, rather than a take silently undoing somebody's work.
    pub fn apply_drafts_of(&self, client: Option<&str>, scene: Id) -> Vec<Outcome> {
        let waiting: Vec<Draft> =
            self.inner.lock().drafts.iter().filter(|d| d.of == scene && !d.live).cloned().collect();
        waiting
            .into_iter()
            .filter_map(|d| match self.edit_apply(client, &d.id.to_string(), false) {
                Ok(outcome) => Some(outcome),
                Err(e) => {
                    tracing::warn!(draft = %d.id, error = %e, "a draft was left open at the take");
                    None
                }
            })
            .collect()
    }
}

/// Put a draft's scene in its place in a document, or at the end if the
/// scene has gone, and say where it is.
pub(super) fn put_in(doc: &mut Collection, scene: &Scene) -> usize {
    match doc.scenes.iter().position(|s| s.id == scene.id) {
        Some(index) => {
            doc.scenes[index] = scene.clone();
            index
        }
        None => {
            doc.scenes.push(scene.clone());
            doc.scenes.len() - 1
        }
    }
}

/// One scene alone in a copy of its collection, for comparing two versions of
/// it.
///
/// Every edit anywhere gives an order key to whatever lacks one, so a scene
/// read from an old file and the same scene after an unrelated edit differ
/// only in keys nobody chose. Both sides are given their keys here, the same
/// way, and where the scene sits among the others is left out: moving a scene
/// in the list is not a change to its layout.
fn alone(doc: &Collection, scene: &Scene) -> Collection {
    let mut one = doc.clone();
    let mut scene = scene.clone();
    scene.order = None;
    one.scenes = vec![scene];
    one.renumber_order();
    one
}

pub(super) fn find_draft<'a>(drafts: &'a [Draft], id: &str) -> Result<&'a Draft> {
    let key = id.trim();
    drafts.iter().find(|d| d.id.to_string() == key).ok_or_else(|| {
        let open: Vec<String> = drafts.iter().map(|d| format!("{} (of {})", d.id, d.name)).collect();
        anyhow::anyhow!(
            "there is no draft {key:?}. Open one with scene.edit.begin. Open drafts: {}",
            if open.is_empty() { "none".into() } else { open.join(", ") }
        )
    })
}

/// A draft refused because its scene changed after it was taken.
#[derive(Debug, Clone)]
pub struct Stale {
    pub draft: Id,
    /// The scene's name when the draft was taken.
    pub scene: String,
    /// The revision the draft was taken at, and the one the document is at.
    pub base_seq: u64,
    pub seq: u64,
    /// True when the scene itself has been removed since.
    pub removed: bool,
    /// What changed, and who changed it last.
    pub changes: Vec<Conflict>,
}

impl std::fmt::Display for Stale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.removed {
            return write!(
                f,
                "the scene {:?} was removed after this draft was taken. Nothing was applied. \
                 Apply with force: true to bring it back as the draft has it, or throw the \
                 draft away with scene.edit.discard.",
                self.scene
            );
        }
        let list: Vec<String> = self.changes.iter().map(Conflict::describe).collect();
        write!(
            f,
            "the scene {:?} changed after this draft was taken at revision {} (it is at {} \
             now): {}. Applying would overwrite that, so nothing was applied. Throw the draft \
             away with scene.edit.discard and open a fresh one with scene.edit.begin to start \
             from what is there now, or apply with force: true to replace the scene with your \
             draft.",
            self.scene,
            self.base_seq,
            self.seq,
            if list.is_empty() { "its own settings".to_string() } else { list.join(", ") }
        )
    }
}

impl std::error::Error for Stale {}
