//! Transactions, each belonging to the client that opened it.
//!
//! A transaction gathers one client's patches and publishes them as one at
//! the commit, so an open transaction holds nobody else up: another client's
//! edit goes through and is published at once. An abort takes back only what
//! the client did inside it, and leaves alone any record somebody else has
//! changed since.

use anyhow::{bail, Result};
use std::sync::atomic::Ordering;

use super::{apply, conflict, patch, Patch, SceneServer};

impl SceneServer {
    /// Begin a transaction for this client. Everything it does until `commit`
    /// applies on one frame or not at all (CasparCG's `MIXER COMMIT`).
    pub fn begin(&self, client: Option<&str>) -> Result<()> {
        let at = self.seq.load(Ordering::SeqCst);
        let mut inner = self.inner.lock();
        let history = inner.clients.of(client, at);
        if history.transaction.is_some() {
            bail!(
                "you already have a transaction open on this core. Commit it with \
                 scene.transaction.commit or throw it away with scene.transaction.abort"
            );
        }
        history.transaction = Some(Vec::new());
        Ok(())
    }

    /// Commit: one patch for everything this client did, one undo step.
    pub fn commit(&self, client: Option<&str>) -> Result<Patch> {
        let at = self.seq.load(Ordering::SeqCst);
        let mut guard = self.inner.lock();
        let inner = &mut *guard;
        let history = inner.clients.of(client, at);
        let Some(pending) = history.transaction.take() else {
            bail!("you have no transaction open. Open one with scene.transaction.begin");
        };
        let mut p = fold(pending);
        if p.is_empty() {
            return Ok(p);
        }
        p.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        p.source_client = client.map(str::to_string);
        // Nothing inside the transaction went on the undo stack, so the whole
        // batch is one step: that is what "applies on one frame or not at all"
        // means for somebody pressing Ctrl+Z afterwards.
        history.redo.clear();
        history.push_undo(p.inverse());
        inner.writers.note(&p, client);
        inner.save();
        drop(guard);
        let _ = self.patches.send(p.clone());
        Ok(p)
    }

    /// Throw this client's transaction away, in one patch so every mirror
    /// follows. A record somebody else changed since is left as they left it.
    pub fn abort(&self, client: Option<&str>) -> Result<Patch> {
        let at = self.seq.load(Ordering::SeqCst);
        let mut guard = self.inner.lock();
        let inner = &mut *guard;
        let Some(pending) = inner.clients.of(client, at).transaction.take() else {
            bail!("you have no transaction open. Open one with scene.transaction.begin");
        };
        let step = fold(pending).inverse();
        let now = inner.doc.to_flat();
        let theirs = conflict::check(&now, &step, &inner.writers, client);
        let mut working = inner.doc.clone();
        apply(&mut working, &conflict::without(&step, &theirs))?;
        let mut p = patch::diff(&now, &working.to_flat());
        if p.is_empty() {
            return Ok(p);
        }
        p.seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        p.source_client = client.map(str::to_string);
        inner.doc = working;
        inner.writers.note(&p, client);
        inner.save();
        drop(guard);
        let _ = self.patches.send(p.clone());
        Ok(p)
    }

    /// True while this client has a transaction open.
    pub fn in_transaction(&self, client: Option<&str>) -> bool {
        self.inner.lock().clients.peek(client).is_some_and(|h| h.transaction.is_some())
    }
}

/// A transaction's patches as the one patch they add up to.
fn fold(pending: Vec<Patch>) -> Patch {
    let mut patches = pending.into_iter();
    let Some(mut out) = patches.next() else {
        return Patch { scope: "document".into(), ..Patch::default() };
    };
    for later in patches {
        out.merge(&later);
    }
    out.label = None;
    out.client_seq = None;
    out
}
