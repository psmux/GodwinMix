//! Undo and redo, on the asking client's own stacks.
//!
//! Both refuse, with `refused::Refused`, when the step would overwrite
//! another client's later change to the same record. `conflict.rs` says why a
//! step is refused whole rather than merged. Transactions are in
//! `transaction.rs`.

use anyhow::{bail, Result};
use std::sync::atomic::Ordering;

use super::{apply, conflict, patch, Patch, SceneServer};

impl SceneServer {
    /// Group this client's commands that follow into one undo step, until its
    /// next mark. A drag of forty moves is one Ctrl+Z.
    pub fn mark(&self, client: Option<&str>, label: Option<String>) {
        let at = self.seq.load(Ordering::SeqCst);
        self.inner.lock().clients.of(client, at).group = label;
    }

    /// Take back this client's last change. `force` puts it back even over
    /// somebody else's later change.
    pub fn undo(&self, client: Option<&str>, force: bool) -> Result<Patch> {
        self.step(client, true, force)
    }

    pub fn redo(&self, client: Option<&str>, force: bool) -> Result<Patch> {
        self.step(client, false, force)
    }

    fn step(&self, client: Option<&str>, back: bool, force: bool) -> Result<Patch> {
        let verb = if back { "undo" } else { "redo" };
        let at = self.seq.load(Ordering::SeqCst);
        let mut guard = self.inner.lock();
        let inner = &mut *guard;
        let history = inner.clients.of(client, at);
        if history.transaction.is_some() {
            bail!("you have a transaction open. Commit or abort it before you {verb}");
        }
        let stack = if back { &history.undo } else { &history.redo };
        let Some(step) = stack.last().cloned() else {
            bail!(
                "there is nothing to {verb} for this client. 0 changes are on its stack: {verb} \
                 works on the changes this client made, never on somebody else's"
            );
        };
        let now = inner.doc.to_flat();
        let conflicts = conflict::check(&now, &step, &inner.writers, client);
        if !conflicts.is_empty() && !force {
            return Err(super::refused::Refused { verb, conflicts }.into());
        }
        // Forced, a record whose scene has gone has nowhere to go back to, so
        // it is left out and everything else is put back.
        let gone: Vec<_> = conflicts.into_iter().filter(|c| c.gone).collect();
        let mut working = inner.doc.clone();
        apply(&mut working, &conflict::without(&step, &gone))?;
        let mut p = patch::diff(&now, &working.to_flat());
        p.source_client = client.map(str::to_string);
        p.label = step.label.clone();
        let history = inner.clients.of(client, at);
        // The step that reverses what we just did goes on the other stack,
        // bounded by the stack it came off.
        if back {
            history.undo.pop();
            history.redo.push(p.inverse());
        } else {
            history.redo.pop();
            history.push_undo(p.inverse());
        }
        history.group = None;
        if p.is_empty() {
            return Ok(p);
        }
        p.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        inner.doc = working;
        inner.writers.note(&p, client);
        inner.save();
        drop(guard);
        let _ = self.patches.send(p.clone());
        Ok(p)
    }

    /// How many steps are on this client's stacks, for a UI that greys out a
    /// button.
    pub fn history(&self, client: Option<&str>) -> (usize, usize) {
        let inner = self.inner.lock();
        inner.clients.peek(client).map(|h| (h.undo.len(), h.redo.len())).unwrap_or((0, 0))
    }
}
