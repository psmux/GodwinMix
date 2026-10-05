//! One history per client.
//!
//! Several people edit one document from a phone, a laptop and the CLI at
//! once. With one undo stack for the whole core, Ctrl+Z on the laptop took
//! back whatever the phone had just done, and a transaction opened by one
//! client refused everybody else's. So everything that belongs to a person's
//! own sequence of edits lives here, keyed by the client id the control plane
//! gives every connection: the undo and redo stacks, the open `history.mark`
//! group, and the open transaction.
//!
//! A change made with no client at all (a test, the channel bridge, a project
//! being applied) keeps its own history under the empty key, exactly as the
//! single stack used to behave for it.

use std::collections::HashMap;

use super::patch::Patch;

/// How deep one client's undo stack goes. Each entry is a diff of the records
/// that changed, not a copy of the document, so this is cheap.
pub(super) const UNDO_DEPTH: usize = 200;

/// How many clients' histories are kept. A phone that reconnects with the
/// same `client_id` finds its stack again; one that never comes back is
/// forgotten once this many others have edited since.
const CLIENTS_KEPT: usize = 64;

/// Everything one client has going.
#[derive(Default)]
pub(super) struct ClientHistory {
    /// Patches that undo what this client has done, newest last.
    pub undo: Vec<Patch>,
    pub redo: Vec<Patch>,
    /// The label set with `scene.history.mark`, which merges the commands
    /// that follow into one undo step.
    pub group: Option<String>,
    /// The patches gathered while this client's transaction is open. `None`
    /// while it has none open.
    pub transaction: Option<Vec<Patch>>,
    /// The document sequence number this client last did anything at, which
    /// is what decides who is forgotten first.
    used: u64,
}

impl ClientHistory {
    /// Put a change on this client's history. True when it was held back
    /// because a transaction is open, which means nothing is published yet.
    pub fn remember(&mut self, p: Patch) -> bool {
        if let Some(pending) = self.transaction.as_mut() {
            // Inside a transaction the batch is the step. See `commit`.
            pending.push(p);
            return true;
        }
        self.redo.clear();
        let inverse = p.inverse();
        let merge =
            self.group.is_some() && self.undo.last().is_some_and(|last| last.label == p.label);
        if merge {
            // The inverse of "a then b" is "inverse of b then inverse of a",
            // so the newer inverse goes first and the older one is folded into
            // it: undoing the pair puts everything back where it started.
            let older = self.undo.pop().expect("just checked");
            let mut merged = inverse;
            merged.merge(&older);
            self.push_undo(merged);
        } else {
            self.push_undo(inverse);
        }
        false
    }

    /// One step onto the undo stack, and the stack kept to its depth.
    ///
    /// Every push goes through here. A transaction's commit used to push its
    /// one step directly and skip the trim, so a core that ran on transactions
    /// (the designer's drag does, and so does every `scene.transaction`) kept
    /// every step it had ever made.
    pub fn push_undo(&mut self, step: Patch) {
        self.undo.push(step);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
    }
}

/// Every client's history, by client id.
#[derive(Default)]
pub(super) struct Histories {
    by_client: HashMap<String, ClientHistory>,
}

impl Histories {
    /// This client's history, made on first use. `at` is the document's
    /// current sequence number, kept so the least recent is forgotten first.
    pub fn of(&mut self, client: Option<&str>, at: u64) -> &mut ClientHistory {
        let key = key(client);
        if !self.by_client.contains_key(key) {
            self.forget_one_if_full();
        }
        let history = self.by_client.entry(key.to_string()).or_default();
        history.used = history.used.max(at);
        history
    }

    /// Read without creating, for a UI asking how deep its stacks are.
    pub fn peek(&self, client: Option<&str>) -> Option<&ClientHistory> {
        self.by_client.get(key(client))
    }

    /// Make room for one more. A client with a transaction open is never the
    /// one forgotten: it is in the middle of something.
    fn forget_one_if_full(&mut self) {
        if self.by_client.len() < CLIENTS_KEPT {
            return;
        }
        let oldest = self
            .by_client
            .iter()
            .filter(|(_, h)| h.transaction.is_none())
            .min_by_key(|(_, h)| h.used)
            .map(|(k, _)| k.clone());
        if let Some(k) = oldest {
            self.by_client.remove(&k);
        }
    }
}

/// The key a client's history is filed under.
fn key(client: Option<&str>) -> &str {
    client.unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_clients_have_two_histories() {
        let mut all = Histories::default();
        all.of(Some("default.phone"), 1).push_undo(Patch::default());
        assert_eq!(all.peek(Some("default.phone")).map(|h| h.undo.len()), Some(1));
        assert!(all.peek(Some("default.laptop")).is_none(), "the laptop has done nothing");
    }

    #[test]
    fn the_least_recent_client_is_forgotten_first_and_an_open_transaction_never_is() {
        let mut all = Histories::default();
        all.of(Some("busy"), 0).transaction = Some(Vec::new());
        for n in 1..CLIENTS_KEPT as u64 {
            all.of(Some(&format!("c{n}")), n);
        }
        all.of(Some("late"), 999);
        assert!(all.peek(Some("busy")).is_some(), "a client mid transaction was forgotten");
        assert!(all.peek(Some("c1")).is_none(), "the oldest idle client should have gone");
        assert!(all.peek(Some("late")).is_some());
    }
}
