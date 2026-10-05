//! The refusal an undo or redo gets when somebody else's change is in the way.

use super::conflict::Conflict;

/// An undo or redo refused because it would overwrite somebody else's change.
#[derive(Debug, Clone)]
pub struct Refused {
    /// `undo` or `redo`, the method's own word.
    pub verb: &'static str,
    pub conflicts: Vec<Conflict>,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list: Vec<String> = self.conflicts.iter().map(Conflict::describe).collect();
        write!(
            f,
            "your last change cannot be {}ne without overwriting somebody else's work on {}. \
             Nothing was {}ne and the step stays on your stack. Send scene.{} with force: true \
             to put your version back over theirs, or change it by hand.",
            self.verb,
            list.join(", "),
            self.verb,
            self.verb
        )
    }
}

impl std::error::Error for Refused {}
