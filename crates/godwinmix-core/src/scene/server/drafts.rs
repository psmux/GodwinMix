//! Drafts: a working copy of one scene, edited off air.
//!
//! The reference designer opens as a modal on one of these, and a draft of the
//! scene that is on air is applied on the next take or on an explicit apply,
//! never on each keystroke. That is the OBS pitfall of editing the programme
//! scene live, turned into a choice.
//!
//! A draft remembers the scene as it was when it was taken and the document's
//! revision then. An apply over a scene that changed since is refused with
//! `Stale` (see `stale.rs`) unless the caller says `force`. It used to replace
//! the scene regardless, so a phone could wipe out a laptop's moves unseen.

use anyhow::Result;
use std::sync::atomic::Ordering;

use super::{find, Outcome, SceneServer, SceneView};
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
