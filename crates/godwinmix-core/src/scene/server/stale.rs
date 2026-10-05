//! A draft whose scene changed after it was taken, and how that is told.

use std::sync::atomic::Ordering;

use super::conflict::{self, Conflict};
use super::drafts::Draft;
use super::{patch, Outcome, SceneServer};
use crate::scene::document::{Collection, Scene};
use crate::scene::id::Id;

impl SceneServer {
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

    /// What changed in the draft's scene since it was taken, or None when
    /// nothing did.
    pub(super) fn stale(&self, draft: &Draft) -> Option<Stale> {
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
